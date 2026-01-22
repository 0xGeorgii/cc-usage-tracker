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
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};
use std::time::Duration;

/// Track whether the session is locked to skip UI updates
static SESSION_LOCKED: AtomicBool = AtomicBool::new(false);

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
    // Skip updates while session is locked to avoid GTK widget errors
    if SESSION_LOCKED.load(Ordering::Relaxed) {
        return;
    }

    if let Some(wrapper) = UI_CONTEXT.get() {
        let ptr = wrapper.0;
        // SAFETY: UI_CONTEXT is only accessed from GTK main thread
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

/// Build a compact usage row: header with stats + progress bar
fn build_usage_row(menu: &gtk::Menu, label: &str, utilization: f64, reset_time: &str) {
    // Header line with inline stats: [ SESSION ]  20%  ·  Resets in 3h 42m
    let header = gtk::MenuItem::with_label(&format!(
        "{} {:.0}% · {}",
        display::format_section_header(label),
        utilization,
        reset_time
    ));
    header.set_sensitive(false);
    menu.append(&header);

    // Progress bar
    let bar = display::wide_progress_bar(utilization);
    let bar_item = gtk::MenuItem::with_label(&bar);
    bar_item.set_sensitive(false);
    menu.append(&bar_item);
}

/// Build the usage data sections (session, weekly, sonnet)
fn build_usage_sections(menu: &gtk::Menu, usage: &api::UsageResponse) {
    // Session
    build_usage_row(
        menu,
        "SESSION",
        usage.five_hour.utilization,
        &format!(
            "Resets in {}",
            display::format_time_until_short(usage.five_hour.resets_at)
        ),
    );

    // Weekly
    build_usage_row(
        menu,
        "WEEKLY",
        usage.seven_day.utilization,
        &format!(
            "Resets in {}",
            display::format_time_until_short(usage.seven_day.resets_at)
        ),
    );

    // Sonnet (if available and enabled)
    if display::show_sonnet() {
        if let Some(ref sonnet) = usage.seven_day_opus {
            build_usage_row(menu, "SONNET", sonnet.utilization, "7-day window");
        }
    }

    // Updated time (smaller, at end of status section)
    if display::show_updated_time() {
        let updated_item =
            gtk::MenuItem::with_label(&format!("Updated {}", display::format_current_time()));
        updated_item.set_sensitive(false);
        menu.append(&updated_item);
    }
}

/// Build the Display settings submenu
fn build_display_submenu() -> gtk::Menu {
    let submenu = gtk::Menu::new();

    let sonnet_label = if display::show_sonnet() {
        "● Show Sonnet"
    } else {
        "○ Show Sonnet"
    };
    let sonnet_toggle = gtk::MenuItem::with_label(sonnet_label);
    sonnet_toggle.connect_activate(|_| {
        display::toggle_sonnet();
        rebuild_menu_now();
    });
    submenu.append(&sonnet_toggle);

    let updated_label = if display::show_updated_time() {
        "● Show Updated Time"
    } else {
        "○ Show Updated Time"
    };
    let updated_toggle = gtk::MenuItem::with_label(updated_label);
    updated_toggle.connect_activate(|_| {
        display::toggle_updated_time();
        rebuild_menu_now();
    });
    submenu.append(&updated_toggle);

    submenu
}

/// Build the Update Interval settings submenu
fn build_interval_submenu() -> gtk::Menu {
    let submenu = gtk::Menu::new();
    let current_interval = display::update_interval_secs();

    for (secs, label) in display::update_interval_options() {
        let item_label = if *secs == current_interval {
            format!("● {label}")
        } else {
            format!("○ {label}")
        };
        let interval_option = gtk::MenuItem::with_label(&item_label);
        let secs_to_set = *secs;
        interval_option.connect_activate(move |_| {
            display::set_update_interval_secs(secs_to_set);
            rebuild_menu_now();
        });
        submenu.append(&interval_option);
    }

    submenu
}

/// Build the Theme settings submenu
fn build_theme_submenu() -> gtk::Menu {
    let submenu = gtk::Menu::new();
    let current = display::current_theme_name();

    for theme_name in theme::ThemeName::all() {
        let label = if *theme_name == current {
            format!("● {}", theme_name.as_str())
        } else {
            format!("○ {}", theme_name.as_str())
        };
        let theme_option = gtk::MenuItem::with_label(&label);
        let theme_to_set = *theme_name;
        theme_option.connect_activate(move |_| {
            display::set_theme(theme_to_set);
            rebuild_menu_now();
        });
        submenu.append(&theme_option);
    }

    submenu
}

/// Build the consolidated Settings submenu
fn build_settings_submenu() -> gtk::Menu {
    let submenu = gtk::Menu::new();

    // Display submenu with inline status
    let display_count = [display::show_sonnet(), display::show_updated_time()]
        .iter()
        .filter(|&&x| x)
        .count();
    let display_item = gtk::MenuItem::with_label(&format!("Display: {display_count} on"));
    display_item.set_submenu(Some(&build_display_submenu()));
    submenu.append(&display_item);

    // Update Interval submenu with inline current value
    let current_interval = display::update_interval_secs();
    let interval_label = display::update_interval_options()
        .iter()
        .find(|(secs, _)| *secs == current_interval)
        .map_or("1 min", |(_, label)| label);
    let interval_item = gtk::MenuItem::with_label(&format!("Update: {interval_label}"));
    interval_item.set_submenu(Some(&build_interval_submenu()));
    submenu.append(&interval_item);

    // Theme submenu with inline current theme
    let current_theme = display::current_theme_name();
    let theme_item = gtk::MenuItem::with_label(&format!("Theme: {}", current_theme.as_str()));
    theme_item.set_submenu(Some(&build_theme_submenu()));
    submenu.append(&theme_item);

    submenu
}

/// Schedule a timer to trigger at the specified time for a specific period
fn schedule_timer_at(period: display::TimePeriod, target_time: chrono::DateTime<chrono::Local>) {
    use chrono::Local;

    // Store the scheduled time for this period
    display::set_scheduled_timer(period, target_time);
    rebuild_menu_now();

    // Calculate delay until target time
    let now = Local::now();
    let delay = target_time.signed_duration_since(now);

    if delay.num_seconds() <= 0 {
        // Time is in the past or now - trigger immediately
        trigger_timer_now(period);
        return;
    }

    // Schedule using glib timeout
    let delay_ms = u64::try_from(delay.num_milliseconds().max(0)).unwrap_or(u64::MAX);
    glib::timeout_add_once(std::time::Duration::from_millis(delay_ms), move || {
        trigger_timer_now(period);
    });

    eprintln!(
        "[timer] {} scheduled for {} ({} seconds from now)",
        period.name(),
        target_time.format("%-I:%M %p"),
        delay.num_seconds()
    );
}

/// Trigger the timer start message now for a specific period
fn trigger_timer_now(period: display::TimePeriod) {
    // Spawn a thread to run the async code
    std::thread::spawn(move || {
        // Create a small tokio runtime for this operation
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("Failed to create tokio runtime");

        rt.block_on(async {
            match cli_provider::send_timer_start_message().await {
                Ok(()) => eprintln!("[timer] {} timer message sent successfully", period.name()),
                Err(e) => eprintln!(
                    "[timer] Failed to send {} timer message: {e}",
                    period.name()
                ),
            }
        });

        // Clear the scheduled timer for this period and refresh menu on GTK main thread
        glib::idle_add_once(move || {
            display::clear_scheduled_timer(period);
            rebuild_menu_now();
        });
    });
}

/// Build a time period submenu (e.g., Morning with hourly slots)
fn build_time_period_submenu(period: display::TimePeriod) -> gtk::Menu {
    use chrono::{Local, TimeZone};

    let submenu = gtk::Menu::new();
    let now = Local::now();
    let (start_hour, end_hour) = period.hour_range();

    for hour in start_hour..end_hour {
        // Create target time for today at this hour
        let target: Option<chrono::DateTime<Local>> = now
            .date_naive()
            .and_hms_opt(hour, 0, 0)
            .and_then(|dt| Local.from_local_datetime(&dt).single());

        let Some(mut target_time) = target else {
            continue;
        };

        // If the time has passed today, schedule for tomorrow
        if target_time <= now {
            target_time += chrono::Duration::days(1);
        }

        // Format label (e.g., "8:00 AM" or "8:00 AM (tomorrow)")
        let is_tomorrow = target_time.date_naive() != now.date_naive();
        let label = if is_tomorrow {
            format!("{} (tomorrow)", target_time.format("%-I:%M %p"))
        } else {
            target_time.format("%-I:%M %p").to_string()
        };

        let time_item = gtk::MenuItem::with_label(&label);
        time_item.connect_activate(move |_| {
            schedule_timer_at(period, target_time);
        });
        submenu.append(&time_item);
    }

    // Add cancel option if this period has a scheduled timer
    if display::scheduled_timer(period).is_some() {
        submenu.append(&gtk::SeparatorMenuItem::new());
        let cancel_item = gtk::MenuItem::with_label("Cancel");
        cancel_item.connect_activate(move |_| {
            display::clear_scheduled_timer(period);
            rebuild_menu_now();
            eprintln!("[timer] {} timer cancelled", period.name());
        });
        submenu.append(&cancel_item);
    }

    submenu
}

/// Build the Schedule Timer submenu with Morning/Afternoon/Evening submenus
fn build_timer_submenu() -> gtk::Menu {
    let submenu = gtk::Menu::new();

    for &period in display::TimePeriod::all() {
        // Show dot if timer is scheduled for this period
        let scheduled_time = display::scheduled_timer(period);
        let label = if let Some(time) = scheduled_time {
            format!("● {} ({})", period.name(), time.format("%-I:%M %p"))
        } else {
            format!("○ {}", period.name())
        };

        let period_item = gtk::MenuItem::with_label(&label);
        period_item.set_submenu(Some(&build_time_period_submenu(period)));
        submenu.append(&period_item);
    }

    // Cancel All Timers (if any are scheduled)
    if display::has_any_scheduled_timer() {
        submenu.append(&gtk::SeparatorMenuItem::new());
        let cancel_all = gtk::MenuItem::with_label("Cancel All Timers");
        cancel_all.connect_activate(|_| {
            for &period in display::TimePeriod::all() {
                display::clear_scheduled_timer(period);
            }
            rebuild_menu_now();
            eprintln!("[timer] All timers cancelled");
        });
        submenu.append(&cancel_all);
    }

    submenu
}

fn build_menu(state: &AppState) -> gtk::Menu {
    let menu = gtk::Menu::new();

    // ═══════════════════════════════════════════════════════════════════════════
    // ZONE 1: Status (usage data, error, or loading)
    // ═══════════════════════════════════════════════════════════════════════════
    if let Some(ref usage) = state.usage {
        build_usage_sections(&menu, usage);
    } else if let Some(ref error) = state.error {
        let error_header = gtk::MenuItem::with_label(&display::format_section_header("ERROR"));
        error_header.set_sensitive(false);
        menu.append(&error_header);

        let error_msg = if error.len() > 40 {
            &error[..40]
        } else {
            error
        };
        let error_item =
            gtk::MenuItem::with_label(&format!("{} {error_msg}", display::error_icon()));
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

    // ═══════════════════════════════════════════════════════════════════════════
    // ZONE 2: Timer scheduling
    // ═══════════════════════════════════════════════════════════════════════════
    menu.append(&gtk::SeparatorMenuItem::new());

    // Show consolidated scheduled timers status
    let scheduled = display::all_scheduled_timers();
    if !scheduled.is_empty() {
        let times: Vec<String> = scheduled
            .iter()
            .map(|(_, time)| time.format("%-I:%M %p").to_string())
            .collect();
        let timer_status =
            gtk::MenuItem::with_label(&format!("⏰ Scheduled: {}", times.join(", ")));
        timer_status.set_sensitive(false);
        menu.append(&timer_status);
    }

    // Schedule Timer submenu
    let timer_label = if display::has_any_scheduled_timer() {
        "Schedule Timer ⏰"
    } else {
        "Schedule Timer"
    };
    let timer_item = gtk::MenuItem::with_label(timer_label);
    timer_item.set_submenu(Some(&build_timer_submenu()));
    menu.append(&timer_item);

    // ═══════════════════════════════════════════════════════════════════════════
    // ZONE 3: Settings
    // ═══════════════════════════════════════════════════════════════════════════
    menu.append(&gtk::SeparatorMenuItem::new());

    let settings_item = gtk::MenuItem::with_label("Settings");
    settings_item.set_submenu(Some(&build_settings_submenu()));
    menu.append(&settings_item);

    // ═══════════════════════════════════════════════════════════════════════════
    // ZONE 4: Quit
    // ═══════════════════════════════════════════════════════════════════════════
    menu.append(&gtk::SeparatorMenuItem::new());

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

/// Find the assets directory path, checking multiple locations.
fn find_assets_path() -> String {
    std::env::current_exe()
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
        .unwrap_or_else(|| format!("{}/assets", env!("CARGO_MANIFEST_DIR")))
}

/// Spawn the signal handler for graceful shutdown.
fn spawn_signal_handler() {
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

        kill_all_children();
        glib::idle_add_once(gtk::main_quit);
    });
}

/// Spawn the polling task that fetches usage data periodically.
fn spawn_polling_task(provider: Box<dyn UsageProvider>, state: Arc<RwLock<AppState>>) {
    tokio::spawn(async move {
        loop {
            // Skip fetching entirely when session is locked - no one is there to see updates
            if SESSION_LOCKED.load(Ordering::Relaxed) {
                tokio::time::sleep(Duration::from_secs(display::update_interval_secs())).await;
                continue;
            }

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
                    let mut s = state.write().unwrap();
                    s.usage = Some(usage);
                    s.error = None;
                    true
                }
                Err(e) => {
                    eprintln!("[{}] Error: {}", Local::now().format("%H:%M:%S"), e);
                    let mut s = state.write().unwrap();
                    s.error = Some(e.clone());
                    s.usage.is_some()
                }
            };

            if should_update_ui {
                let state_for_idle = Arc::clone(&state);
                glib::idle_add_once(move || {
                    let state_snapshot = state_for_idle.read().unwrap().clone();
                    let label = state_snapshot.format_label();
                    let mut new_menu = build_menu(&state_snapshot);

                    if let Some(wrapper) = UI_CONTEXT.get() {
                        let ptr = wrapper.0;
                        // SAFETY: UI_CONTEXT is only accessed from GTK main thread
                        unsafe {
                            (*ptr).indicator.set_label(&label, "");
                            (*ptr).indicator.set_menu(&mut new_menu);
                            (*ptr).current_menu = Some(new_menu);
                        }
                    }
                });
            }

            tokio::time::sleep(Duration::from_secs(display::update_interval_secs())).await;
        }
    });
}

/// Watch for system sleep/wake and session lock/unlock events via D-Bus.
///
/// Listens to:
/// - `org.freedesktop.login1.Manager.PrepareForSleep` - system sleep/wake
/// - `org.freedesktop.login1.Session.Lock/Unlock` - screen lock/unlock
async fn watch_system_events() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    use futures_util::StreamExt;
    use zbus::Connection;

    let connection = Connection::system().await?;

    // Match both Manager signals (sleep) and Session signals (lock)
    let rule = zbus::match_rule::MatchRule::builder()
        .msg_type(zbus::message::Type::Signal)
        .interface("org.freedesktop.login1.Manager")?
        .build();

    let session_rule = zbus::match_rule::MatchRule::builder()
        .msg_type(zbus::message::Type::Signal)
        .interface("org.freedesktop.login1.Session")?
        .build();

    let mut stream = zbus::MessageStream::for_match_rule(rule, &connection, None).await?;
    let mut session_stream =
        zbus::MessageStream::for_match_rule(session_rule, &connection, None).await?;

    eprintln!("[events] Watching for sleep/wake and lock/unlock events");

    loop {
        tokio::select! {
            Some(msg) = stream.next() => {
                if let Ok(msg) = msg {
                    let member = msg.header().member().map(ToString::to_string);
                    if member.as_deref() == Some("PrepareForSleep") {
                        if let Ok((going_to_sleep,)) = msg.body().deserialize::<(bool,)>() {
                            if going_to_sleep {
                                eprintln!("[events] System going to sleep");
                                SESSION_LOCKED.store(true, Ordering::Relaxed);
                            } else {
                                eprintln!("[events] System waking up, refreshing in 1s");
                                SESSION_LOCKED.store(false, Ordering::Relaxed);
                                glib::timeout_add_once(Duration::from_secs(1), rebuild_menu_now);
                            }
                        }
                    }
                }
            }
            Some(msg) = session_stream.next() => {
                if let Ok(msg) = msg {
                    let member = msg.header().member().map(ToString::to_string);
                    match member.as_deref() {
                        Some("Lock") => {
                            eprintln!("[events] Session locked - pausing UI updates");
                            SESSION_LOCKED.store(true, Ordering::Relaxed);
                        }
                        Some("Unlock") => {
                            eprintln!("[events] Session unlocked, refreshing in 1s");
                            SESSION_LOCKED.store(false, Ordering::Relaxed);
                            glib::timeout_add_once(Duration::from_secs(1), rebuild_menu_now);
                        }
                        _ => {}
                    }
                }
            }
        }
    }
}

#[tokio::main]
async fn main() {
    gtk::init().expect("Failed to initialize GTK");

    let provider = create_provider();
    let state = Arc::new(RwLock::new(AppState::new()));
    STATE.set(Arc::clone(&state)).ok();

    // Create and configure the AppIndicator
    let mut indicator = AppIndicator::new("Claude Code Usage", "transparent");
    indicator.set_icon_theme_path(&find_assets_path());
    indicator.set_label("...", "");
    indicator.set_status(AppIndicatorStatus::Active);

    // Build initial menu
    let initial_state = state.read().unwrap();
    let mut menu = build_menu(&initial_state);
    drop(initial_state);
    indicator.set_menu(&mut menu);

    // Store UI context globally for access from callbacks
    let ui_context = Box::new(UiContext {
        indicator,
        current_menu: Some(menu),
    });
    UI_CONTEXT.set(UiContextPtr(Box::into_raw(ui_context))).ok();

    println!("cc-usage-tracker started");
    println!("Look for the indicator in your system tray.");

    // Spawn background tasks
    spawn_signal_handler();
    spawn_polling_task(provider, Arc::clone(&state));
    tokio::spawn(async {
        if let Err(e) = watch_system_events().await {
            eprintln!("[events] Failed to watch for system events: {e}");
        }
    });

    gtk::main();
}
