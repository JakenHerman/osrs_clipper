mod api;
mod clip_detector;
mod s3_querier;
mod s3_uploader;
mod transcribe;

use anyhow::Result;
use env_logger;
use tokio::join;

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();
    let _ = tracing_subscriber::fmt::try_init();

    // Use Streamlink to get HLS stream
    let twitch_user = "mmorpg";
    let hls = std::process::Command::new("streamlink")
        .args(&[
            &format!("https://www.twitch.tv/{}", twitch_user),
            "best",
            "--stream-url",
        ])
        .output()?;

    let s3_bucket = "clips".to_string();
    let video_path = String::from_utf8(hls.stdout)?;

    // Spawn the video processing task.
    let processing = tokio::spawn(async move {
        println!("Spawning video processing task");
        if let Err(e) = transcribe::transcribe_stream_and_save(&video_path).await {
            eprintln!("Video processing error: {:?}", e);
        }
    });

    let api_server = api::run_api_server();

    // Run processing and API server concurrently
    let _ = join!(processing, api_server);
    Ok(())
}
