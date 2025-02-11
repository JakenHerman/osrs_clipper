#![allow(clippy::result_large_err)]

use anyhow::{ensure, Result};
use aws_sdk_transcribestreaming::operation::start_stream_transcription::{
    StartStreamTranscription, StartStreamTranscriptionOutput,
};
use aws_sdk_transcribestreaming::types::{LanguageCode, MediaEncoding, TranscriptResultStream};
use aws_sdk_transcribestreaming::Error;
use log::debug;
use std::io::Write;
use std::time::Duration;

use crate::aws::generate_client_and_s3_uploader;
use crate::utilities::{format_timestamp, run_ffmpeg};

const CHUNK_SIZE: usize = 8192;

pub fn get_initial_args(hls_url: &str) -> Vec<&str> {
    let initial_args = vec![
        "-i",
        &hls_url,
        "-t",
        "5",
        "-map",
        "0:a",
        "-c:a",
        "pcm_s16le",
        "-ar",
        "16000",
        "-ac",
        "1",
        "-f",
        "s16le",
        "output.raw",
        "-map",
        "0:v",
        "-c:v",
        "copy",
        "-t",
        "5",
        "output.mp4",
    ];
    initial_args
}

/// Transcribes an HLS stream and saves the transcribed message to a txt file
pub async fn transcribe_stream_and_save(hls_url: &str, s3_bucket: &str) -> Result<(), Error> {
    let (client, s3_uploader) = generate_client_and_s3_uploader(hls_url, s3_bucket)
        .await
        .unwrap();
    debug!("Created client");

    let hls_url_clone = hls_url.to_string();
    let output = tokio::task::spawn_blocking(move || {
        std::process::Command::new("ffmpeg")
            .args(get_initial_args(hls_url_clone.as_str()))
            .output()
            .expect("Failed to run ffmpeg")
    })
    .await
    .unwrap();

    let input_stream = async_stream::stream! {
        let pcm = std::fs::read("output.raw").unwrap();
        for chunk in pcm.chunks(CHUNK_SIZE) {
            tokio::time::sleep(Duration::from_millis(100)).await;
            yield Ok(aws_sdk_transcribestreaming::types::AudioStream::AudioEvent(
                aws_sdk_transcribestreaming::types::AudioEvent::builder()
                    .audio_chunk(aws_sdk_transcribestreaming::primitives::Blob::new(chunk))
                    .build(),
            ));
        }
    };

    let mut output = client
        .start_stream_transcription()
        .language_code(LanguageCode::EnGb)
        .media_sample_rate_hertz(16000)
        .media_encoding(MediaEncoding::Pcm)
        .audio_stream(input_stream.into())
        .send()
        .await?;

    generate_srt(&mut output).await?;
    if std::fs::metadata("transcribed_message.srt").unwrap().len() == 0 {
        panic!("Transcription failed");
    }

    obtain_wav_from_raw().await.unwrap();
    if std::fs::metadata("output.wav").unwrap().len() == 0 {
        panic!("Failed to obtain wav from raw");
    }

    // now that transcription is complete, let's add the subtitles to the video
    add_subtitles_to_video("output.mp4", "transcribed_message.srt")
        .await
        .unwrap();
    if std::fs::metadata("output_with_subtitles.mp4")
        .unwrap()
        .len()
        == 0
    {
        panic!("Failed to add subtitles to video");
    }

    add_audio_to_video("output_with_subtitles.mp4", "output.wav")
        .await
        .unwrap();
    if std::fs::metadata("output_with_audio.mp4").unwrap().len() == 0 {
        panic!("Failed to add audio to video");
    }

    let file_prefix = chrono::Local::now().format("%Y-%m-%d_%H-%M-%S").to_string();

    // upload the video , srt transcription, and .wav to s3
    s3_uploader
        .upload_file(
            "output_with_audio.mp4",
            &format!("{}/{}", file_prefix, "output_with_audio.mp4"),
        )
        .await
        .unwrap();
    s3_uploader
        .upload_file("output.wav", &format!("{}/{}", file_prefix, "output.wav"))
        .await
        .unwrap();
    s3_uploader
        .upload_file(
            "transcribed_message.srt",
            &format!("{}/{}", file_prefix, "transcribed_message.srt"),
        )
        .await
        .unwrap();

    println!("Transcription complete. Transcribed message saved to output_with_subtitles.txt");
    Ok(())
}

async fn obtain_wav_from_raw() -> Result<()> {
    println!("obtaining wav");
    run_ffmpeg(&[
        "-f", "s16le",    // input format is s16le
        "-ar", "16000",   // sample rate 16000 Hz
        "-ac", "1",       // 1 channel (mono)
        "-t", "5",        // duration of 5 seconds
        "-i", "output.raw",  // input file
        "output.wav",     // output file
    ])
    .await
}

async fn add_subtitles_to_video(video_path: &str, subtitle_path: &str) -> Result<()> {
    println!("Adding subtitles to video");

    // verify that the transcription text is not empty (i.e. is streamer AFK?)
    ensure!(
        std::fs::metadata(subtitle_path)?.len() > 0,
        "Transcription text is empty"
    );

    run_ffmpeg(&[
        "-y", // Overwrite output if exists
        "-i",
        video_path, // Input video
        "-vf",
        &format!("subtitles='{}'", subtitle_path), // Apply subtitles filter
        "-t",
        "5",                         // Limit duration to 10 seconds (remove if not needed)
        "output_with_subtitles.mp4", // Output file
    ])
    .await
}

async fn add_audio_to_video(video_path: &str, audio_path: &str) -> Result<()> {
    println!("Adding audio to video");
    run_ffmpeg(&[
        "-y", // Overwrite output if exists
        "-i",
        video_path, // Input video
        "-i",
        audio_path, // Input audio
        "-c",
        "copy", // Copy video codec
        "-map",
        "0:v:0", // Map video from first input
        "-map",
        "1:a:0",                 // Map audio from second input
        "output_with_audio.mp4", // Output file
    ])
    .await
}

async fn generate_srt(output: &mut StartStreamTranscriptionOutput) -> Result<(), Error> {
    // Create a file writer to write the transcribed message to an SRT file.
    let mut file_writer = std::fs::File::create("transcribed_message.srt").unwrap();
    println!("Created filewriter to write transcribed message to file");

    // This will be our SRT entry counter.
    let mut srt_index = 1;

    // Process transcription events as they arrive.
    while let Some(event) = output.transcript_result_stream.recv().await? {
        match event {
            TranscriptResultStream::TranscriptEvent(transcript_event) => {
                // Unwrap the transcript container.
                let transcript = transcript_event.transcript.unwrap();
                // Iterate over each transcript result.
                for result in transcript.results.unwrap_or_default() {
                    debug!("Transcript result: {:?}", result);
                    if !result.is_partial {
                        // Get the first alternative.
                        let first_alternative = &result.alternatives.as_ref().unwrap()[0];
                        let text = first_alternative.transcript.as_ref().unwrap();

                        // Retrieve timing information.
                        // If the API provides these fields (as Option<f64>), use them.
                        // Otherwise, you can set defaults or skip timing.
                        let start_time = result.start_time;
                        let end_time = result.end_time;

                        let start_timestamp = format_timestamp(start_time);
                        let end_timestamp = format_timestamp(end_time);

                        // Construct an SRT entry.
                        let srt_entry = format!(
                            "{}\n{} --> {}\n{}\n\n",
                            srt_index, start_timestamp, end_timestamp, text
                        );
                        srt_index += 1;

                        // Write the SRT entry to the file.
                        file_writer.write_all(srt_entry.as_bytes()).unwrap();
                    }
                }
            }
            otherwise => panic!("received unexpected event type: {:?}", otherwise), // Consider better error handling
        }
    }
    Ok(())
}
