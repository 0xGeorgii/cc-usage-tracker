//! OAuth credential management for Claude API access.
//!
//! This module reads Claude Code credentials from `~/.claude/.credentials.json`.
//! Currently reserved for when Anthropic enables external OAuth API access.

#![allow(dead_code)] // Reserved for API provider when OAuth is enabled

use serde::Deserialize;
use std::fs;
use std::path::PathBuf;

/// Top-level credentials structure from `.credentials.json`.
#[derive(Debug, Deserialize)]
pub struct Credentials {
    #[serde(rename = "claudeAiOauth")]
    pub claude_ai_oauth: Option<OAuthCredentials>,
}

/// OAuth token credentials from Claude AI.
#[derive(Debug, Deserialize)]
pub struct OAuthCredentials {
    /// Bearer token for API authentication.
    #[serde(rename = "accessToken")]
    pub access_token: String,
    /// Token for refreshing expired access tokens.
    #[allow(dead_code)] // Reserved for future token refresh
    #[serde(rename = "refreshToken")]
    pub refresh_token: String,
    /// Unix timestamp when the access token expires.
    #[allow(dead_code)] // Reserved for future token expiry check
    #[serde(rename = "expiresAt")]
    pub expires_at: i64,
}

/// Get the path to the Claude credentials file.
pub fn get_credentials_path() -> Option<PathBuf> {
    dirs::home_dir().map(|h| h.join(".claude").join(".credentials.json"))
}

/// Read and parse the credentials file.
pub fn read_credentials() -> Result<Credentials, String> {
    let path = get_credentials_path().ok_or("Could not determine home directory")?;

    if !path.exists() {
        return Err(format!(
            "Credentials file not found at {}. Please log in to Claude Code first.",
            path.display()
        ));
    }

    let content =
        fs::read_to_string(&path).map_err(|e| format!("Failed to read credentials file: {e}"))?;

    serde_json::from_str(&content).map_err(|e| format!("Failed to parse credentials JSON: {e}"))
}

/// Get the OAuth access token from credentials.
pub fn get_access_token() -> Result<String, String> {
    let creds = read_credentials()?;

    let oauth = creds
        .claude_ai_oauth
        .ok_or("No OAuth credentials found. Please log in to Claude Code.")?;

    Ok(oauth.access_token)
}
