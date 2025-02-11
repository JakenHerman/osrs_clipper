use anyhow::Result;
use log::{debug, error};

/// Convert seconds to total milliseconds.
pub fn format_timestamp(seconds: f64) -> String {
    let total_millis = (seconds * 1000.0).round() as u64;
    let hours = total_millis / 3_600_000;
    let minutes = (total_millis % 3_600_000) / 60_000;
    let secs = (total_millis % 60_000) / 1000;
    let millis = total_millis % 1000;
    format!("{:02}:{:02}:{:02},{:03}", hours, minutes, secs, millis)
}

pub async fn run_ffmpeg(args: &[&str]) -> Result<()> {
    let output = tokio::process::Command::new("ffmpeg")
        .args(args)
        .output()
        .await?;

    debug!("ffmpeg stdout: {}", String::from_utf8_lossy(&output.stdout));
    error!("ffmpeg stderr: {}", String::from_utf8_lossy(&output.stderr));

    Ok(())
}
