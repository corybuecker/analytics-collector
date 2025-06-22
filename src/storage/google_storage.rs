use anyhow::{Result, anyhow};
use reqwest::{Body, Client};
use tracing::{debug, error, info};

use crate::utilities::google_auth::{GoogleAuthClient, WorkloadIdentityConfig};

pub struct GoogleStorageClient {
    auth_client: GoogleAuthClient,
    client: Client,
}

impl GoogleStorageClient {
    /// Create a new GoogleStorageClient instance
    ///
    /// # Arguments
    /// * `config` - WorkloadIdentityConfig for authentication
    ///
    /// # Returns
    /// * `GoogleStorageClient` - New client instance
    pub fn new(config: WorkloadIdentityConfig) -> Self {
        Self {
            auth_client: GoogleAuthClient::new(config),
            client: Client::new(),
        }
    }

    /// Upload binary data to Google Cloud Storage
    ///
    /// # Arguments
    /// * `bucket_name` - The name of the GCS bucket
    /// * `object_name` - The name/path of the object in the bucket
    /// * `data` - The binary data as a byte slice
    /// * `content_type` - Optional content type (defaults to "application/octet-stream")
    ///
    /// # Returns
    /// * `Result<()>` - Ok if upload succeeded, Err if failed
    pub async fn upload_binary_data(
        &mut self,
        bucket_name: &str,
        object_name: &str,
        data: &[u8],
        content_type: Option<&str>,
    ) -> Result<()> {
        let content_type = content_type.unwrap_or("application/octet-stream");

        let url = format!(
            "https://storage.googleapis.com/upload/storage/v1/b/{}/o?uploadType=media&name={}",
            bucket_name, object_name
        );

        debug!(
            "Uploading binary data to GCS: bucket={}, object={}, size={} bytes",
            bucket_name,
            object_name,
            data.len()
        );

        let response = self
            .client
            .post(&url)
            .header(
                "Authorization",
                format!("Bearer {}", self.auth_client.get_access_token().await?),
            )
            .header("Content-Type", content_type)
            .header("Content-Length", data.len().to_string())
            .body(Body::from(data.to_vec()))
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            error!(
                "Binary data upload failed with status {}: {}",
                status, error_text
            );
            return Err(anyhow!(
                "Binary data upload failed: {} - {}",
                status,
                error_text
            ));
        }

        info!(
            "Successfully uploaded binary data to GCS: bucket={}, object={}",
            bucket_name, object_name
        );
        Ok(())
    }
}
