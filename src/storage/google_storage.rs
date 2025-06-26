mod auth;

use anyhow::{Result, anyhow};
use auth::{GoogleAuthClient, WorkloadIdentityConfig};
use reqwest::{Body, Client};
use tracing::{debug, error, info};

pub struct GoogleStorageClient {
    auth_client: GoogleAuthClient,
    client: Client,
    bucket: String,
}

impl GoogleStorageClient {
    /// Create a new GoogleStorageClient instance
    ///
    /// # Arguments
    /// * `config` - WorkloadIdentityConfig for authentication
    ///
    /// # Returns
    /// * `GoogleStorageClient` - New client instance
    pub fn new() -> Result<Self> {
        let workload_identity_config = WorkloadIdentityConfig::default();

        if !workload_identity_config.enabled() {
            debug!("client not configured");
            return Err(anyhow!("cannot create client"));
        }

        let bucket = std::env::var("PARQUET_STORAGE_BUCKET")?;

        Ok(Self {
            auth_client: GoogleAuthClient::new(workload_identity_config),
            client: Client::new(),
            bucket,
        })
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
        object_name: &str,
        data: &[u8],
        content_type: Option<&str>,
    ) -> Result<()> {
        let content_type = content_type.unwrap_or("application/octet-stream");

        let (bucket, prefix) = match self.bucket.split_once("/") {
            None => (self.bucket.clone(), "".to_string()),
            Some((a, b)) => (a.to_string(), b.to_string()),
        };

        let object_name = match prefix.len() {
            0 => object_name.to_string(),
            _ => format!("{prefix}/{object_name}"),
        };

        let url = format!(
            "https://storage.googleapis.com/upload/storage/v1/b/{}/o?uploadType=media&name={}",
            bucket,
            urlencoding::encode(&object_name)
        );

        debug!(
            "Uploading binary data to GCS: bucket={}, object={}, size={} bytes",
            bucket,
            object_name,
            data.len(),
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
            bucket, object_name
        );
        Ok(())
    }
}
