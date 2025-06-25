//! Google Cloud Storage client with Workload Identity Federation
//!
//! This module provides functionality to exchange Kubernetes service account tokens
//! for Google Cloud access tokens using workload identity federation, which can then
//! be used to authenticate with Google Cloud Storage APIs.
//!
//! # Usage Example
//!
//! ```rust,no_run
//! use analytics_collector::storage::google_storage::{GoogleStorageClient, WorkloadIdentityConfig};
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let config = WorkloadIdentityConfig {
//!         audience: "//iam.googleapis.com/projects/123456789/locations/global/workloadIdentityPools/my-pool/providers/my-provider".to_string(),
//!         service_account_token_path: "/var/run/secrets/kubernetes.io/serviceaccount/token".to_string(),
//!         sts_endpoint: "https://sts.googleapis.com/v1/token".to_string(),
//!     };
//!
//!     let mut client = GoogleStorageClient::new(config);
//!
//!     // Exchange the K8s service account token for a Google Cloud access token
//!     let access_token = client.exchange_token().await?;
//!     println!("Access token: {}", access_token);
//!
//!     // Subsequent calls will use the cached token if it hasn't expired
//!     let cached_token = client.get_access_token().await?;
//!
//!     Ok(())
//! }
//! ```
//!
//! # Workload Identity Federation
//!
//! This implementation follows the standard workload identity federation flow:
//! 1. Read the Kubernetes service account token from the mounted volume
//! 2. Exchange it for a Google Cloud access token using Google's STS endpoint
//! 3. Cache the access token until it expires (with a 5-minute buffer)
//! 4. Use the access token to authenticate with Google Cloud services

use anyhow::{Result, anyhow};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::fs;
use tracing::{debug, error, info};

/// Token exchange request payload for workload identity federation
#[derive(Debug, Serialize)]
struct TokenExchangeRequest {
    audience: String,
    grant_type: String,
    requested_token_type: String,
    subject_token: String,
    subject_token_type: String,
    scope: String,
}

/// Token exchange response from Google's STS endpoint
#[derive(Debug, Deserialize)]
struct TokenExchangeResponse {
    access_token: String,
    expires_in: u64,
}

/// Configuration for workload identity federation
#[derive(Debug, Clone)]
pub struct WorkloadIdentityConfig {
    pub audience: Option<String>,
    pub service_account_token_path: String,
    pub sts_endpoint: String,
}

impl Default for WorkloadIdentityConfig {
    fn default() -> Self {
        Self {
            audience: Some(String::new()),
            service_account_token_path: "/var/run/secrets/kubernetes.io/serviceaccount/token"
                .to_string(),
            sts_endpoint: "https://sts.googleapis.com/v1/token".to_string(),
        }
    }
}

impl WorkloadIdentityConfig {
    pub fn audience(&self) -> Result<String> {
        match &self.audience {
            None => Err(anyhow!("audience must be configured")),
            Some(s) => Ok(s.clone()),
        }
    }
}

/// Cached access token with expiration
#[derive(Debug, Clone)]
pub struct AccessToken {
    pub token: String,
    pub expires_at: SystemTime,
}

impl AccessToken {
    /// Check if the token is expired (with 5 minute buffer)
    pub fn is_expired(&self) -> bool {
        match SystemTime::now().duration_since(UNIX_EPOCH) {
            Ok(now) => {
                match self.expires_at.duration_since(UNIX_EPOCH) {
                    Ok(expires) => now.as_secs() + 300 >= expires.as_secs(), // 5 minute buffer
                    Err(_) => true,
                }
            }
            Err(_) => true,
        }
    }
}

/// Google Cloud Storage client with workload identity federation
pub struct GoogleAuthClient {
    client: Client,
    config: WorkloadIdentityConfig,
    cached_token: Option<AccessToken>,
}

impl GoogleAuthClient {
    /// Create a new Google Storage client with workload identity configuration
    pub fn new(config: WorkloadIdentityConfig) -> Self {
        Self {
            client: Client::new(),
            config,
            cached_token: None,
        }
    }

    /// Exchange a Kubernetes service account token for a Google Cloud access token
    /// using workload identity federation
    pub async fn exchange_token(&mut self) -> Result<String> {
        // Check if we have a valid cached token
        if let Some(ref token) = self.cached_token {
            if !token.is_expired() {
                debug!("Using cached access token");
                return Ok(token.token.clone());
            }
        }

        info!("Exchanging Kubernetes service account token for Google Cloud access token");

        // Read the Kubernetes service account token
        let k8s_token = self.read_service_account_token().await?;

        // Prepare the token exchange request
        let request = TokenExchangeRequest {
            audience: self.config.audience()?,
            grant_type: "urn:ietf:params:oauth:grant-type:token-exchange".to_string(),
            requested_token_type: "urn:ietf:params:oauth:token-type:access_token".to_string(),
            subject_token: k8s_token,
            subject_token_type: "urn:ietf:params:oauth:token-type:jwt".to_string(),
            scope: "https://www.googleapis.com/auth/cloud-platform".to_string(),
        };

        debug!(
            "Making token exchange request to: {}",
            self.config.sts_endpoint
        );

        // Make the token exchange request
        let response = self
            .client
            .post(&self.config.sts_endpoint)
            .header("Content-Type", "application/x-www-form-urlencoded")
            .form(&request)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            error!(
                "Token exchange failed with status {}: {}",
                status, error_text
            );
            return Err(anyhow!(
                "Token exchange failed: {} - {}",
                status,
                error_text
            ));
        }

        let token_response: TokenExchangeResponse = response.json().await?;

        // Calculate expiration time
        let expires_at = SystemTime::now()
            .checked_add(Duration::from_secs(token_response.expires_in))
            .unwrap_or_else(|| SystemTime::now() + Duration::from_secs(3600)); // Default to 1 hour

        // Cache the token
        let access_token = AccessToken {
            token: token_response.access_token.clone(),
            expires_at,
        };
        self.cached_token = Some(access_token);

        info!(
            "Successfully exchanged token, expires in {} seconds",
            token_response.expires_in
        );
        Ok(token_response.access_token)
    }

    /// Read the Kubernetes service account token from the filesystem
    async fn read_service_account_token(&self) -> Result<String> {
        debug!(
            "Reading service account token from: {}",
            self.config.service_account_token_path
        );

        let token = fs::read_to_string(&self.config.service_account_token_path)
            .await
            .map_err(|e| {
                error!("Failed to read service account token: {}", e);
                anyhow!(
                    "Failed to read service account token from {}: {}",
                    self.config.service_account_token_path,
                    e
                )
            })?;

        let token = token.trim().to_string();

        if token.is_empty() {
            return Err(anyhow!("Service account token is empty"));
        }

        debug!(
            "Successfully read service account token ({} characters)",
            token.len()
        );
        Ok(token)
    }

    /// Get a valid access token (uses cache if available and not expired)
    pub async fn get_access_token(&mut self) -> Result<String> {
        self.exchange_token().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_access_token_expiration() {
        let expired_token = AccessToken {
            token: "test_token".to_string(),
            expires_at: SystemTime::now() - Duration::from_secs(3600), // 1 hour ago
        };
        assert!(expired_token.is_expired());

        let valid_token = AccessToken {
            token: "test_token".to_string(),
            expires_at: SystemTime::now() + Duration::from_secs(3600), // 1 hour from now
        };
        assert!(!valid_token.is_expired());

        // Test with buffer (should be expired if less than 5 minutes remaining)
        let soon_expired_token = AccessToken {
            token: "test_token".to_string(),
            expires_at: SystemTime::now() + Duration::from_secs(200), // 3 minutes from now
        };
        assert!(soon_expired_token.is_expired());
    }

    #[test]
    fn test_workload_identity_config_default() {
        let config = WorkloadIdentityConfig::default();
        assert_eq!(
            config.service_account_token_path,
            "/var/run/secrets/kubernetes.io/serviceaccount/token"
        );
        assert_eq!(config.sts_endpoint, "https://sts.googleapis.com/v1/token");
        assert!(config.audience.unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_google_storage_client_creation() {
        let config = WorkloadIdentityConfig {
            audience: Some("//iam.googleapis.com/projects/123456789/locations/global/workloadIdentityPools/my-pool/providers/my-provider".to_string()),
            ..Default::default()
        };

        let client = GoogleAuthClient::new(config.clone());
        assert_eq!(client.config.audience, config.audience);
        assert!(client.cached_token.is_none());
    }
}
