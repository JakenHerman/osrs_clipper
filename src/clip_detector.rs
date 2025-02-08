use anyhow::{ensure, Result};
use log::debug;
use std::process::Command;
use tokio::{
    sync::mpsc,
    time::Duration,
};

use opencv::{core, imgcodecs, imgproc, prelude::*, videoio};

use crate::s3_uploader::S3Uploader;

/// Continuously processes the video stream for events.
/// When an event is detected, a message is sent on the channel.
fn spawn_detection_task(
    video_url: String,
    template_paths: Vec<String>,
    threshold: f32,
) -> mpsc::Receiver<()> {
    let (tx, rx) = mpsc::channel::<()>(10);

    // Spawn a task to process frames.
    tokio::task::spawn_blocking(move || -> opencv::Result<()> {
        let templates = load_templates(&template_paths)?;
        if templates.is_empty() {
          return Err(opencv::Error::new(opencv::core::StsError, "No valid templates loaded. Exiting detection task.".to_string()));
        }

        // Open the video capture.
        // For a Twitch stream, the video_url is the HLS URL.
        let mut cap = videoio::VideoCapture::from_file(&video_url, videoio::CAP_FFMPEG)?;
        
        if !cap.is_opened()? {
            return Err(opencv::Error::new(opencv::core::StsError, format!("Failed to open video stream at {}", video_url)));
        }

        loop {
            let mut frame = core::Mat::default();
            cap.read(&mut frame)?;
            if frame.empty() {
                // If no frame was captured, sleep briefly and try again.
                std::thread::sleep(Duration::from_millis(100));
                continue;
            }

            let mut event_detected = false;

            // Iterate over all templates.
            for template in &templates {
                let mut result = core::Mat::default();
                imgproc::match_template(
                    &frame,
                    template,
                    &mut result,
                    imgproc::TM_CCOEFF_NORMED,
                    &core::no_array(),
                )?;

                let mut min_val = 0.0;
                let mut max_val = 0.0;
                let mut min_loc = core::Point::default();
                let mut max_loc = core::Point::default();
                core::min_max_loc(
                    &result,
                    Some(&mut min_val),
                    Some(&mut max_val),
                    Some(&mut min_loc),
                    Some(&mut max_loc),
                    &core::no_array(),
                )?;

                debug!("Template match max value for one template: {}", max_val);

                if max_val >= threshold as f64 {
                    event_detected = true;
                    break;
                }
            }

            if event_detected {
                println!("Event detected by one of the templates!");
                // Attempt to send an event notification. If the channel is full, ignore the send.
                // It's too late at night for me to think about how to actually handle a full channel.
                let _ = tx.try_send(());
            }

            // Small delay to reduce CPU usage.
            std::thread::sleep(Duration::from_millis(100));
        }
    });

    rx
}

/// Generates a clip using FFmpeg given the start time and duration from a video stream.
/// This command extracts a segment from the video stream.
pub async fn generate_clip(
    video_url: &str,
    output_path: &str,
    start: u32,
    duration: u32,
) -> Result<()> {
    let status = Command::new("ffmpeg")
        .args(&[
            "-y", // Overwrite output if it exists
            "-i",
            video_url, // Input video stream URL
            "-ss",
            &start.to_string(), // Start time
            "-t",
            &duration.to_string(), // Duration
            "-c",
            "copy",      // Copy codec (faster extraction)
            output_path, // Output file path
        ])
        .status()?;

    ensure!(
        status.success(),
        "FFmpeg failed to generate clip from stream"
    );
    Ok(())
}

/// Loads multiple template images given their file paths.
fn load_templates(paths: &[String]) -> opencv::Result<Vec<core::Mat>> {
    let mut templates = Vec::new();
    for path in paths {
        let template = imgcodecs::imread(&path, imgcodecs::IMREAD_COLOR)?;
        if template.empty() {
            eprintln!("Warning: Template {} is empty or not found.", path);
        } else {
            templates.push(template);
        }
    }
    Ok(templates)
}

/// Processes the video stream in realtime.
/// When an event is detected, generate a clip and upload it to S3.
pub async fn process_video_stream_realtime(video_url: &str, s3_bucket: String) -> Result<()> {
    // Create an S3 uploader instance.
    let uploader = S3Uploader::new(&s3_bucket, Some("http://127.0.0.1:9000")).await?;

    // Grab every file in templates/
    let template_paths = std::fs::read_dir("templates")?
        .filter_map(|entry| {
            if let Ok(entry) = entry {
                if let Some(path) = entry.path().to_str() {
                    return Some(path.to_string());
                }
            }
            None
        })
        .collect::<Vec<String>>();

    // Threshold for detection... not sure where to set this right now.
    let detection_threshold = 0.5_f32;

    // Spawn the detection task.
    let mut detection_rx =
        spawn_detection_task(video_url.to_string(), template_paths.iter().map(|s| s.to_string()).collect(), detection_threshold);

    // Listen for detection events.
    while let Some(()) = detection_rx.recv().await {
        println!("Processing detected event: generating clip.");
        // Todo: Implement a more sophisticated event naming mechanism.
        let clip_output = "highlight_clip.mp4";
        // Generate the clip from the video stream.
        // Todo: Implement a more sophisticated clip generation mechanism that considers the event time.
        generate_clip(video_url, clip_output, 0, 30).await?;
        // Upload the generated clip to S3.
        let clip_url = uploader.upload_file(clip_output, clip_output).await?;
        println!("Generated and uploaded clip: {}", clip_url);
    }
    Ok(())
}
