use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use byteorder::{BigEndian, ReadBytesExt};
use anyhow::{bail, Result};

#[derive(Debug, Clone)]
pub struct AiffHeader {
    pub sample_rate: u32,
    pub channels: u16,
    pub bits_per_sample: u16,
    pub num_sample_frames: u32,
    pub data_offset: u64,
    pub data_size: u64,
}

pub fn parse_extended80(buf: &[u8]) -> f64 {
    let sign = (buf[0] >> 7) & 1;
    let exp = (((buf[0] & 0x7F) as i32) << 8) | (buf[1] as i32);
    let mut mantissa = 0u64;
    for i in 0..8 {
        mantissa = (mantissa << 8) | (buf[2 + i] as u64);
    }
    if exp == 0 && mantissa == 0 {
        return 0.0;
    }
    let f_mantissa = mantissa as f64 / 2.0f64.powi(63);
    let val = f_mantissa * 2.0f64.powi(exp - 16383);
    if sign != 0 { -val } else { val }
}

pub fn parse_aiff(file: &mut (impl Read + Seek)) -> Result<AiffHeader> {
    file.seek(SeekFrom::Start(0))?;
    let mut header_buf = [0u8; 12];
    file.read_exact(&mut header_buf)?;

    if &header_buf[0..4] != b"FORM" {
        bail!("Not a valid AIFF/FORM file");
    }
    let form_type = &header_buf[8..12];
    if form_type != b"AIFF" && form_type != b"AIFC" {
        bail!("Unsupported AIFF form type");
    }

    let mut comm: Option<(u16, u32, u16, u32)> = None;
    let mut data_offset = 0u64;
    let mut data_size = 0u64;

    loop {
        let mut chunk_id = [0u8; 4];
        if file.read_exact(&mut chunk_id).is_err() {
            break;
        }
        let chunk_size = file.read_u32::<BigEndian>()? as u64;

        if &chunk_id == b"COMM" {
            let channels = file.read_u16::<BigEndian>()?;
            let num_sample_frames = file.read_u32::<BigEndian>()?;
            let bits_per_sample = file.read_u16::<BigEndian>()?;
            let mut ext_buf = [0u8; 10];
            file.read_exact(&mut ext_buf)?;
            let sample_rate = parse_extended80(&ext_buf).round() as u32;

            comm = Some((channels, num_sample_frames, bits_per_sample, sample_rate));

            if chunk_size > 18 {
                file.seek(SeekFrom::Current((chunk_size - 18) as i64))?;
            }
        } else if &chunk_id == b"SSND" {
            let offset_val = file.read_u32::<BigEndian>()? as u64;
            let _block_size = file.read_u32::<BigEndian>()?;
            data_offset = file.stream_position()? + offset_val;
            data_size = chunk_size.saturating_sub(8);
            break;
        } else {
            file.seek(SeekFrom::Current(chunk_size as i64))?;
        }
    }

    let (channels, num_sample_frames, bits_per_sample, sample_rate) = match comm {
        Some(c) => c,
        None => bail!("Missing 'COMM' chunk in AIFF"),
    };

    Ok(AiffHeader {
        sample_rate,
        channels,
        bits_per_sample,
        num_sample_frames,
        data_offset,
        data_size,
    })
}

pub fn read_aiff_pcm(path: &str, start_sec: f64, max_duration_sec: f64) -> Result<(u32, Vec<f64>, Vec<f64>)> {
    let mut file = File::open(path)?;
    let header = parse_aiff(&mut file)?;

    let bytes_per_sample = (header.bits_per_sample / 8) as usize;
    let frame_size = header.channels as usize * bytes_per_sample;
    let sample_rate = header.sample_rate;

    let start_frame = (start_sec * sample_rate as f64).round() as u64;
    let max_frames = (max_duration_sec * sample_rate as f64).round() as u64;

    let total_frames = header.num_sample_frames as u64;
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
        let l_sample = parse_be_sample(&buf[base..base + bytes_per_sample], header.bits_per_sample);
        let r_sample = if header.channels >= 2 {
            parse_be_sample(&buf[base + bytes_per_sample..base + 2 * bytes_per_sample], header.bits_per_sample)
        } else {
            l_sample
        };
        left.push(l_sample);
        right.push(r_sample);
    }

    Ok((sample_rate, left, right))
}

#[inline]
fn parse_be_sample(buf: &[u8], bits: u16) -> f64 {
    match bits {
        16 => {
            let v = i16::from_be_bytes([buf[0], buf[1]]) as f64;
            v / 32768.0
        }
        24 => {
            let raw = ((buf[0] as i8 as i32) << 16) | ((buf[1] as i32) << 8) | (buf[2] as i32);
            raw as f64 / 8388608.0
        }
        32 => {
            let mut b = [0u8; 4];
            b.copy_from_slice(buf);
            let v = i32::from_be_bytes(b) as f64;
            v / 2147483648.0
        }
        _ => 0.0,
    }
}
