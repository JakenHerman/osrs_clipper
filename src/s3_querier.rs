use anyhow::Result;
use aws_config::BehaviorVersion;
use aws_sdk_s3::Client;
use aws_types::region::Region;

use crate::api::Clip;

pub struct S3Querier {
    client: Client,
    bucket: String,
    /// Optional endpoint override (e.g. "http://127.0.0.1:9000" for MinIO)
    endpoint: Option<String>,
}

impl S3Querier {
    /// Creates a new S3Querier.
    ///
    /// * `bucket` - The S3 bucket name.
    /// * `endpoint` - An optional endpoint override (e.g. Some("http://127.0.0.1:9000")
    ///   to use a local S3-compatible service like MinIO).
    pub async fn new(bucket: &str, endpoint: Option<&str>) -> Result<Self> {
        let mut config = aws_sdk_s3::Config::builder()
            .region(Region::new("us-east-1"))
            .behavior_version(BehaviorVersion::v2024_03_28());

        if let Some(ep) = endpoint {
            config = config.endpoint_url(ep);
        }

        let client = Client::from_conf(config.build());

        Ok(S3Querier {
            client,
            bucket: bucket.to_string(),
            endpoint: endpoint.map(|s| s.to_string()),
        })
    }

    /// Lists the object keys in the bucket.
    pub async fn list_clips(&self) -> Result<Vec<Clip>> {
        let resp = self
            .client
            .list_objects_v2()
            .bucket(&self.bucket)
            .send()
            .await?;

        let mut clips = Vec::new();
        let mut clip = Clip {
            ..Default::default()
        };

        for obj in resp.contents.unwrap_or_default() {
            let dir = obj.key.as_deref().unwrap_or_default().split("/").next().unwrap_or_default();
            if clip.id.is_empty() {
                clip.id = dir.to_string();
            } else if clip.id != dir {
                clips.push(clip);
                clip = Clip {
                    ..Default::default()
                };
                clip.id = dir.to_string();
            }

            let key = obj.key.as_deref().unwrap_or_default().split("/").last().unwrap_or_default();
            // match last 3 characters of key to determine type
            let key_type = &key[key.len() - 3..];
            match key_type {
                "mp4" => {
                    clip.video.id = key.to_string();
                    clip.video.s3_url = format!(
                        "{}/clips/{}",
                        self.endpoint
                            .clone()
                            .unwrap_or("https://s3.amazonaws.com".to_string()),
                        key
                    );
                }
                "srt" => {
                    clip.transcript.id = key.to_string();
                    clip.transcript.s3_url = format!(
                        "{}/clips/{}",
                        self.endpoint
                            .clone()
                            .unwrap_or("https://s3.amazonaws.com".to_string()),
                        key
                    );
                }
                "wav" => {
                    clip.audio.id = key.to_string();
                    clip.audio.s3_url = format!(
                        "{}/clips/{}",
                        self.endpoint
                            .clone()
                            .unwrap_or("https://s3.amazonaws.com".to_string()),
                        key
                    );
                }
                _ => {}
            }
        }
        Ok(clips)
    }
}
