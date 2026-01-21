//! Usage provider trait and mock implementation.
//!
//! This module defines the [`UsageProvider`] trait that abstracts how usage data
//! is fetched. Implementations include:
//! - [`ClaudeCliProvider`](crate::cli_provider::ClaudeCliProvider) - Production provider using CLI
//! - [`MockProvider`] - Test provider with configurable values

use crate::api::{UsageData, UsageResponse};
use async_trait::async_trait;
use chrono::{Duration, Utc};

/// Trait for fetching Claude Code usage statistics.
///
/// Implementations must be thread-safe (`Send + Sync`) to support async polling.
#[async_trait]
pub trait UsageProvider: Send + Sync {
    async fn fetch_usage(&self) -> Result<UsageResponse, String>;

    #[allow(dead_code)] // Reserved for provider identification/logging
    fn name(&self) -> &'static str;
}

#[async_trait]
impl UsageProvider for Box<dyn UsageProvider> {
    async fn fetch_usage(&self) -> Result<UsageResponse, String> {
        (**self).fetch_usage().await
    }

    fn name(&self) -> &'static str {
        (**self).name()
    }
}

/// Mock provider for testing and development.
///
/// Returns configurable static usage values with future reset times.
#[allow(dead_code)] // Available as fallback if CLI provider fails
pub struct MockProvider {
    five_hour: f64,
    seven_day: f64,
    opus: f64,
}

#[allow(dead_code)]
impl MockProvider {
    pub fn new(five_hour: f64, seven_day: f64, opus: f64) -> Self {
        Self {
            five_hour,
            seven_day,
            opus,
        }
    }

    pub fn default_demo() -> Self {
        Self::new(15.0, 42.0, 5.0)
    }
}

#[async_trait]
impl UsageProvider for MockProvider {
    async fn fetch_usage(&self) -> Result<UsageResponse, String> {
        let now = Utc::now();

        Ok(UsageResponse {
            five_hour: UsageData {
                utilization: self.five_hour,
                resets_at: Some(now + Duration::hours(2) + Duration::minutes(30)),
            },
            seven_day: UsageData {
                utilization: self.seven_day,
                resets_at: Some(now + Duration::days(3) + Duration::hours(8)),
            },
            seven_day_opus: Some(UsageData {
                utilization: self.opus,
                resets_at: Some(now + Duration::days(5)),
            }),
        })
    }

    fn name(&self) -> &'static str {
        "mock"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_mock_provider_returns_expected_values() {
        let provider = MockProvider::new(25.0, 50.0, 10.0);
        let usage = provider.fetch_usage().await.unwrap();

        assert!((usage.five_hour.utilization - 25.0).abs() < f64::EPSILON);
        assert!((usage.seven_day.utilization - 50.0).abs() < f64::EPSILON);
        assert!(usage.seven_day_opus.is_some());
        assert!((usage.seven_day_opus.unwrap().utilization - 10.0).abs() < f64::EPSILON);
    }

    #[tokio::test]
    async fn test_mock_provider_reset_times_are_in_future() {
        let provider = MockProvider::default_demo();
        let usage = provider.fetch_usage().await.unwrap();
        let now = Utc::now();

        assert!(usage.five_hour.resets_at.unwrap() > now);
        assert!(usage.seven_day.resets_at.unwrap() > now);
    }

    #[test]
    fn test_mock_provider_name() {
        let provider = MockProvider::default_demo();
        assert_eq!(provider.name(), "mock");
    }

    #[tokio::test]
    async fn test_mock_provider_includes_opus() {
        let provider = MockProvider::default_demo();
        let usage = provider.fetch_usage().await.unwrap();

        assert!(usage.seven_day_opus.is_some());
        let opus = usage.seven_day_opus.unwrap();
        assert!((opus.utilization - 5.0).abs() < f64::EPSILON);
    }
}
