//! Display formatting utilities for usage data.
//!
//! This module provides functions to format usage statistics for display
//! in tooltips, menus, and labels.

use crate::api::UsageResponse;
use chrono::{DateTime, Utc};

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
}
