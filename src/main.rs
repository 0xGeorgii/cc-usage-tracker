#![warn(clippy::pedantic)]
//! # cc-usage-tracker
//!
//! A Linux system tray application that displays Claude Code usage statistics.
//!
//! This application shows:
//! - Current session (5-hour) usage percentage and reset time
//! - Weekly (7-day) usage percentage and reset time
//! - Optional Sonnet-specific usage statistics
//!
//! The tray label displays usage in the format: `XX% (Xh Xm)`
//!
//! ## Architecture
//!
//! - Uses `libappindicator` with GTK3 for system tray integration
//! - Polls Claude CLI every 60 seconds using `echo '/usage' | claude`
//! - Event-driven UI updates (no polling timer)

mod api;
mod cli_provider;
mod credentials;
mod display;
mod provider;

use api::UsageResponse;
use chrono::Local;
use cli_provider::ClaudeCliProvider;
use gtk::prelude::*;
use libappindicator::{AppIndicator, AppIndicatorStatus};
use provider::UsageProvider;
use std::sync::{Arc, RwLock};
use std::time::Duration;

/// Polling interval for fetching usage data from Claude CLI.
const POLL_INTERVAL_SECS: u64 = 60;

/// Application state shared between the polling task and UI thread.
#[derive(Debug, Clone)]
struct AppState {
    usage: Option<UsageResponse>,
    error: Option<String>,
}

impl AppState {
    fn new() -> Self {
        Self {
            usage: None,
            error: None,
        }
    }

    fn format_label(&self) -> String {
        if let Some(ref usage) = self.usage {
            let reset_time = display::format_time_until_short(usage.five_hour.resets_at);
            format!("{:.0}% ({})", usage.five_hour.utilization, reset_time)
        } else if self.error.is_some() {
            "CC: Error".to_string()
        } else {
            "CC: ...".to_string()
        }
    }
}

fn create_provider() -> Box<dyn UsageProvider> {
    eprintln!("Using Claude CLI provider to fetch real usage data.");
    Box::new(ClaudeCliProvider::new())
}

fn build_menu(state: &AppState) -> gtk::Menu {
    let menu = gtk::Menu::new();

    if let Some(ref usage) = state.usage {
        // Session usage
        let session_item = gtk::MenuItem::with_label(&format!(
            "Session (5h): {:.0}% used",
            usage.five_hour.utilization
        ));
        session_item.set_sensitive(false);
        menu.append(&session_item);

        let session_reset = gtk::MenuItem::with_label(&format!(
            "  Resets in: {}",
            display::format_time_until_short(usage.five_hour.resets_at)
        ));
        session_reset.set_sensitive(false);
        menu.append(&session_reset);

        // Separator
        menu.append(&gtk::SeparatorMenuItem::new());

        // Weekly usage
        let weekly_item = gtk::MenuItem::with_label(&format!(
            "Weekly (7d): {:.0}% used",
            usage.seven_day.utilization
        ));
        weekly_item.set_sensitive(false);
        menu.append(&weekly_item);

        let weekly_reset = gtk::MenuItem::with_label(&format!(
            "  Resets in: {}",
            display::format_time_until_short(usage.seven_day.resets_at)
        ));
        weekly_reset.set_sensitive(false);
        menu.append(&weekly_reset);

        // Sonnet usage if available
        if let Some(ref opus) = usage.seven_day_opus {
            menu.append(&gtk::SeparatorMenuItem::new());

            let opus_item =
                gtk::MenuItem::with_label(&format!("Sonnet (7d): {:.0}% used", opus.utilization));
            opus_item.set_sensitive(false);
            menu.append(&opus_item);
        }
    } else if let Some(ref error) = state.error {
        let error_item = gtk::MenuItem::with_label(&format!(
            "Error: {}",
            if error.len() > 50 {
                &error[..50]
            } else {
                error
            }
        ));
        error_item.set_sensitive(false);
        menu.append(&error_item);
    } else {
        let loading_item = gtk::MenuItem::with_label("Loading...");
        loading_item.set_sensitive(false);
        menu.append(&loading_item);
    }

    // Separator before quit
    menu.append(&gtk::SeparatorMenuItem::new());

    // Quit item
    let quit_item = gtk::MenuItem::with_label("Quit");
    quit_item.connect_activate(|_| {
        gtk::main_quit();
        std::process::exit(0);
    });
    menu.append(&quit_item);

    menu.show_all();
    menu
}

/// UI context stored behind raw pointer for access from GTK main thread.
/// SAFETY: Only accessed from the GTK main thread via glib callbacks.
struct UiContext {
    indicator: AppIndicator,
    current_menu: Option<gtk::Menu>,
}

/// Wrapper to make raw pointer Send+Sync.
/// SAFETY: `UiContext` is only ever accessed from the GTK main thread.
struct UiContextPtr(*mut UiContext);
unsafe impl Send for UiContextPtr {}
unsafe impl Sync for UiContextPtr {}

/// Global UI context pointer. Set once at startup, accessed only from main thread.
static UI_CONTEXT: std::sync::OnceLock<UiContextPtr> = std::sync::OnceLock::new();

#[tokio::main]
async fn main() {
    // Initialize GTK first
    gtk::init().expect("Failed to initialize GTK");

    let provider = create_provider();
    let state = Arc::new(RwLock::new(AppState::new()));

    // Create AppIndicator with a transparent icon (we only use the label)
    // Get path to assets directory - try multiple locations
    let icon_theme_path = std::env::current_exe()
        .ok()
        .and_then(|exe| {
            // Try next to the executable
            let exe_dir = exe.parent()?;
            let assets = exe_dir.join("assets");
            if assets.join("transparent.png").exists() {
                return Some(assets.to_string_lossy().to_string());
            }
            // Try parent directory (for cargo run)
            let parent = exe_dir.parent()?;
            let assets = parent.join("assets");
            if assets.join("transparent.png").exists() {
                return Some(assets.to_string_lossy().to_string());
            }
            None
        })
        // Fallback to compile-time path for development
        .unwrap_or_else(|| format!("{}/assets", env!("CARGO_MANIFEST_DIR")));

    let mut indicator = AppIndicator::new("Claude Code Usage", "transparent");
    indicator.set_icon_theme_path(&icon_theme_path);
    indicator.set_status(AppIndicatorStatus::Active);
    indicator.set_label("CC: ...", "");

    // Build initial menu
    let initial_state = state.read().unwrap();
    let mut menu = build_menu(&initial_state);
    drop(initial_state);
    indicator.set_menu(&mut menu);

    // Store UI context for access from idle callbacks
    let ui_context = Box::new(UiContext {
        indicator,
        current_menu: Some(menu),
    });
    let ui_ptr = Box::into_raw(ui_context);
    UI_CONTEXT.set(UiContextPtr(ui_ptr)).ok();

    println!("cc-usage-tracker started");
    println!("Look for the indicator in your system tray.");

    // Clone for polling
    let poll_state = Arc::clone(&state);

    // Spawn polling task
    tokio::spawn(async move {
        loop {
            let timestamp = Local::now().format("%H:%M:%S");
            eprintln!("[{timestamp}] Fetching usage data...");

            match provider.fetch_usage().await {
                Ok(usage) => {
                    eprintln!(
                        "[{}] Success: 5h={}%, 7d={}%",
                        Local::now().format("%H:%M:%S"),
                        usage.five_hour.utilization,
                        usage.seven_day.utilization
                    );
                    let mut s = poll_state.write().unwrap();
                    s.usage = Some(usage);
                    s.error = None;
                }
                Err(e) => {
                    eprintln!("[{}] Error: {}", Local::now().format("%H:%M:%S"), e);
                    let mut s = poll_state.write().unwrap();
                    s.error = Some(e);
                }
            }

            // Schedule UI update on main thread (event-driven, no polling)
            let state_for_idle = Arc::clone(&poll_state);
            glib::idle_add_once(move || {
                let state_snapshot = state_for_idle.read().unwrap().clone();
                let label = state_snapshot.format_label();
                let mut new_menu = build_menu(&state_snapshot);

                // SAFETY: UI_CONTEXT is only accessed from GTK main thread
                if let Some(wrapper) = UI_CONTEXT.get() {
                    let ptr = wrapper.0;
                    unsafe {
                        (*ptr).indicator.set_label(&label, "");
                        (*ptr).indicator.set_menu(&mut new_menu);
                        // Drop old menu, store new one (fixes memory leak)
                        (*ptr).current_menu = Some(new_menu);
                    }
                }
            });

            tokio::time::sleep(Duration::from_secs(POLL_INTERVAL_SECS)).await;
        }
    });

    // Run GTK main loop
    gtk::main();
}
