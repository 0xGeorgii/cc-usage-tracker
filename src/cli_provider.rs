//! Claude CLI-based usage provider.
//!
//! This module fetches usage data by running the Claude CLI with the `/usage` command.
//! It parses the terminal output to extract usage percentages and reset times.
//!
//! ## Implementation Details
//!
//! - Uses `script` for PTY emulation (Claude CLI requires a terminal)
//! - Nested timeouts: inner `timeout` command + outer tokio timeout
//! - Parses ANSI-stripped output using pre-compiled regex patterns

use crate::api::{UsageData, UsageResponse};
use crate::provider::UsageProvider;
use async_trait::async_trait;
use chrono::{Datelike, Local, NaiveTime, TimeZone, Utc};
use std::sync::LazyLock;
use std::time::Duration;
use tokio::process::Command;

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

/// Inner timeout for the CLI command (claude doesn't exit after /usage).
const CLI_INNER_TIMEOUT_SECS: u64 = 8;
/// Outer timeout as a safety net for the entire operation.
const CLI_OUTER_TIMEOUT_SECS: u64 = 15;

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

#[async_trait]
impl UsageProvider for ClaudeCliProvider {
    async fn fetch_usage(&self) -> Result<UsageResponse, String> {
        // Run Claude CLI with /usage command using script for PTY emulation
        // The claude CLI doesn't exit after /usage, so we use timeout to kill it
        // after the usage data has been displayed (typically within 3-5 seconds)
        let inner_cmd =
            format!("timeout {CLI_INNER_TIMEOUT_SECS} sh -c \"echo '/usage' | claude\"");

        let result = tokio::time::timeout(
            Duration::from_secs(CLI_OUTER_TIMEOUT_SECS),
            Command::new("script")
                .args(["-q", "-c", &inner_cmd, "/dev/null"])
                .output(),
        )
        .await;

        let output = match result {
            Ok(Ok(output)) => output,
            Ok(Err(e)) => return Err(format!("Failed to run claude CLI: {e}")),
            Err(_) => {
                return Err(format!(
                    "CLI command timed out after {CLI_OUTER_TIMEOUT_SECS}s"
                ))
            }
        };

        let stdout = String::from_utf8_lossy(&output.stdout);

        // Parse the output
        parse_usage_output(&stdout)
    }

    fn name(&self) -> &'static str {
        "claude-cli"
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

    #[test]
    fn test_strip_ansi_codes() {
        let input = "\x1b[38;2;215;119;87mHello\x1b[0m World";
        let result = strip_ansi_codes(input);
        assert!(result.contains("Hello"));
        assert!(result.contains("World"));
        assert!(!result.contains('\x1b'));
    }

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
    fn test_parse_usage_section() {
        let text = "Current session    \n████████   16% used  \nResets 1pm";
        let usage = parse_usage_section(text, "Current session");
        assert!(usage.is_some());
        let usage = usage.unwrap();
        assert!((usage.utilization - 16.0).abs() < 0.1);
    }

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
}
