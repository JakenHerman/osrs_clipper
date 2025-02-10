#![allow(clippy::result_large_err)]

use anyhow::{bail, ensure, Result};
use aws_config::meta::region::RegionProviderChain;
use aws_sdk_transcribestreaming::types::{
    LanguageCode, MediaEncoding, TranscriptResultStream,
};
use aws_sdk_transcribestreaming::{config::Region, Client, Error};
use log::debug;
use std::fs::File;
use std::io::{Write};
use std::time::Duration;


const CHUNK_SIZE: usize = 8192;


/// Transcribes an HLS stream and saves the transcribed message to a txt file
pub async fn transcribe_stream_and_save(hls_url: &str) -> Result<(), Error> {

    debug!("Transcribing stream from HLS URL: {}", hls_url);
    let region_provider = RegionProviderChain::first_try(Some("us-east-1").map(Region::new))
        .or_default_provider()
        .or_else(Region::new("us-west-2"));

    let shared_config = aws_config::from_env().region(region_provider).load().await;
    let client = Client::new(&shared_config);
    debug!("Created client");
    let hls_url_clone = hls_url.to_string();
    let output = tokio::task::spawn_blocking(move || {
        std::process::Command::new("ffmpeg")
            .args(&[
                "-i", &hls_url_clone,
                "-t", "15",
                "-map", "0:a",
                "-c:a", "pcm_s16le",
                "-ar", "16000",
                "-ac", "1",
                "-f", "s16le",
                "output.raw",
                "-map", "0:v",
                "-c:v", "copy",
                "-t", "15",
                "output.mp4",
            ])
            .output()
            .expect("Failed to run ffmpeg")
    }).await.unwrap();

    let input_stream = async_stream::stream! {
        let pcm = pcm_data_raw("output.raw");
        for chunk in pcm.chunks(CHUNK_SIZE) {
            tokio::time::sleep(Duration::from_millis(100)).await;
            yield Ok(aws_sdk_transcribestreaming::types::AudioStream::AudioEvent(
                aws_sdk_transcribestreaming::types::AudioEvent::builder()
                    .audio_chunk(aws_sdk_transcribestreaming::primitives::Blob::new(chunk))
                    .build(),
            ));
        }
    };

    debug!("Obtained input stream");

    let mut output = client
        .start_stream_transcription()
        .language_code(LanguageCode::EnGb)
        .media_sample_rate_hertz(16000)
        .media_encoding(MediaEncoding::Pcm)
        .audio_stream(input_stream.into())
        .send()
        .await?;

    debug!("Transcribing audio file");
    let mut full_message = String::new();

    // create a filewriter to write the transcribed message to a file and save the file in the S3 bucket
    let mut file_writer = std::fs::File::create("transcribed_message.txt").unwrap();

    debug!("Created filewriter to write transcribed message to file");
    while let Some(event) = output.transcript_result_stream.recv().await? {
        match event {
            TranscriptResultStream::TranscriptEvent(transcript_event) => {
                let transcript = transcript_event.transcript.unwrap();
                for result in transcript.results.unwrap_or_default() {
                    debug!("Transcript result: {:?}", result);
                    if !result.is_partial {
                        let first_alternative = &result.alternatives.as_ref().unwrap()[0];
                        full_message += first_alternative.transcript.as_ref().unwrap();
                        full_message.push('\n');
                        file_writer.write_all(first_alternative.transcript.as_ref().unwrap().as_bytes()).unwrap();
                    }
                }
            }
            otherwise => panic!("received unexpected event type: {:?}", otherwise), // todo: handle this better
        }
    }

    // store the raw file as .wav as well:
    std::process::Command::new("ffmpeg")
        .args(&[
            "-f", "s16le",
            "-ar", "16000",
            "-ac", "1",
            "-i", "output.raw",
            "output.wav",
        ])
        .output()
        .expect("Failed to run ffmpeg");

    debug!("\nFully transcribed message:\n\n{}", full_message);

    // now that transcription is complete, let's add the subtitles to the video
    add_subtitles_to_video("output.mp4", "transcribed_message.txt").unwrap();

    Ok(())
}

fn add_subtitles_to_video(video_path: &str, subtitle_path: &str) -> Result<()> {
    debug!("Adding subtitles to video");

    // verify that the transcription text is not empty (i.e. is streamer AFK?)
    ensure!(
        std::fs::metadata(subtitle_path)?.len() > 0,
        "Transcription text is empty"
    );
    
    // obtain the absolute path of the video and subtitle files
    let binding = std::fs::canonicalize(video_path)?;
    let video_path = binding.to_str().unwrap();
    let binding = std::fs::canonicalize(subtitle_path)?;
    let subtitle_path = binding.to_str().unwrap();

    // If video_path is a live stream, specify a time limit (for example, 10 seconds)
    // If video_path is a finite file, you might omit "-t" (or adjust it to the desired clip length)
    let args = [
        "-y",                       // Overwrite output if exists
        "-i", video_path,           // Input video
        "-vf", &format!("subtitles='{}'", subtitle_path), // Apply subtitles filter
        "-t", "15",                 // Limit duration to 10 seconds (remove if not needed)
        "output_with_subtitles.mp4" // Output file
    ];

    let output = std::process::Command::new("ffmpeg")
        .args(&args)
        .output()
        .expect("Failed to run ffmpeg");

    // Print ffmpeg's stdout and stderr for debugging purposes
    debug!("ffmpeg stdout: {}", String::from_utf8_lossy(&output.stdout));
    debug!("ffmpeg stderr: {}", String::from_utf8_lossy(&output.stderr));

    // Optionally, check if output file exists and report an error if not.
    if !std::path::Path::new("output_with_subtitles.mp4").exists() {
        bail!("ffmpeg did not produce output_with_subtitles.mp4");
    }

    Ok(())
}

fn pcm_data_raw(file_path: &str) -> Vec<u8> {
  std::fs::read(file_path).unwrap()
}