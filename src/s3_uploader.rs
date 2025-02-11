use anyhow::Result;
use aws_config::BehaviorVersion;
use aws_sdk_s3::primitives::ByteStream;
use aws_sdk_s3::Client;
use aws_types::region::Region;
use tokio::fs;

pub struct S3Uploader {
    client: Client,
    bucket: String,
    /// Optional endpoint override (for example, "http://127.0.0.1:9000" for MinIO)
    endpoint: Option<String>,
}

impl S3Uploader {
    /// Creates a new S3Uploader.
    ///
    /// * `bucket` - The S3 bucket name.
    /// * `endpoint` - An optional endpoint override (pass, for example, Some("http://127.0.0.1:9000")
    ///   to use a local S3-compatible service like MinIO).
    pub async fn new(bucket: &str, endpoint: Option<&str>) -> Result<Self> {
        let mut config = aws_sdk_s3::Config::builder()
            .region(Region::new("us-east-1"))
            .behavior_version(BehaviorVersion::v2024_03_28());

        if let Some(ep) = endpoint {
            config = config.endpoint_url(ep);
        }

        let client = Client::from_conf(config.build());

        Ok(S3Uploader {
            client,
            bucket: bucket.to_string(),
            endpoint: endpoint.map(|s| s.to_string()),
        })
    }

    /// Uploads the file at `file_path` to the S3 bucket using the specified `object_key`.
    ///
    /// Returns the URL where the object is available.
    pub async fn upload_file(&self, file_path: &str, object_key: &str) -> Result<String> {
        // Read the file's contents.
        let file_bytes = fs::read(file_path).await?;
        let body = ByteStream::from(file_bytes);

        // Upload the file.
        self.client
            .put_object()
            .bucket(&self.bucket)
            .key(object_key)
            .body(body)
            .send()
            .await?;

        // Construct the URL for the uploaded object.
        let url = if let Some(ref ep) = self.endpoint {
            let trimmed = ep.trim_end_matches('/');
            format!("{}/{}/{}", trimmed, self.bucket, object_key)
        } else {
            format!("https://{}.s3.amazonaws.com/{}", self.bucket, object_key)
        };

        println!("Successfully uploaded file to {}", url);
        Ok(url)
    }
}
