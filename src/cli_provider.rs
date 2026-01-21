//! Claude CLI-based usage provider.
//!
//! This module fetches usage data by running the Claude CLI with the `/usage` command.
//! It parses the terminal output to extract usage percentages and reset times.
//!
//! ## Implementation Details
//!
//! - Uses `pty-process` for direct PTY control (Claude CLI requires a terminal)
//! - Reads output incrementally with per-read timeouts (prevents indefinite hangs)
//! - Uses `PR_SET_PDEATHSIG` to ensure children die when parent dies
//! - Kills entire process tree on completion or timeout (handles internal forks)
//! - Parses ANSI-stripped output using pre-compiled regex patterns
//! - Implements retry mechanism for transient failures
//!
//! ## Process Cleanup Strategy
//!
//! The `claude` CLI is a launcher that internally forks (spawns Node.js). Simple
//! process group kills can fail because:
//! 1. The original launcher may exit before we kill it
//! 2. Children inherit the PGID but the group leader is dead
//! 3. `PR_SET_PDEATHSIG` is cleared on `fork()` so grandchildren don't inherit it
//!
//! To handle this, we use a multi-pronged approach:
//! 1. Try `killpg()` for the process group (catches most cases)
//! 2. Walk `/proc/<pid>/task/*/children` to find all descendants
//! 3. Kill each descendant individually
//! 4. Verify all processes are dead before continuing
//!
//! ## Timeout Strategy
//!
//! Three-level timeout approach:
//! 1. **Per-read timeout (5s)**: Prevents individual reads from blocking forever
//! 2. **Data timeout (15s)**: Maximum time to wait for complete data
//! 3. **Overall timeout (30s)**: Hard limit including startup and cleanup

use crate::api::{UsageData, UsageResponse};
use crate::provider::UsageProvider;
use async_trait::async_trait;
use chrono::{Datelike, Local, NaiveTime, TimeZone, Utc};
use nix::errno::Errno;
use nix::sys::signal::{kill, killpg, Signal};
use nix::unistd::Pid;
use pty_process::Pty;
use std::collections::HashSet;
use std::fs;
use std::sync::{Arc, LazyLock, Mutex};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

/// Pre-compiled regex for extracting percentage values like "16% used" or "40%used".
static PCT_PATTERN: LazyLock<regex_lite::Regex> = LazyLock::new(|| {
    regex_lite::Regex::new(r"(\d+(?:\.\d+)?)\s*%\s*used").expect("Invalid PCT_PATTERN regex")
});

/// Pre-compiled regex for extracting time like "1pm" or "12:59pm".
static TIME_PATTERN: LazyLock<regex_lite::Regex> = LazyLock::new(|| {
    regex_lite::Regex::new(r"(\d{1,2})(?::(\d{2}))?(am|pm)").expect("Invalid TIME_PATTERN regex")
});

/// Pre-compiled regex for extracting dates like "Jan 27".
static DATE_PATTERN: LazyLock<regex_lite::Regex> = LazyLock::new(|| {
    regex_lite::Regex::new(r"(Jan|Feb|Mar|Apr|May|Jun|Jul|Aug|Sep|Oct|Nov|Dec)\s+(\d{1,2})")
        .expect("Invalid DATE_PATTERN regex")
});

/// Overall timeout for the CLI command in seconds.
/// Claude CLI startup can be slow (loading Node.js, network calls), so we use
/// a generous timeout. The process is killed when we have enough data anyway.
const CLI_TIMEOUT_SECS: u64 = 30;

/// Timeout for individual PTY reads in seconds.
/// Prevents individual reads from blocking forever if CLI stops outputting.
const READ_TIMEOUT_SECS: u64 = 5;

/// Maximum consecutive read timeouts before giving up.
/// After this many timeouts with no new data, we assume CLI is stuck.
const MAX_CONSECUTIVE_TIMEOUTS: u32 = 3;

/// Base startup delay in milliseconds.
/// Allows Claude CLI to initialize (loads Node.js, connects to API).
/// This increases on retries via adaptive backoff.
const BASE_STARTUP_DELAY_MS: u64 = 3000;

/// Maximum startup delay in milliseconds (cap for adaptive backoff).
const MAX_STARTUP_DELAY_MS: u64 = 8000;

/// Inter-character delay for typing command (milliseconds).
/// Claude CLI autocomplete needs time to process each keystroke.
const CHAR_DELAY_MS: u64 = 30;

/// Number of retry attempts for transient failures.
const MAX_RETRIES: u32 = 2;

/// Delay between retries in milliseconds.
const RETRY_DELAY_MS: u64 = 1000;

/// Global tracking of active child process PIDs for cleanup on shutdown.
static ACTIVE_CHILDREN: Mutex<Vec<i32>> = Mutex::new(Vec::new());

/// Register a child process PID for tracking.
fn register_child(pid: i32) {
    if let Ok(mut children) = ACTIVE_CHILDREN.lock() {
        children.push(pid);
    }
}

/// Unregister a child process PID after it has been cleaned up.
fn unregister_child(pid: i32) {
    if let Ok(mut children) = ACTIVE_CHILDREN.lock() {
        children.retain(|&p| p != pid);
    }
}

/// Kill all tracked child processes. Called on application shutdown.
pub fn kill_all_children() {
    if let Ok(children) = ACTIVE_CHILDREN.lock() {
        for &pid in children.iter() {
            // Use full process tree kill for each tracked PID
            kill_process_tree(pid);
        }
    }
}

// =============================================================================
// Process Tree Management
// =============================================================================

/// Check if a process exists (is alive and not a zombie).
fn process_exists(pid: i32) -> bool {
    // Quick check with kill(0) - sends no signal, just checks existence
    if kill(Pid::from_raw(pid), None).is_err() {
        return false;
    }

    // Also check it's not a zombie by reading /proc/<pid>/stat
    let stat_path = format!("/proc/{pid}/stat");
    if let Ok(content) = fs::read_to_string(stat_path) {
        // Format: pid (comm) state ppid ...
        // State is right after the last ')' in comm field
        if let Some(last_paren) = content.rfind(')') {
            if let Some(state_char) = content[last_paren + 2..].chars().next() {
                // 'Z' = zombie, 'X' = dead - these don't count as "alive"
                if state_char == 'Z' || state_char == 'X' {
                    return false;
                }
            }
        }
        true
    } else {
        false
    }
}

/// Get direct children of a process by reading /proc/<pid>/task/<tid>/children.
fn get_children(pid: i32) -> Vec<i32> {
    let mut children = Vec::new();
    let task_dir = format!("/proc/{pid}/task");

    // Read children from all threads of the process
    if let Ok(entries) = fs::read_dir(&task_dir) {
        for entry in entries.flatten() {
            let tid = entry.file_name();
            let children_path = format!("{}/{}/children", task_dir, tid.to_string_lossy());

            if let Ok(content) = fs::read_to_string(&children_path) {
                for word in content.split_whitespace() {
                    if let Ok(child_pid) = word.parse::<i32>() {
                        children.push(child_pid);
                    }
                }
            }
        }
    }

    // Deduplicate
    children.sort_unstable();
    children.dedup();
    children
}

/// Get the process group ID of a process.
fn get_process_group(pid: i32) -> Option<i32> {
    let stat_path = format!("/proc/{pid}/stat");
    let content = fs::read_to_string(stat_path).ok()?;

    // Format: pid (comm) state ppid pgrp ...
    let last_paren = content.rfind(')')?;
    let after_comm = &content[last_paren + 2..];
    let fields: Vec<&str> = after_comm.split_whitespace().collect();

    // fields[0] = state, fields[1] = ppid, fields[2] = pgrp
    fields.get(2)?.parse().ok()
}

/// Collect all descendant PIDs of a process (children, grandchildren, etc.).
fn collect_descendants(pid: i32) -> HashSet<i32> {
    let mut descendants = HashSet::new();
    let mut to_visit = vec![pid];

    while let Some(current_pid) = to_visit.pop() {
        let children = get_children(current_pid);
        for child in children {
            if descendants.insert(child) {
                // Only visit if we haven't seen this PID before
                to_visit.push(child);
            }
        }
    }

    descendants
}

/// Find orphaned processes that were in the same process group.
/// When the original parent dies, children are reparented to init but keep their PGID.
fn find_orphans_by_pgid(original_pgid: i32) -> HashSet<i32> {
    let mut orphans = HashSet::new();

    if let Ok(entries) = fs::read_dir("/proc") {
        for entry in entries.flatten() {
            if let Ok(pid) = entry.file_name().to_string_lossy().parse::<i32>() {
                if let Some(pgrp) = get_process_group(pid) {
                    if pgrp == original_pgid && process_exists(pid) {
                        orphans.insert(pid);
                    }
                }
            }
        }
    }

    orphans
}

/// Send a signal to a process. Returns Ok(()) if signal sent or process doesn't exist.
fn send_signal(pid: i32, signal: Signal) -> Result<(), Errno> {
    match kill(Pid::from_raw(pid), Some(signal)) {
        Ok(()) | Err(Errno::ESRCH) => Ok(()), // ESRCH = process doesn't exist, which is fine
        Err(e) => Err(e),
    }
}

/// Kill a process and ALL its descendants reliably.
///
/// Uses a multi-pronged approach:
/// 1. Try to kill the process group (catches most cases)
/// 2. Walk /proc to find all descendants and kill them
/// 3. Find orphans by PGID and kill them
/// 4. Verify all processes are dead
fn kill_process_tree(root_pid: i32) {
    // The PID we spawned should be the process group leader (due to setpgid(0,0))
    // So the process group ID equals the root PID
    let process_group = root_pid;

    // Step 1: Collect all processes to kill BEFORE sending signals
    // This minimizes race conditions with forking
    let mut all_pids: HashSet<i32> = HashSet::new();
    all_pids.insert(root_pid);

    // Add descendants from /proc
    all_pids.extend(collect_descendants(root_pid));

    // Add orphans that share our process group (in case parent already exited)
    all_pids.extend(find_orphans_by_pgid(process_group));

    // Step 2: Send SIGTERM to process group first (most efficient)
    if let Err(e) = killpg(Pid::from_raw(process_group), Signal::SIGTERM) {
        if e != Errno::ESRCH {
            eprintln!("[cleanup] killpg({process_group}, SIGTERM) failed: {e}");
        }
    }

    // Also send SIGTERM to each individual PID (in case they escaped the group)
    for &p in &all_pids {
        let _ = send_signal(p, Signal::SIGTERM);
    }

    // Step 3: Wait briefly for graceful shutdown
    std::thread::sleep(Duration::from_millis(100));

    // Step 4: Check who's still alive and collect any new children
    let mut still_alive: HashSet<i32> = all_pids
        .iter()
        .copied()
        .filter(|&p| process_exists(p))
        .collect();

    // Re-scan for new descendants (in case of racing forks)
    for &p in &all_pids {
        still_alive.extend(
            collect_descendants(p)
                .into_iter()
                .filter(|&c| process_exists(c)),
        );
    }
    still_alive.extend(
        find_orphans_by_pgid(process_group)
            .into_iter()
            .filter(|&p| process_exists(p)),
    );

    if still_alive.is_empty() {
        return;
    }

    // Step 5: Send SIGKILL to process group
    if let Err(e) = killpg(Pid::from_raw(process_group), Signal::SIGKILL) {
        if e != Errno::ESRCH {
            eprintln!("[cleanup] killpg({process_group}, SIGKILL) failed: {e}");
        }
    }

    // Send SIGKILL to each individual process
    for &p in &still_alive {
        if let Err(e) = send_signal(p, Signal::SIGKILL) {
            eprintln!("[cleanup] kill({p}, SIGKILL) failed: {e}");
        }
    }

    // Step 6: Verify all processes are dead (with timeout)
    let deadline = std::time::Instant::now() + Duration::from_secs(2);
    loop {
        // Re-check who's still alive
        still_alive.retain(|&p| process_exists(p));

        // Also check for any new orphans
        let new_orphans: Vec<i32> = find_orphans_by_pgid(process_group)
            .into_iter()
            .filter(|&p| process_exists(p))
            .collect();

        for orphan in new_orphans {
            if still_alive.insert(orphan) {
                // New orphan found, kill it
                let _ = send_signal(orphan, Signal::SIGKILL);
            }
        }

        if still_alive.is_empty() {
            break;
        }

        if std::time::Instant::now() >= deadline {
            eprintln!(
                "[cleanup] Warning: {} processes still alive after SIGKILL: {:?}",
                still_alive.len(),
                still_alive
            );
            break;
        }

        std::thread::sleep(Duration::from_millis(50));
    }
}

/// RAII guard that ensures process cleanup happens even on early returns.
///
/// When this guard is dropped, it will:
/// 1. Kill the entire process tree
/// 2. Wait for the child process to be reaped
/// 3. Unregister from global tracking
struct ProcessGuard {
    pid: i32,
    child: Option<tokio::process::Child>,
}

impl ProcessGuard {
    fn new(pid: i32, child: tokio::process::Child) -> Self {
        register_child(pid);
        Self {
            pid,
            child: Some(child),
        }
    }

    /// Take ownership of the child for explicit waiting.
    fn take_child(&mut self) -> Option<tokio::process::Child> {
        self.child.take()
    }
}

impl Drop for ProcessGuard {
    fn drop(&mut self) {
        // Kill the entire process tree
        kill_process_tree(self.pid);

        // Try to reap the child if we still own it
        if let Some(mut child) = self.child.take() {
            // Spawn a background thread to reap the child WITHOUT blocking
            // This prevents zombie processes while not blocking the async runtime.
            // We don't join() the thread - it runs in the background and the OS
            // will clean it up when it finishes.
            std::thread::spawn(move || {
                // Give a short time for process to exit after SIGKILL
                std::thread::sleep(Duration::from_millis(100));
                // Use try_wait in a loop to avoid blocking forever
                for _ in 0..20 {
                    match child.try_wait() {
                        Ok(None) => std::thread::sleep(Duration::from_millis(50)),
                        Ok(Some(_)) | Err(_) => break,
                    }
                }
            });
        }

        unregister_child(self.pid);
    }
}

/// Result from reading PTY output.
#[derive(Debug)]
enum ReadResult {
    /// Successfully read complete usage data.
    Complete(String),
    /// Read timed out with partial data.
    Timeout(String),
    /// PTY closed (EOF) with whatever data was received.
    Eof(String),
    /// IO error occurred.
    Error(std::io::Error),
}

/// Read PTY output until we have complete usage data or timeout.
///
/// Uses per-read timeouts to prevent indefinite blocking. Returns early when
/// we detect all required sections have been received, avoiding unnecessary
/// waiting for process exit.
///
/// # Timeout Strategy
/// - Each individual read has a 5-second timeout
/// - After 3 consecutive timeouts with no new data, we give up
/// - This prevents hangs when CLI stops producing output but doesn't close
async fn read_pty_output(
    mut reader: pty_process::OwnedReadPty,
    cancel_flag: Arc<std::sync::atomic::AtomicBool>,
) -> ReadResult {
    let mut output = String::new();
    let mut buf = [0u8; 1024];

    // Track which sections we've seen
    let mut seen_session = false;
    let mut seen_weekly = false;
    let mut consecutive_timeouts = 0u32;
    let mut total_bytes = 0usize;

    let start_time = std::time::Instant::now();

    loop {
        // Check if we've been asked to cancel
        if cancel_flag.load(std::sync::atomic::Ordering::Relaxed) {
            eprintln!(
                "[read] Cancelled after {:.1}s, {} bytes",
                start_time.elapsed().as_secs_f32(),
                total_bytes
            );
            return ReadResult::Timeout(output);
        }

        // Use per-read timeout to prevent indefinite blocking
        let read_result = tokio::time::timeout(
            Duration::from_secs(READ_TIMEOUT_SECS),
            reader.read(&mut buf),
        )
        .await;

        match read_result {
            // Timeout on this read
            Err(_) => {
                consecutive_timeouts += 1;
                eprintln!(
                    "[read] Timeout #{consecutive_timeouts}/{MAX_CONSECUTIVE_TIMEOUTS} after {:.1}s, {} bytes so far",
                    start_time.elapsed().as_secs_f32(),
                    total_bytes
                );

                // Check if we already have enough data
                if has_complete_data(&output) {
                    eprintln!("[read] Have complete data despite timeout, returning");
                    return ReadResult::Complete(output);
                }

                // Give up after too many consecutive timeouts
                if consecutive_timeouts >= MAX_CONSECUTIVE_TIMEOUTS {
                    eprintln!(
                        "[read] Too many timeouts ({consecutive_timeouts}), giving up with {total_bytes} bytes"
                    );
                    return ReadResult::Timeout(output);
                }
            }

            // EOF
            Ok(Ok(0)) => {
                eprintln!(
                    "[read] EOF after {:.1}s, {} bytes",
                    start_time.elapsed().as_secs_f32(),
                    total_bytes
                );
                return ReadResult::Eof(output);
            }

            // Successful read with data
            Ok(Ok(n)) => {
                consecutive_timeouts = 0; // Reset timeout counter on successful read
                total_bytes += n;

                let chunk = String::from_utf8_lossy(&buf[..n]);
                output.push_str(&chunk);

                // Strip ANSI codes for pattern matching (raw output may have escape sequences)
                let clean_chunk = strip_ansi_codes(&chunk);
                let clean_output = strip_ansi_codes(&output);

                // Check if we've seen the required sections
                if clean_chunk.contains("Current session") || clean_chunk.contains("session") {
                    seen_session = true;
                }
                if clean_chunk.contains("Current week") || clean_chunk.contains("week (all") {
                    seen_weekly = true;
                }

                // Debug: log progress periodically
                if total_bytes % 500 < n {
                    eprintln!(
                        "[read] Progress: {total_bytes} bytes, session={seen_session}, weekly={seen_weekly}"
                    );
                }

                // If we've seen both required sections and have percentage data
                if seen_session && seen_weekly && clean_output.contains("% used") {
                    let pct_count = clean_output.matches("% used").count();
                    if pct_count >= 2 {
                        eprintln!(
                            "[read] Found {pct_count} percentages after {:.1}s, {} bytes",
                            start_time.elapsed().as_secs_f32(),
                            total_bytes
                        );
                        // Give a tiny bit more time to capture any trailing output
                        tokio::time::sleep(Duration::from_millis(100)).await;
                        // Try to read any remaining buffered data
                        drain_remaining(&mut reader, &mut output, &mut buf).await;
                        return ReadResult::Complete(output);
                    }
                }
            }

            // Read error
            Ok(Err(e)) => {
                eprintln!(
                    "[read] Error after {:.1}s: {e}",
                    start_time.elapsed().as_secs_f32()
                );
                return ReadResult::Error(e);
            }
        }
    }
}

/// Check if we have all the data we need.
fn has_complete_data(output: &str) -> bool {
    let clean = strip_ansi_codes(output);
    let has_session = clean.contains("Current session") || clean.contains("session");
    let has_weekly = clean.contains("Current week") || clean.contains("week (all");
    let pct_count = clean.matches("% used").count();
    has_session && has_weekly && pct_count >= 2
}

/// Drain any remaining buffered data from the PTY.
async fn drain_remaining(
    reader: &mut pty_process::OwnedReadPty,
    output: &mut String,
    buf: &mut [u8],
) {
    loop {
        match tokio::time::timeout(Duration::from_millis(50), reader.read(buf)).await {
            Ok(Ok(n)) if n > 0 => {
                output.push_str(&String::from_utf8_lossy(&buf[..n]));
            }
            _ => break, // EOF, error, or timeout
        }
    }
}

/// Usage provider that fetches data from the Claude CLI.
///
/// Runs `echo '/usage' | claude` and parses the output to extract
/// usage statistics including percentages and reset times.
pub struct ClaudeCliProvider;

impl ClaudeCliProvider {
    pub fn new() -> Self {
        Self
    }
}

impl Default for ClaudeCliProvider {
    fn default() -> Self {
        Self::new()
    }
}

/// Calculate adaptive startup delay based on attempt number.
/// Uses exponential backoff: 2s, 4s, 8s (capped at MAX_STARTUP_DELAY_MS)
fn calculate_startup_delay(attempt: u32) -> u64 {
    let delay = BASE_STARTUP_DELAY_MS * (1 << attempt); // 2^attempt multiplier
    delay.min(MAX_STARTUP_DELAY_MS)
}

#[async_trait]
impl UsageProvider for ClaudeCliProvider {
    async fn fetch_usage(&self) -> Result<UsageResponse, String> {
        // Implement retry mechanism with exponential backoff
        let mut last_error = String::new();

        for attempt in 0..=MAX_RETRIES {
            if attempt > 0 {
                eprintln!("[fetch] Retry attempt {attempt}/{MAX_RETRIES} after {RETRY_DELAY_MS}ms");
                tokio::time::sleep(Duration::from_millis(RETRY_DELAY_MS)).await;
            }

            // Calculate adaptive startup delay for this attempt
            let startup_delay = calculate_startup_delay(attempt);

            match self.fetch_usage_once(startup_delay).await {
                Ok(usage) => return Ok(usage),
                Err(e) => {
                    eprintln!("[fetch] Attempt {attempt} failed: {e}");
                    last_error = e;

                    // Don't retry on parse errors (they won't magically fix themselves)
                    if last_error.contains("Could not find") {
                        break;
                    }
                }
            }
        }

        Err(last_error)
    }

    fn name(&self) -> &'static str {
        "claude-cli"
    }
}

impl ClaudeCliProvider {
    /// Single attempt to fetch usage data with configurable startup delay.
    #[allow(clippy::too_many_lines)]
    async fn fetch_usage_once(&self, startup_delay_ms: u64) -> Result<UsageResponse, String> {
        // Run Claude CLI with /usage command using pty-process for direct PTY control.
        // The claude CLI doesn't exit after /usage, so we read output incrementally
        // and kill the process when we have enough data or timeout.
        //
        // Strategy:
        // 1. Create PTY and spawn process with PR_SET_PDEATHSIG
        // 2. Make process its own process group leader
        // 3. Use ProcessGuard for RAII cleanup (handles early returns)
        // 4. Read PTY output with per-read timeouts
        // 5. Kill entire process tree when done or on timeout

        let start_time = std::time::Instant::now();

        // Create PTY
        let pty = Pty::new().map_err(|e| format!("Failed to create PTY: {e}"))?;
        pty.resize(pty_process::Size::new(24, 80))
            .map_err(|e| format!("Failed to resize PTY: {e}"))?;

        // Get pts for spawning
        let pts = pty
            .pts()
            .map_err(|e| format!("Failed to get PTY pts: {e}"))?;

        // Build command with process group isolation and death signal
        let mut cmd = pty_process::Command::new("claude");
        unsafe {
            cmd.pre_exec(|| {
                // Create new process group (child becomes leader)
                libc::setpgid(0, 0);
                // Die when parent dies (prevents orphans if tracker crashes)
                // Note: This is cleared on fork(), so grandchildren won't inherit it
                libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL);
                Ok(())
            });
        }

        // Spawn child on PTY
        let child = cmd
            .spawn(&pts)
            .map_err(|e| format!("Failed to spawn claude CLI: {e}"))?;

        let pid = child.id().ok_or("Failed to get child PID")?.cast_signed();
        eprintln!("[fetch] Spawned claude CLI with PID {pid}");

        // Create RAII guard - this ensures cleanup happens even on early returns
        // The guard will kill the entire process tree when dropped
        let mut guard = ProcessGuard::new(pid, child);

        // Split PTY into reader and writer
        let (reader, mut writer) = pty.into_split();

        // Create cancel flag for cooperative cancellation
        let cancel_flag = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let cancel_flag_clone = Arc::clone(&cancel_flag);

        // Start reading concurrently
        // The CLI needs to receive input while we're reading its output
        let read_task =
            tokio::spawn(async move { read_pty_output(reader, cancel_flag_clone).await });

        // Wait for Claude CLI to start up and show its prompt
        // Uses adaptive delay that increases on retries
        eprintln!("[fetch] Waiting {startup_delay_ms}ms for CLI startup...");
        tokio::time::sleep(Duration::from_millis(startup_delay_ms)).await;

        // Write /usage command character by character
        // Claude CLI has autocomplete that needs time to process each keystroke.
        // If we send the whole command at once, it shows autocomplete suggestions
        // instead of executing the command.
        eprintln!("[fetch] Sending /usage command...");
        for c in b"/usage" {
            writer
                .write_all(&[*c])
                .await
                .map_err(|e| format!("Failed to write to PTY: {e}"))?;
            writer
                .flush()
                .await
                .map_err(|e| format!("Failed to flush PTY: {e}"))?;
            tokio::time::sleep(Duration::from_millis(CHAR_DELAY_MS)).await;
        }

        // Small delay then send Enter
        tokio::time::sleep(Duration::from_millis(100)).await;
        writer
            .write_all(b"\r")
            .await
            .map_err(|e| format!("Failed to write Enter to PTY: {e}"))?;
        writer
            .flush()
            .await
            .map_err(|e| format!("Failed to flush PTY: {e}"))?;
        eprintln!("[fetch] Command sent, waiting for response...");

        // Wait for read task with overall timeout
        let result = tokio::time::timeout(Duration::from_secs(CLI_TIMEOUT_SECS), read_task).await;

        // Signal cancellation to read task (in case timeout fires)
        cancel_flag.store(true, std::sync::atomic::Ordering::Relaxed);

        // Explicit cleanup: kill process tree and wait for child
        // This is more thorough than just letting the guard drop
        eprintln!("[fetch] Cleaning up process tree...");
        kill_process_tree(pid);

        // Take child from guard and wait for it
        if let Some(mut child) = guard.take_child() {
            // Give processes time to exit after SIGKILL
            tokio::time::sleep(Duration::from_millis(100)).await;
            let _ = child.wait().await;
        }

        let elapsed = start_time.elapsed();
        eprintln!("[fetch] Total time: {:.1}s", elapsed.as_secs_f32());

        // Guard will be dropped here, but since we already cleaned up,
        // it won't do much (child is already taken)

        // Handle the result
        let output = match result {
            // Overall timeout expired
            Err(_) => {
                return Err(format!(
                    "CLI command timed out after {CLI_TIMEOUT_SECS}s (overall timeout)"
                ));
            }
            // Read task panicked
            Ok(Err(e)) => {
                return Err(format!("Read task panicked: {e}"));
            }
            // Read task completed - check the result
            Ok(Ok(read_result)) => match read_result {
                ReadResult::Complete(data) => {
                    eprintln!("[fetch] Got complete data ({} bytes)", data.len());
                    data
                }
                ReadResult::Eof(data) => {
                    eprintln!("[fetch] Got EOF with {} bytes", data.len());
                    // CLI closed - try to parse what we got
                    if has_complete_data(&data) {
                        data
                    } else {
                        return Err(format!(
                            "CLI closed early with incomplete data ({} bytes)",
                            data.len()
                        ));
                    }
                }
                ReadResult::Timeout(data) => {
                    eprintln!(
                        "[fetch] Read timed out with {} bytes (per-read timeout)",
                        data.len()
                    );
                    // Per-read timeout - try to parse what we got
                    if has_complete_data(&data) {
                        data
                    } else {
                        return Err(format!(
                            "CLI read timed out with incomplete data ({} bytes)",
                            data.len()
                        ));
                    }
                }
                ReadResult::Error(e) => {
                    return Err(format!("Failed to read CLI output: {e}"));
                }
            },
        };

        // Parse the output
        parse_usage_output(&output)
    }
}

/// Safely truncate a string at a valid UTF-8 character boundary.
///
/// Prevents panics when slicing multi-byte UTF-8 characters.
fn safe_truncate(s: &str, max_bytes: usize) -> &str {
    if s.len() <= max_bytes {
        return s;
    }
    // Find a valid char boundary at or before max_bytes
    let mut end = max_bytes;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}

/// Parse the Claude CLI /usage output into structured usage data.
///
/// Extracts session (5h), weekly (7d), and optional Sonnet usage from the CLI output.
fn parse_usage_output(output: &str) -> Result<UsageResponse, String> {
    // Strip ANSI codes for easier parsing
    let clean = strip_ansi_codes(output);

    // Truncate output for error messages (max 500 chars)
    let truncated_output = if clean.len() > 500 {
        format!("{}...", safe_truncate(&clean, 500))
    } else {
        clean.clone()
    };

    // Parse session usage (5h)
    let five_hour = parse_usage_section(&clean, "Current session")
        .or_else(|| parse_usage_section(&clean, "session"))
        .ok_or_else(|| {
            format!("Could not find session usage data. CLI output:\n{truncated_output}")
        })?;

    // Parse weekly usage (7d) - "Current week (all models)"
    let seven_day = parse_usage_section(&clean, "Current week (all models)")
        .or_else(|| parse_usage_section(&clean, "week (all"))
        .ok_or_else(|| {
            format!("Could not find weekly usage data. CLI output:\n{truncated_output}")
        })?;

    // Parse Opus/Sonnet specific (optional)
    let seven_day_opus = parse_usage_section(&clean, "Current week (Sonnet only)")
        .or_else(|| parse_usage_section(&clean, "Sonnet only"));

    Ok(UsageResponse {
        five_hour,
        seven_day,
        seven_day_opus,
    })
}

/// Remove ANSI escape sequences from terminal output.
fn strip_ansi_codes(s: &str) -> String {
    // Remove ANSI escape sequences
    let mut result = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '\x1b' {
            // Skip until we hit a letter (end of escape sequence)
            while let Some(&next) = chars.peek() {
                chars.next();
                if next.is_ascii_alphabetic() {
                    break;
                }
            }
        } else if c == '\x07' || c == '\x00' {
            // Skip bell and null characters
        } else {
            result.push(c);
        }
    }

    result
}

fn parse_usage_section(text: &str, section_name: &str) -> Option<UsageData> {
    // Find the section
    let section_start = text.find(section_name)?;
    let after_section = &text[section_start..];

    // Limit section text to avoid including the next section
    // Look for "Current" which marks the next section (e.g., "Current week")
    let section_text = if section_name.contains("session") {
        // For session, stop at "Current week" or limit to 300 chars
        after_section.find("Current week").map_or_else(
            || safe_truncate(after_section, 300),
            |pos| &after_section[..pos],
        )
    } else if section_name.contains("week (all") {
        // For weekly (all models), stop at "Sonnet only" or limit to 300 chars
        after_section.find("Sonnet only").map_or_else(
            || safe_truncate(after_section, 300),
            |pos| &after_section[..pos],
        )
    } else {
        // For other sections, limit to 300 chars
        safe_truncate(after_section, 300)
    };

    // Look for percentage pattern like "16%" or "20%"
    let utilization = extract_percentage(section_text)?;

    // Look for reset time
    let resets_at = extract_reset_time(section_text);

    Some(UsageData {
        utilization,
        resets_at,
    })
}

fn extract_percentage(text: &str) -> Option<f64> {
    // Search in first 500 chars (enough to find the percentage for this section)
    let search_text = safe_truncate(text, 500);

    if let Some(caps) = PCT_PATTERN.captures(search_text) {
        if let Some(num_match) = caps.get(1) {
            if let Ok(pct) = num_match.as_str().parse::<f64>() {
                return Some(pct);
            }
        }
    }

    None
}

fn extract_reset_time(text: &str) -> Option<chrono::DateTime<Utc>> {
    // Search within first 500 chars for reset time
    // The stripped output may have everything on one line
    let search_text = safe_truncate(text, 500);

    // Look for "Reset" (handles both "Resets" and partial matches like "Reses" from terminal artifacts)
    // and then parse the time that follows
    if search_text.contains("Reset") || search_text.contains("Rese") {
        return parse_reset_time_line(search_text);
    }

    None
}

fn parse_reset_time_line(line: &str) -> Option<chrono::DateTime<Utc>> {
    let now = Local::now();

    // Extract time part - handles both "1pm" and "12:59pm" formats
    if let Some(caps) = TIME_PATTERN.captures(line) {
        let hour: u32 = caps.get(1)?.as_str().parse().ok()?;
        let minutes: u32 = caps.get(2).map_or(0, |m| m.as_str().parse().unwrap_or(0));
        let ampm = caps.get(3)?.as_str();

        let hour_24 = if ampm == "pm" && hour != 12 {
            hour + 12
        } else if ampm == "am" && hour == 12 {
            0
        } else {
            hour
        };

        let time = NaiveTime::from_hms_opt(hour_24, minutes, 0)?;

        // Check if there's a date (e.g., "Jan 27")
        let date = if let Some(date_caps) = DATE_PATTERN.captures(line) {
            let month_str = date_caps.get(1)?.as_str();
            let day: u32 = date_caps.get(2)?.as_str().parse().ok()?;

            let month = match month_str {
                "Jan" => 1,
                "Feb" => 2,
                "Mar" => 3,
                "Apr" => 4,
                "May" => 5,
                "Jun" => 6,
                "Jul" => 7,
                "Aug" => 8,
                "Sep" => 9,
                "Oct" => 10,
                "Nov" => 11,
                "Dec" => 12,
                _ => return None,
            };

            // Use current year, but handle year boundary
            let year = if month < now.month() {
                now.year() + 1
            } else {
                now.year()
            };

            chrono::NaiveDate::from_ymd_opt(year, month, day)?
        } else {
            // No date specified, assume today or tomorrow
            let today = now.date_naive();
            let target_time = today.and_time(time);

            if target_time <= now.naive_local() {
                today + chrono::Duration::days(1)
            } else {
                today
            }
        };

        let datetime = date.and_time(time);

        // Convert from local time to UTC
        let local_dt = Local.from_local_datetime(&datetime).single()?;
        return Some(local_dt.with_timezone(&Utc));
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    // =============================================================================
    // Data Completeness Tests
    // =============================================================================

    #[test]
    fn test_has_complete_data_with_all_sections() {
        let data = "Current session 16% used Current week (all models) 20% used";
        assert!(has_complete_data(data));
    }

    #[test]
    fn test_has_complete_data_missing_session() {
        let data = "Current week (all models) 20% used 30% used";
        assert!(!has_complete_data(data));
    }

    #[test]
    fn test_has_complete_data_missing_weekly() {
        let data = "Current session 16% used";
        assert!(!has_complete_data(data));
    }

    #[test]
    fn test_has_complete_data_only_one_percentage() {
        let data = "Current session Current week (all models) 16% used";
        assert!(!has_complete_data(data));
    }

    #[test]
    fn test_has_complete_data_with_fallback_patterns() {
        // Uses "session" and "week (all" fallback patterns
        let data = "session 16% used week (all 20% used";
        assert!(has_complete_data(data));
    }

    #[test]
    fn test_has_complete_data_empty() {
        assert!(!has_complete_data(""));
    }

    #[test]
    fn test_has_complete_data_partial_output() {
        // Simulates incomplete data that might come from timeout
        let data = "Current session\n████████  16% used\nResets";
        assert!(!has_complete_data(data));
    }

    // =============================================================================
    // ANSI Code Stripping Tests
    // =============================================================================

    #[test]
    fn test_strip_ansi_codes() {
        let input = "\x1b[38;2;215;119;87mHello\x1b[0m World";
        let result = strip_ansi_codes(input);
        assert!(result.contains("Hello"));
        assert!(result.contains("World"));
        assert!(!result.contains('\x1b'));
    }

    #[test]
    fn test_strip_ansi_codes_complex() {
        // Multiple escape sequences, bell, and null chars
        let input = "\x1b[1m\x1b[32mGreen\x1b[0m\x07 \x00Normal";
        let result = strip_ansi_codes(input);
        assert!(result.contains("Green"));
        assert!(result.contains("Normal"));
        assert!(!result.contains('\x07'));
        assert!(!result.contains('\x00'));
    }

    #[test]
    fn test_strip_ansi_codes_preserves_content() {
        let input = "No ANSI codes here";
        let result = strip_ansi_codes(input);
        assert_eq!(result, input);
    }

    // =============================================================================
    // Percentage Extraction Tests
    // =============================================================================

    #[test]
    fn test_extract_percentage() {
        let text = "███████████ 16% used";
        let pct = extract_percentage(text);
        assert!(pct.is_some());
        assert!((pct.unwrap() - 16.0).abs() < 0.1);
    }

    #[test]
    fn test_extract_percentage_20() {
        let text = "██████████ 20% used\nResets Jan 27";
        let pct = extract_percentage(text);
        assert!(pct.is_some());
        assert!((pct.unwrap() - 20.0).abs() < 0.1);
    }

    #[test]
    fn test_extract_percentage_no_space() {
        // CLI sometimes outputs "40%used" without space
        let text = "████████████████████                              40%used";
        let pct = extract_percentage(text);
        assert!(pct.is_some());
        assert!((pct.unwrap() - 40.0).abs() < 0.1);
    }

    #[test]
    fn test_extract_percentage_decimal() {
        let text = "16.5% used";
        let pct = extract_percentage(text);
        assert!(pct.is_some());
        assert!((pct.unwrap() - 16.5).abs() < 0.01);
    }

    #[test]
    fn test_extract_percentage_zero() {
        let text = "0% used";
        let pct = extract_percentage(text);
        assert!(pct.is_some());
        assert!((pct.unwrap() - 0.0).abs() < 0.01);
    }

    #[test]
    fn test_extract_percentage_hundred() {
        let text = "100% used";
        let pct = extract_percentage(text);
        assert!(pct.is_some());
        assert!((pct.unwrap() - 100.0).abs() < 0.01);
    }

    #[test]
    fn test_extract_percentage_not_found() {
        let text = "No percentage here";
        let pct = extract_percentage(text);
        assert!(pct.is_none());
    }

    // =============================================================================
    // Section Parsing Tests
    // =============================================================================

    #[test]
    fn test_parse_usage_section() {
        let text = "Current session    \n████████   16% used  \nResets 1pm";
        let usage = parse_usage_section(text, "Current session");
        assert!(usage.is_some());
        let usage = usage.unwrap();
        assert!((usage.utilization - 16.0).abs() < 0.1);
    }

    #[test]
    fn test_parse_usage_section_boundary() {
        // Test that section parsing stops at next section
        let text = "Current session 10% used Current week (all models) 50% used";
        let usage = parse_usage_section(text, "Current session");
        assert!(usage.is_some());
        let usage = usage.unwrap();
        // Should find 10%, not 50%
        assert!((usage.utilization - 10.0).abs() < 0.1);
    }

    #[test]
    fn test_parse_usage_section_not_found() {
        let text = "Some random text without usage data";
        let usage = parse_usage_section(text, "Current session");
        assert!(usage.is_none());
    }

    // =============================================================================
    // Full Output Parsing Tests
    // =============================================================================

    #[test]
    fn test_parse_full_output() {
        let sample = r"
Current session
████████                                          16% used
Resets 1pm (Asia/Tokyo)

Current week (all models)
██████████                                        20% used
Resets Jan 27, 10am (Asia/Tokyo)

Current week (Sonnet only)
██                                                4% used
Resets 11am (Asia/Tokyo)
";
        let result = parse_usage_output(sample);
        assert!(result.is_ok());
        let usage = result.unwrap();
        assert!((usage.five_hour.utilization - 16.0).abs() < 0.1);
        assert!((usage.seven_day.utilization - 20.0).abs() < 0.1);
        assert!(usage.seven_day_opus.is_some());
        assert!((usage.seven_day_opus.unwrap().utilization - 4.0).abs() < 0.1);
    }

    #[test]
    fn test_parse_output_minimal() {
        // Minimal output with just session and weekly
        let sample = "Current session 16% used Current week (all models) 20% used";
        let result = parse_usage_output(sample);
        assert!(result.is_ok());
        let usage = result.unwrap();
        assert!((usage.five_hour.utilization - 16.0).abs() < 0.1);
        assert!((usage.seven_day.utilization - 20.0).abs() < 0.1);
        assert!(usage.seven_day_opus.is_none());
    }

    #[test]
    fn test_parse_output_with_ansi() {
        // Output with ANSI escape codes
        let sample = "\x1b[1mCurrent session\x1b[0m 16% used \x1b[32mCurrent week (all models)\x1b[0m 20% used";
        let result = parse_usage_output(sample);
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_output_missing_session() {
        let sample = "Current week (all models) 20% used";
        let result = parse_usage_output(sample);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("session"));
    }

    #[test]
    fn test_parse_output_missing_weekly() {
        let sample = "Current session 16% used";
        let result = parse_usage_output(sample);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("weekly"));
    }

    // =============================================================================
    // Safe Truncate Tests
    // =============================================================================

    #[test]
    fn test_safe_truncate_ascii() {
        let s = "Hello World";
        assert_eq!(safe_truncate(s, 5), "Hello");
        assert_eq!(safe_truncate(s, 100), s);
    }

    #[test]
    fn test_safe_truncate_utf8() {
        let s = "Hello 世界";
        // "世" is 3 bytes, so truncating at 7 should give "Hello "
        let truncated = safe_truncate(s, 7);
        assert!(truncated.is_char_boundary(truncated.len()));
        assert_eq!(truncated, "Hello ");
    }

    #[test]
    fn test_safe_truncate_emoji() {
        let s = "Hi 👋";
        // emoji is 4 bytes, so truncating in the middle should back up
        let truncated = safe_truncate(s, 4);
        assert!(truncated.is_char_boundary(truncated.len()));
        assert_eq!(truncated, "Hi ");
    }

    // =============================================================================
    // Process Helper Tests (where possible without actual processes)
    // =============================================================================

    #[test]
    fn test_process_exists_nonexistent() {
        // PID 1 (init) should exist, but a very high PID likely doesn't
        assert!(!process_exists(999_999_999));
    }

    #[test]
    fn test_get_children_nonexistent() {
        // Non-existent process should return empty vec
        let children = get_children(999_999_999);
        assert!(children.is_empty());
    }

    #[test]
    fn test_get_process_group_nonexistent() {
        assert!(get_process_group(999_999_999).is_none());
    }

    #[test]
    fn test_collect_descendants_nonexistent() {
        let descendants = collect_descendants(999_999_999);
        assert!(descendants.is_empty());
    }

    #[test]
    fn test_find_orphans_nonexistent_pgid() {
        // A PGID that likely doesn't exist
        let orphans = find_orphans_by_pgid(999_999_999);
        assert!(orphans.is_empty());
    }

    // =============================================================================
    // ReadResult Tests
    // =============================================================================

    #[test]
    fn test_read_result_debug() {
        // Ensure ReadResult implements Debug correctly
        let result = ReadResult::Complete("test".to_string());
        let debug_str = format!("{result:?}");
        assert!(debug_str.contains("Complete"));
    }

    // =============================================================================
    // Backoff Tests
    // =============================================================================

    #[test]
    fn test_calculate_startup_delay_exponential() {
        // Attempt 0: 3000ms base
        assert_eq!(calculate_startup_delay(0), 3000);
        // Attempt 1: 6000ms (2^1 * 3000)
        assert_eq!(calculate_startup_delay(1), 6000);
        // Attempt 2: 8000ms (2^2 * 3000 = 12000, capped at MAX)
        assert_eq!(calculate_startup_delay(2), 8000);
        // Attempt 3: still 8000ms (capped)
        assert_eq!(calculate_startup_delay(3), 8000);
    }
}
