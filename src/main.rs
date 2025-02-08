mod api;
mod clip_detector;
mod s3_querier;
mod s3_uploader;

use anyhow::Result;
use tokio::join;
use env_logger;

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();

    // Use Streamlink to get HLS stream
    let twitch_user = "jakenherman";
    let hls = std::process::Command::new("streamlink")
        .args(&[
            &format!("https://www.twitch.tv/{}", twitch_user),
            "best",
            "--stream-url"
        ])
        .output()?;

    let s3_bucket = "clips".to_string();
    let video_path = String::from_utf8(hls.stdout)?;
    
    // Spawn the video processing task.
    let processing = tokio::spawn(async move {
        if let Err(e) = clip_detector::process_video_stream(&video_path, s3_bucket).await {
            eprintln!("Video processing error: {:?}", e);
        }
    });

    let api_server = api::run_api_server();
    
    // Run processing and API server concurrently
    let _ = join!(processing, api_server);
    Ok(())
}