//! API types for Claude usage data.
//!
//! This module defines the data structures for usage statistics.

use chrono::{DateTime, Utc};
use serde::Deserialize;

/// Response containing all usage statistics from Claude.
#[derive(Debug, Deserialize, Clone)]
pub struct UsageResponse {
    /// Current session (5-hour window) usage.
    pub five_hour: UsageData,
    /// Weekly (7-day) usage across all models.
    pub seven_day: UsageData,
    /// Weekly usage for Sonnet model specifically (optional).
    #[serde(default)]
    pub seven_day_opus: Option<UsageData>,
}

/// Usage data for a specific time window.
#[derive(Debug, Deserialize, Clone)]
pub struct UsageData {
    /// Usage percentage (0.0 to 100.0).
    pub utilization: f64,
    /// When this usage window resets.
    pub resets_at: Option<DateTime<Utc>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_usage_response_deserialize() {
        let json = r#"{
            "five_hour": {"utilization": 6.0, "resets_at": "2025-11-04T04:59:59+00:00"},
            "seven_day": {"utilization": 35.0, "resets_at": "2025-11-06T03:59:59+00:00"},
            "seven_day_opus": {"utilization": 0.0, "resets_at": null}
        }"#;

        let usage: UsageResponse = serde_json::from_str(json).unwrap();
        assert!((usage.five_hour.utilization - 6.0).abs() < f64::EPSILON);
        assert!((usage.seven_day.utilization - 35.0).abs() < f64::EPSILON);
        assert!(usage.seven_day_opus.is_some());
    }

    #[test]
    fn test_usage_response_without_opus() {
        let json = r#"{
            "five_hour": {"utilization": 10.0, "resets_at": "2025-11-04T04:59:59+00:00"},
            "seven_day": {"utilization": 20.0, "resets_at": "2025-11-06T03:59:59+00:00"}
        }"#;

        let usage: UsageResponse = serde_json::from_str(json).unwrap();
        assert!(usage.seven_day_opus.is_none());
    }

    #[test]
    fn test_usage_data_with_null_reset() {
        let json = r#"{"utilization": 5.0, "resets_at": null}"#;
        let data: UsageData = serde_json::from_str(json).unwrap();
        assert!(data.resets_at.is_none());
    }
}
