use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use byteorder::{LittleEndian, ReadBytesExt};
use anyhow::{bail, Result};

#[derive(Debug, Clone)]
pub struct WavHeader {
    pub sample_rate: u32,
    pub channels: u16,
    pub bits_per_sample: u16,
    pub block_align: u16,
    pub is_float: bool,
    pub data_offset: u64,
    pub data_size: u64,
}

pub fn parse_wav(file: &mut (impl Read + Seek)) -> Result<WavHeader> {
    file.seek(SeekFrom::Start(0))?;
    let mut riff_buf = [0u8; 12];
    file.read_exact(&mut riff_buf)?;
    if &riff_buf[0..4] != b"RIFF" || &riff_buf[8..12] != b"WAVE" {
        bail!("Not a valid WAV/RIFF file");
    }

    let mut fmt: Option<(u16, u16, u32, u16, u16)> = None;
    let mut data_offset = 0u64;
    let mut data_size = 0u64;

    loop {
        let mut chunk_id = [0u8; 4];
        if file.read_exact(&mut chunk_id).is_err() {
            break;
        }
        let chunk_size = file.read_u32::<LittleEndian>()? as u64;

        if &chunk_id == b"fmt " {
            let format_tag = file.read_u16::<LittleEndian>()?;
            let channels = file.read_u16::<LittleEndian>()?;
            let sample_rate = file.read_u32::<LittleEndian>()?;
            let _byte_rate = file.read_u32::<LittleEndian>()?;
            let block_align = file.read_u16::<LittleEndian>()?;
            let bits_per_sample = file.read_u16::<LittleEndian>()?;
            fmt = Some((format_tag, channels, sample_rate, block_align, bits_per_sample));

            if chunk_size > 16 {
                file.seek(SeekFrom::Current((chunk_size - 16) as i64))?;
            }
        } else if &chunk_id == b"data" {
            data_offset = file.stream_position()?;
            data_size = chunk_size;
            break;
        } else {
            file.seek(SeekFrom::Current(chunk_size as i64))?;
        }
    }

    let (format_tag, channels, sample_rate, block_align, bits_per_sample) = match fmt {
        Some(f) => f,
        None => bail!("Missing 'fmt ' chunk in WAV file"),
    };

    Ok(WavHeader {
        sample_rate,
        channels,
        bits_per_sample,
        block_align,
        is_float: format_tag == 3,
        data_offset,
        data_size,
    })
}

pub fn read_wav_pcm(path: &str, start_sec: f64, max_duration_sec: f64) -> Result<(u32, Vec<f64>, Vec<f64>)> {
    let mut file = File::open(path)?;
    let header = parse_wav(&mut file)?;

    let bytes_per_sample = (header.bits_per_sample / 8) as usize;
    let frame_size = header.channels as usize * bytes_per_sample;
    let sample_rate = header.sample_rate;

    let start_frame = (start_sec * sample_rate as f64).round() as u64;
    let max_frames = (max_duration_sec * sample_rate as f64).round() as u64;

    let total_frames = header.data_size / frame_size as u64;
    let frames_to_read = max_frames.min(total_frames.saturating_sub(start_frame)) as usize;

    let start_offset = header.data_offset + start_frame * frame_size as u64;
    file.seek(SeekFrom::Start(start_offset))?;

    let mut left = Vec::with_capacity(frames_to_read);
    let mut right = Vec::with_capacity(frames_to_read);

    let mut buf = vec![0u8; frames_to_read * frame_size];
    let bytes_read = file.read(&mut buf)?;
    let actual_frames = bytes_read / frame_size;

    for frame_idx in 0..actual_frames {
        let base = frame_idx * frame_size;
        let l_sample = parse_sample(&buf[base..base + bytes_per_sample], header.bits_per_sample, header.is_float);
        let r_sample = if header.channels >= 2 {
            parse_sample(&buf[base + bytes_per_sample..base + 2 * bytes_per_sample], header.bits_per_sample, header.is_float)
        } else {
            l_sample
        };
        left.push(l_sample);
        right.push(r_sample);
    }

    Ok((sample_rate, left, right))
}

#[inline]
fn parse_sample(buf: &[u8], bits: u16, is_float: bool) -> f64 {
    if is_float {
        if bits == 32 {
            let mut b = [0u8; 4];
            b.copy_from_slice(buf);
            f32::from_le_bytes(b) as f64
        } else if bits == 64 {
            let mut b = [0u8; 8];
            b.copy_from_slice(buf);
            f64::from_le_bytes(b)
        } else {
            0.0
        }
    } else {
        match bits {
            16 => {
                let v = i16::from_le_bytes([buf[0], buf[1]]) as f64;
                v / 32768.0
            }
            24 => {
                // 24-bit sign-extended
                let b = [buf[0], buf[1], buf[2]];
                let raw = (b[0] as i32) | ((b[1] as i32) << 8) | ((b[2] as i8 as i32) << 16);
                raw as f64 / 8388608.0
            }
            32 => {
                let mut b = [0u8; 4];
                b.copy_from_slice(buf);
                let v = i32::from_le_bytes(b) as f64;
                v / 2147483648.0
            }
            _ => 0.0,
        }
    }
}
