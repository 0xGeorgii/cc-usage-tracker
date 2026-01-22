//! Display formatting utilities for usage data.
//!
//! This module provides functions to format usage statistics for display
//! in tooltips, menus, and labels. Supports multiple themes for customization.

use crate::api::UsageResponse;
use crate::theme::{Theme, ThemeName};
use chrono::{DateTime, Local, Utc};
use std::sync::RwLock;

/// Current active theme - stored in a `RwLock` for runtime switching
static CURRENT_THEME: RwLock<ThemeName> = RwLock::new(ThemeName::Minimal);

/// Display options
static SHOW_SONNET: RwLock<bool> = RwLock::new(true);
static SHOW_UPDATED_TIME: RwLock<bool> = RwLock::new(true);

/// Update interval in seconds (default 5 minutes)
static UPDATE_INTERVAL_SECS: RwLock<u64> = RwLock::new(300);

/// Time periods for scheduling timers
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TimePeriod {
    Morning,   // 6 AM - 12 PM
    Afternoon, // 12 PM - 6 PM
    Evening,   // 6 PM - 12 AM
}

impl TimePeriod {
    /// Get the hour range for this period (start inclusive, end exclusive)
    pub fn hour_range(self) -> (u32, u32) {
        match self {
            Self::Morning => (6, 12),
            Self::Afternoon => (12, 18),
            Self::Evening => (18, 24),
        }
    }

    /// Get the display name for this period
    pub fn name(self) -> &'static str {
        match self {
            Self::Morning => "Morning",
            Self::Afternoon => "Afternoon",
            Self::Evening => "Evening",
        }
    }

    /// All time periods
    pub fn all() -> &'static [TimePeriod] {
        &[Self::Morning, Self::Afternoon, Self::Evening]
    }
}

/// Scheduled timers per time period (one per section)
static SCHEDULED_TIMERS: RwLock<[Option<DateTime<Local>>; 3]> = RwLock::new([None, None, None]);

/// Get the current theme configuration
pub fn current_theme() -> Theme {
    CURRENT_THEME.read().unwrap().config()
}

/// Get the current theme name
pub fn current_theme_name() -> ThemeName {
    *CURRENT_THEME.read().unwrap()
}

/// Set the current theme
pub fn set_theme(theme: ThemeName) {
    *CURRENT_THEME.write().unwrap() = theme;
}

/// Check if Sonnet section should be shown
pub fn show_sonnet() -> bool {
    *SHOW_SONNET.read().unwrap()
}

/// Toggle Sonnet section visibility
pub fn toggle_sonnet() {
    let mut show = SHOW_SONNET.write().unwrap();
    *show = !*show;
}

/// Check if "Updated" time should be shown
pub fn show_updated_time() -> bool {
    *SHOW_UPDATED_TIME.read().unwrap()
}

/// Toggle "Updated" time visibility
pub fn toggle_updated_time() {
    let mut show = SHOW_UPDATED_TIME.write().unwrap();
    *show = !*show;
}

/// Get the current update interval in seconds
pub fn update_interval_secs() -> u64 {
    *UPDATE_INTERVAL_SECS.read().unwrap()
}

/// Set the update interval in seconds
pub fn set_update_interval_secs(secs: u64) {
    *UPDATE_INTERVAL_SECS.write().unwrap() = secs;
}

/// Get the scheduled timer for a specific period
pub fn scheduled_timer(period: TimePeriod) -> Option<DateTime<Local>> {
    let timers = SCHEDULED_TIMERS.read().unwrap();
    timers[period as usize]
}

/// Set the scheduled timer for a specific period
pub fn set_scheduled_timer(period: TimePeriod, time: DateTime<Local>) {
    let mut timers = SCHEDULED_TIMERS.write().unwrap();
    timers[period as usize] = Some(time);
}

/// Clear the scheduled timer for a specific period
pub fn clear_scheduled_timer(period: TimePeriod) {
    let mut timers = SCHEDULED_TIMERS.write().unwrap();
    timers[period as usize] = None;
}

/// Check if any timer is scheduled
pub fn has_any_scheduled_timer() -> bool {
    let timers = SCHEDULED_TIMERS.read().unwrap();
    timers.iter().any(Option::is_some)
}

/// Get all scheduled timers with their periods
pub fn all_scheduled_timers() -> Vec<(TimePeriod, DateTime<Local>)> {
    let timers = SCHEDULED_TIMERS.read().unwrap();
    TimePeriod::all()
        .iter()
        .filter_map(|&period| timers[period as usize].map(|time| (period, time)))
        .collect()
}

/// Format the scheduled timer for a period (e.g., "8:00 AM")
#[allow(dead_code)] // May be useful for future features
pub fn format_scheduled_timer(period: TimePeriod) -> Option<String> {
    scheduled_timer(period).map(|t| t.format("%-I:%M %p").to_string())
}

/// Available update intervals (in seconds) with display labels
pub fn update_interval_options() -> &'static [(u64, &'static str)] {
    &[
        (60, "1 min"),
        (300, "5 min"),
        (900, "15 min"),
        (1800, "30 min"),
    ]
}

/// Create a wider 10-segment progress bar for menu display (using current theme)
pub fn wide_progress_bar(percentage: f64) -> String {
    current_theme().wide_bar(percentage)
}

/// Format the tray label with visual progress indicator
/// Shows session usage with a compact, modern time display
pub fn format_tray_label(usage: &UsageResponse) -> String {
    let theme = current_theme();
    let session_bar = theme.mini_bar(usage.five_hour.utilization);
    let reset_time = format_time_compact(usage.five_hour.resets_at);
    format!(
        "{} {:.0}% {}{}",
        session_bar, usage.five_hour.utilization, theme.time_icon, reset_time
    )
}

/// Format loading state for tray label
pub fn format_loading_label() -> String {
    current_theme().loading_label()
}

/// Format error state for tray label
pub fn format_error_label() -> String {
    current_theme().error_label()
}

/// Format a section header for the menu
pub fn format_section_header(title: &str) -> String {
    current_theme().section_header(title)
}

/// Get the session reset icon
#[allow(dead_code)] // May be useful for future features
pub fn session_icon() -> &'static str {
    current_theme().session_icon
}

/// Get the weekly reset icon
#[allow(dead_code)] // May be useful for future features
pub fn weekly_icon() -> &'static str {
    current_theme().weekly_icon
}

/// Get the quit menu icon
pub fn quit_icon() -> &'static str {
    current_theme().quit_icon
}

/// Get the loading indicator
pub fn loading_indicator() -> &'static str {
    current_theme().loading
}

/// Get the error icon
pub fn error_icon() -> &'static str {
    current_theme().error_icon
}

/// Format reset time in a compact, modern style (e.g., "2h15m" or "45m")
fn format_time_compact(reset_time: Option<DateTime<Utc>>) -> String {
    let Some(reset) = reset_time else {
        return "—".to_string();
    };

    let now = Utc::now();
    if reset <= now {
        return "0m".to_string();
    }

    let duration = reset - now;
    let total_seconds = duration.num_seconds();

    if total_seconds < 0 {
        return "0m".to_string();
    }

    let hours = total_seconds / 3600;
    let minutes = (total_seconds % 3600) / 60;

    if hours > 0 {
        format!("{hours}h{minutes:02}m")
    } else {
        format!("{minutes}m")
    }
}

/// Format usage data as a single-line tooltip string.
#[allow(dead_code)] // Reserved for future tooltip implementations
pub fn format_tooltip(usage: &UsageResponse) -> String {
    let five_hour_reset = format_time_until_short(usage.five_hour.resets_at);
    let seven_day_reset = format_time_until_short(usage.seven_day.resets_at);

    format!(
        "CC: 5h {:.0}% ({}) | 7d {:.0}% ({})",
        usage.five_hour.utilization, five_hour_reset, usage.seven_day.utilization, seven_day_reset
    )
}

/// Format usage data as multi-line menu text.
#[allow(dead_code)] // Reserved for alternative menu implementations
pub fn format_menu_text(usage: &UsageResponse) -> String {
    let mut lines = Vec::new();

    lines.push(format!(
        "Session (5h): {:.0}% used",
        usage.five_hour.utilization
    ));
    lines.push(format!(
        "  Resets in: {}",
        format_time_until_short(usage.five_hour.resets_at)
    ));
    lines.push(String::new());

    lines.push(format!(
        "Weekly (7d): {:.0}% used",
        usage.seven_day.utilization
    ));
    lines.push(format!(
        "  Resets in: {}",
        format_time_until_short(usage.seven_day.resets_at)
    ));

    if let Some(opus) = &usage.seven_day_opus {
        lines.push(String::new());
        lines.push(format!("Opus (7d): {:.0}% used", opus.utilization));
        if opus.resets_at.is_some() {
            lines.push(format!(
                "  Resets in: {}",
                format_time_until_short(opus.resets_at)
            ));
        }
    }

    lines.join("\n")
}

/// Format a reset time as a short human-readable duration.
///
/// Returns formats like "2h 15m", "3d 4h", "45m", or "N/A" if no time provided.
pub fn format_time_until_short(reset_time: Option<DateTime<Utc>>) -> String {
    let Some(reset) = reset_time else {
        return "N/A".to_string();
    };

    let now = Utc::now();
    if reset <= now {
        return "now".to_string();
    }

    let duration = reset - now;
    let total_seconds = duration.num_seconds();

    if total_seconds < 0 {
        return "now".to_string();
    }

    let days = total_seconds / 86400;
    let hours = (total_seconds % 86400) / 3600;
    let minutes = (total_seconds % 3600) / 60;

    if days > 0 {
        format!("{days}d {hours}h")
    } else if hours > 0 {
        format!("{hours}h {minutes}m")
    } else {
        format!("{minutes}m")
    }
}

/// Format an error message as a tooltip string.
#[allow(dead_code)] // Reserved for error display features
pub fn format_error_tooltip(error: &str) -> String {
    format!("CC: Error - {}", truncate_str(error, 30))
}

#[allow(dead_code)] // Used by format_error_tooltip
fn truncate_str(s: &str, max_len: usize) -> &str {
    if s.len() <= max_len {
        s
    } else {
        &s[..max_len]
    }
}

/// Get current time formatted for "last updated" display
pub fn format_current_time() -> String {
    Local::now().format("%H:%M").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::{UsageData, UsageResponse};
    use chrono::Duration;

    fn make_test_usage(five_hour_pct: f64, seven_day_pct: f64) -> UsageResponse {
        let now = Utc::now();
        UsageResponse {
            five_hour: UsageData {
                utilization: five_hour_pct,
                resets_at: Some(now + Duration::hours(2) + Duration::minutes(15)),
            },
            seven_day: UsageData {
                utilization: seven_day_pct,
                resets_at: Some(now + Duration::days(3) + Duration::hours(4)),
            },
            seven_day_opus: None,
        }
    }

    #[test]
    fn test_format_tooltip_basic() {
        let usage = make_test_usage(6.0, 35.0);
        let tooltip = format_tooltip(&usage);

        assert!(tooltip.starts_with("CC: 5h 6%"));
        assert!(tooltip.contains("7d 35%"));
    }

    #[test]
    fn test_format_tooltip_with_reset_times() {
        let usage = make_test_usage(10.0, 20.0);
        let tooltip = format_tooltip(&usage);

        // Should contain time formatting like "2h 15m" and "3d 4h"
        assert!(tooltip.contains("2h"));
        assert!(tooltip.contains("3d"));
    }

    #[test]
    fn test_format_menu_text_contains_all_sections() {
        let usage = make_test_usage(15.0, 42.0);
        let menu = format_menu_text(&usage);

        assert!(menu.contains("Session (5h): 15% used"));
        assert!(menu.contains("Weekly (7d): 42% used"));
        assert!(menu.contains("Resets in:"));
    }

    #[test]
    fn test_format_menu_text_with_opus() {
        let now = Utc::now();
        let usage = UsageResponse {
            five_hour: UsageData {
                utilization: 10.0,
                resets_at: Some(now + Duration::hours(1)),
            },
            seven_day: UsageData {
                utilization: 25.0,
                resets_at: Some(now + Duration::days(2)),
            },
            seven_day_opus: Some(UsageData {
                utilization: 5.0,
                resets_at: Some(now + Duration::days(4)),
            }),
        };

        let menu = format_menu_text(&usage);
        assert!(menu.contains("Opus (7d): 5% used"));
    }

    #[test]
    fn test_format_time_until_none() {
        assert_eq!(format_time_until_short(None), "N/A");
    }

    #[test]
    fn test_format_time_until_past() {
        let past = Utc::now() - Duration::hours(1);
        assert_eq!(format_time_until_short(Some(past)), "now");
    }

    #[test]
    fn test_format_time_until_minutes() {
        let future = Utc::now() + Duration::minutes(45);
        let result = format_time_until_short(Some(future));
        assert!(result.contains('m'));
        assert!(!result.contains('h'));
    }

    #[test]
    fn test_format_time_until_hours() {
        let future = Utc::now() + Duration::hours(3) + Duration::minutes(30);
        let result = format_time_until_short(Some(future));
        assert!(result.contains('h'));
        assert!(result.contains('m'));
    }

    #[test]
    fn test_format_time_until_days() {
        let future = Utc::now() + Duration::days(2) + Duration::hours(5);
        let result = format_time_until_short(Some(future));
        assert!(result.contains('d'));
        assert!(result.contains('h'));
    }

    #[test]
    fn test_format_error_tooltip() {
        let error = "Connection failed";
        let tooltip = format_error_tooltip(error);
        assert_eq!(tooltip, "CC: Error - Connection failed");
    }

    #[test]
    fn test_format_error_tooltip_truncates_long_errors() {
        let long_error = "This is a very long error message that should be truncated to fit";
        let tooltip = format_error_tooltip(long_error);
        assert!(tooltip.len() <= 50); // "CC: Error - " (12) + 30 = 42, plus some margin
    }

    #[test]
    fn test_truncate_str_short() {
        assert_eq!(truncate_str("hello", 10), "hello");
    }

    #[test]
    fn test_truncate_str_exact() {
        assert_eq!(truncate_str("hello", 5), "hello");
    }

    #[test]
    fn test_truncate_str_long() {
        assert_eq!(truncate_str("hello world", 5), "hello");
    }

    // Tests for themed progress bar functions (theme-agnostic)

    #[test]
    fn test_wide_progress_bar_empty() {
        let theme = current_theme();
        let bar = wide_progress_bar(0.0);
        let segments = usize::from(theme.menu_bar_segments);

        // Should have correct number of empty segments
        assert_eq!(
            bar.matches(theme.menu_bar_empty).count(),
            segments,
            "0% bar should have {} empty segments",
            segments
        );
        // Should have no full segments
        assert_eq!(
            bar.matches(theme.menu_bar_full).count(),
            0,
            "0% bar should have no full segments"
        );
    }

    #[test]
    fn test_wide_progress_bar_full() {
        let theme = current_theme();
        let bar = wide_progress_bar(100.0);
        let segments = usize::from(theme.menu_bar_segments);

        // Should have correct number of full segments
        assert_eq!(
            bar.matches(theme.menu_bar_full).count(),
            segments,
            "100% bar should have {} full segments",
            segments
        );
        // Should have no empty segments
        assert_eq!(
            bar.matches(theme.menu_bar_empty).count(),
            0,
            "100% bar should have no empty segments"
        );
    }

    #[test]
    fn test_wide_progress_bar_partial() {
        let theme = current_theme();
        let bar = wide_progress_bar(50.0);
        let segments = usize::from(theme.menu_bar_segments);

        // Should have total of segment count (full + empty)
        let full_count = bar.matches(theme.menu_bar_full).count();
        let empty_count = bar.matches(theme.menu_bar_empty).count();
        assert_eq!(
            full_count + empty_count,
            segments,
            "50% bar should have {} total segments",
            segments
        );
        // Should have roughly half full (allowing for rounding)
        assert!(full_count > 0, "50% bar should have some full segments");
        assert!(empty_count > 0, "50% bar should have some empty segments");
    }

    #[test]
    fn test_format_tray_label() {
        let usage = make_test_usage(17.0, 42.0);
        let label = format_tray_label(&usage);

        // Should contain percentage and time (theme-agnostic checks)
        assert!(label.contains("17%"));
        assert!(label.contains("2h")); // Reset time (2h 15m from test data)
    }

    #[test]
    fn test_format_section_header() {
        let header = format_section_header("TEST");
        // Should contain the title
        assert!(header.contains("TEST"));
    }

    #[test]
    fn test_format_loading_label() {
        let label = format_loading_label();
        // Should have 5 empty segments
        assert!(!label.is_empty());
    }

    #[test]
    fn test_format_error_label() {
        let label = format_error_label();
        assert!(label.contains("Error"));
    }

    #[test]
    fn test_format_time_compact_hours_and_minutes() {
        let future = Utc::now() + Duration::hours(2) + Duration::minutes(15);
        let result = format_time_compact(Some(future));
        // Allow for 1 minute variance due to test execution time
        assert!(result.starts_with("2h"));
        assert!(result.ends_with("m"));
    }

    #[test]
    fn test_format_time_compact_minutes_only() {
        let future = Utc::now() + Duration::minutes(45);
        let result = format_time_compact(Some(future));
        // Should be minutes only (no 'h')
        assert!(!result.contains('h'));
        assert!(result.ends_with("m"));
    }

    #[test]
    fn test_format_time_compact_none() {
        let result = format_time_compact(None);
        assert_eq!(result, "—");
    }

    #[test]
    fn test_format_time_compact_past() {
        let past = Utc::now() - Duration::hours(1);
        let result = format_time_compact(Some(past));
        assert_eq!(result, "0m");
    }

    #[test]
    fn test_format_current_time() {
        let time = format_current_time();
        // Should be in HH:MM format
        assert_eq!(time.len(), 5);
        assert!(time.contains(':'));
    }
}
