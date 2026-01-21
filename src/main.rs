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
mod theme;

use api::UsageResponse;
use chrono::Local;
use cli_provider::{kill_all_children, ClaudeCliProvider};
use gtk::prelude::*;
use libappindicator::{AppIndicator, AppIndicatorStatus};
use provider::UsageProvider;
use std::sync::{Arc, RwLock};
use std::time::Duration;


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
            display::format_tray_label(usage)
        } else if self.error.is_some() {
            display::format_error_label()
        } else {
            display::format_loading_label()
        }
    }
}

fn create_provider() -> Box<dyn UsageProvider> {
    eprintln!("Using Claude CLI provider to fetch real usage data.");
    Box::new(ClaudeCliProvider::new())
}

/// Helper to rebuild the menu immediately (used by option toggles)
fn rebuild_menu_now() {
    if let Some(wrapper) = UI_CONTEXT.get() {
        let ptr = wrapper.0;
        unsafe {
            if let Some(state_lock) = STATE.get() {
                let state_snapshot = state_lock.read().unwrap().clone();
                let label = state_snapshot.format_label();
                (*ptr).indicator.set_label(&label, "");
                let mut new_menu = build_menu(&state_snapshot);
                (*ptr).indicator.set_menu(&mut new_menu);
                (*ptr).current_menu = Some(new_menu);
            }
        }
    }
}

fn build_menu(state: &AppState) -> gtk::Menu {
    let menu = gtk::Menu::new();

    if let Some(ref usage) = state.usage {
        // ━━━ SESSION SECTION ━━━
        let session_header =
            gtk::MenuItem::with_label(&display::format_section_header("SESSION (5h)"));
        session_header.set_sensitive(false);
        menu.append(&session_header);

        // Progress bar with percentage
        let session_bar = display::wide_progress_bar(usage.five_hour.utilization);
        let session_item = gtk::MenuItem::with_label(&format!(
            "{}  {:.0}%",
            session_bar,
            usage.five_hour.utilization
        ));
        session_item.set_sensitive(false);
        menu.append(&session_item);

        let session_reset = gtk::MenuItem::with_label(&format!(
            "{} Resets in {}",
            display::session_icon(),
            display::format_time_until_short(usage.five_hour.resets_at)
        ));
        session_reset.set_sensitive(false);
        menu.append(&session_reset);

        // ━━━ WEEKLY SECTION ━━━
        let weekly_header =
            gtk::MenuItem::with_label(&display::format_section_header("WEEKLY (7d)"));
        weekly_header.set_sensitive(false);
        menu.append(&weekly_header);

        // Progress bar with percentage
        let weekly_bar = display::wide_progress_bar(usage.seven_day.utilization);
        let weekly_item = gtk::MenuItem::with_label(&format!(
            "{}  {:.0}%",
            weekly_bar,
            usage.seven_day.utilization
        ));
        weekly_item.set_sensitive(false);
        menu.append(&weekly_item);

        let weekly_reset = gtk::MenuItem::with_label(&format!(
            "{} Resets in {}",
            display::weekly_icon(),
            display::format_time_until_short(usage.seven_day.resets_at)
        ));
        weekly_reset.set_sensitive(false);
        menu.append(&weekly_reset);

        // ━━━ SONNET SECTION (if available and enabled) ━━━
        if display::show_sonnet() {
            if let Some(ref sonnet) = usage.seven_day_opus {
                let sonnet_header =
                    gtk::MenuItem::with_label(&display::format_section_header("SONNET (7d)"));
                sonnet_header.set_sensitive(false);
                menu.append(&sonnet_header);

                let sonnet_bar = display::wide_progress_bar(sonnet.utilization);
                let sonnet_item = gtk::MenuItem::with_label(&format!(
                    "{}  {:.0}%",
                    sonnet_bar,
                    sonnet.utilization
                ));
                sonnet_item.set_sensitive(false);
                menu.append(&sonnet_item);
            }
        }

        // ━━━ FOOTER ━━━
        if display::show_updated_time() {
            menu.append(&gtk::SeparatorMenuItem::new());

            let updated_item = gtk::MenuItem::with_label(&format!(
                "Updated: {}",
                display::format_current_time()
            ));
            updated_item.set_sensitive(false);
            menu.append(&updated_item);
        }
    } else if let Some(ref error) = state.error {
        let error_header =
            gtk::MenuItem::with_label(&display::format_section_header("ERROR"));
        error_header.set_sensitive(false);
        menu.append(&error_header);

        let error_item = gtk::MenuItem::with_label(&format!(
            "{} {}",
            display::error_icon(),
            if error.len() > 40 {
                &error[..40]
            } else {
                error
            }
        ));
        error_item.set_sensitive(false);
        menu.append(&error_item);
    } else {
        let loading_item = gtk::MenuItem::with_label(&format!(
            "{} Loading usage data...",
            display::loading_indicator()
        ));
        loading_item.set_sensitive(false);
        menu.append(&loading_item);
    }

    // Separator before options/theme/quit
    menu.append(&gtk::SeparatorMenuItem::new());

    // Options submenu
    let options_item = gtk::MenuItem::with_label("Options");
    let options_submenu = gtk::Menu::new();

    // Toggle: Show Sonnet section
    let sonnet_label = if display::show_sonnet() {
        "● Show Sonnet"
    } else {
        "  Show Sonnet"
    };
    let sonnet_toggle = gtk::MenuItem::with_label(sonnet_label);
    sonnet_toggle.connect_activate(|_| {
        display::toggle_sonnet();
        rebuild_menu_now();
    });
    options_submenu.append(&sonnet_toggle);

    // Toggle: Show updated time
    let updated_label = if display::show_updated_time() {
        "● Show Updated Time"
    } else {
        "  Show Updated Time"
    };
    let updated_toggle = gtk::MenuItem::with_label(updated_label);
    updated_toggle.connect_activate(|_| {
        display::toggle_updated_time();
        rebuild_menu_now();
    });
    options_submenu.append(&updated_toggle);

    // Toggle: Show theme selector
    let theme_sel_label = if display::show_theme_selector() {
        "● Show Theme Selector"
    } else {
        "  Show Theme Selector"
    };
    let theme_sel_toggle = gtk::MenuItem::with_label(theme_sel_label);
    theme_sel_toggle.connect_activate(|_| {
        display::toggle_theme_selector();
        rebuild_menu_now();
    });
    options_submenu.append(&theme_sel_toggle);

    options_item.set_submenu(Some(&options_submenu));
    menu.append(&options_item);

    // Update Interval submenu
    let interval_item = gtk::MenuItem::with_label("Update Interval");
    let interval_submenu = gtk::Menu::new();

    let current_interval = display::update_interval_secs();
    for (secs, label) in display::update_interval_options() {
        let item_label = if *secs == current_interval {
            format!("● {}", label)
        } else {
            format!("  {}", label)
        };
        let interval_option = gtk::MenuItem::with_label(&item_label);
        let secs_to_set = *secs;
        interval_option.connect_activate(move |_| {
            display::set_update_interval_secs(secs_to_set);
            rebuild_menu_now();
        });
        interval_submenu.append(&interval_option);
    }

    interval_item.set_submenu(Some(&interval_submenu));
    menu.append(&interval_item);

    // Theme submenu (if enabled)
    if display::show_theme_selector() {
        let theme_item = gtk::MenuItem::with_label("Theme");
        let theme_submenu = gtk::Menu::new();

        let current = display::current_theme_name();
        for theme_name in theme::ThemeName::all() {
            let label = if *theme_name == current {
                format!("● {}", theme_name.as_str())
            } else {
                format!("  {}", theme_name.as_str())
            };
            let theme_option = gtk::MenuItem::with_label(&label);
            let theme_to_set = *theme_name;
            theme_option.connect_activate(move |_| {
                display::set_theme(theme_to_set);
                rebuild_menu_now();
            });
            theme_submenu.append(&theme_option);
        }

        theme_item.set_submenu(Some(&theme_submenu));
        menu.append(&theme_item);
    }

    // Quit item
    let quit_item = gtk::MenuItem::with_label(&format!("{} Quit", display::quit_icon()));
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

/// Global app state for access from menu callbacks.
static STATE: std::sync::OnceLock<Arc<RwLock<AppState>>> = std::sync::OnceLock::new();

#[tokio::main]
async fn main() {
    // Initialize GTK first
    gtk::init().expect("Failed to initialize GTK");

    let provider = create_provider();
    let state = Arc::new(RwLock::new(AppState::new()));

    // Store state globally for menu callbacks
    STATE.set(Arc::clone(&state)).ok();

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
    indicator.set_label(&display::format_loading_label(), "");
    indicator.set_status(AppIndicatorStatus::Active); // Set active AFTER label to avoid blank flash

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

    // Spawn signal handler for graceful shutdown
    tokio::spawn(async {
        use tokio::signal::unix::{signal, SignalKind};

        let mut sigterm = signal(SignalKind::terminate()).expect("Failed to register SIGTERM");
        let mut sigint = signal(SignalKind::interrupt()).expect("Failed to register SIGINT");

        tokio::select! {
            _ = sigterm.recv() => {
                eprintln!("Received SIGTERM, shutting down...");
            }
            _ = sigint.recv() => {
                eprintln!("Received SIGINT, shutting down...");
            }
        }

        // Kill all tracked child processes to prevent orphans
        kill_all_children();

        // Quit GTK main loop
        glib::idle_add_once(|| {
            gtk::main_quit();
        });
    });

    // Clone for polling
    let poll_state = Arc::clone(&state);

    // Spawn polling task
    tokio::spawn(async move {
        loop {
            let timestamp = Local::now().format("%H:%M:%S");
            eprintln!("[{timestamp}] Fetching usage data...");

            let should_update_ui = match provider.fetch_usage().await {
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
                    true // Update UI on success
                }
                Err(e) => {
                    eprintln!("[{}] Error: {}", Local::now().format("%H:%M:%S"), e);
                    let mut s = poll_state.write().unwrap();
                    s.error = Some(e.clone());
                    // Only update UI if we already have data (to show error in menu)
                    // Otherwise keep showing loading indicator
                    s.usage.is_some()
                }
            };

            // Only update UI if we have data or successfully fetched
            if should_update_ui {
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
            }

            tokio::time::sleep(Duration::from_secs(display::update_interval_secs())).await;
        }
    });

    // Run GTK main loop
    gtk::main();
}
