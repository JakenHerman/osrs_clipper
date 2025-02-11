use anyhow::Result;
use aws_config::{meta::region::RegionProviderChain, Region};
use aws_sdk_transcribestreaming::Client;
use log::debug;

use crate::s3_uploader::{self, S3Uploader};

pub async fn generate_client_and_s3_uploader(
    hls_url: &str,
    s3_bucket: &str,
) -> Result<(Client, S3Uploader)> {
    let s3_uploader = s3_uploader::S3Uploader::new(&s3_bucket, Some("http://127.0.0.1:9000"))
        .await
        .unwrap();

    debug!("Transcribing stream from HLS URL: {}", hls_url);
    let region_provider = RegionProviderChain::first_try(Some("us-east-1").map(Region::new))
        .or_default_provider()
        .or_else(Region::new("us-east-1"));

    let shared_config = aws_config::from_env().region(region_provider).load().await;
    let client = Client::new(&shared_config);

    Ok((client, s3_uploader))
}
