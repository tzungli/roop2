use std::fs::File;
use std::path::Path;
use anyhow::{bail, Context, Result};
use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::{DecoderOptions, CODEC_TYPE_NULL};
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;

pub fn read_flac_pcm(path: &str, start_sec: f64, max_duration_sec: f64) -> Result<(u32, Vec<f64>, Vec<f64>)> {
    let file = File::open(path).with_context(|| format!("Failed to open file: {}", path))?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());

    let mut hint = Hint::new();
    if let Some(ext) = Path::new(path).extension().and_then(|s| s.to_str()) {
        hint.with_extension(ext);
    }

    let probed = symphonia::default::get_probe()
        .format(&hint, mss, &FormatOptions::default(), &MetadataOptions::default())
        .with_context(|| "Failed to probe audio format")?;

    let mut format = probed.format;
    let track = format
        .tracks()
        .iter()
        .find(|t| t.codec_params.codec != CODEC_TYPE_NULL)
        .context("No supported audio tracks found")?;

    let sample_rate = track.codec_params.sample_rate.unwrap_or(44100);
    let channels = track.codec_params.channels.map(|c| c.count()).unwrap_or(2);
    let track_id = track.id;

    let mut decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &DecoderOptions::default())
        .context("Failed to create codec decoder")?;

    let start_frame = (start_sec * sample_rate as f64).round() as u64;
    let max_frames = (max_duration_sec * sample_rate as f64).round() as u64;

    let mut left = Vec::new();
    let mut right = Vec::new();
    let mut current_frame = 0u64;

    while let Ok(packet) = format.next_packet() {
        if packet.track_id() != track_id {
            continue;
        }

        match decoder.decode(&packet) {
            Ok(decoded) => {
                let packet_frames = decoded.frames();
                let next_frame = current_frame + packet_frames as u64;

                if next_frame <= start_frame {
                    current_frame = next_frame;
                    continue;
                }

                let mut sample_buf = SampleBuffer::<f64>::new(decoded.capacity() as u64, *decoded.spec());
                sample_buf.copy_interleaved_ref(decoded);
                let samples = sample_buf.samples();

                let frame_start = if current_frame < start_frame {
                    (start_frame - current_frame) as usize
                } else {
                    0
                };

                for f in frame_start..packet_frames {
                    if left.len() as u64 >= max_frames {
                        break;
                    }
                    let base = f * channels;
                    let l = samples[base];
                    let r = if channels >= 2 { samples[base + 1] } else { l };
                    left.push(l);
                    right.push(r);
                }

                current_frame = next_frame;
                if left.len() as u64 >= max_frames {
                    break;
                }
            }
            Err(symphonia::core::errors::Error::DecodeError(_)) => continue,
            Err(e) => bail!("Decoder error: {}", e),
        }
    }

    Ok((sample_rate, left, right))
}
