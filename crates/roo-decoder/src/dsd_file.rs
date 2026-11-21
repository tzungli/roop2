use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use byteorder::{LittleEndian, BigEndian, ReadBytesExt, ByteOrder};
use anyhow::{bail, Result};

#[derive(Debug, Clone)]
pub struct DsfInfo {
    pub dsd_rate: u32,
    pub channels: u16,
    pub sample_count: u64,
    pub block_size: usize,
    pub data_offset: u64,
    pub data_size: u64,
    pub duration_sec: f64,
}

pub fn parse_dsf(file: &mut (impl Read + Seek)) -> Result<DsfInfo> {
    file.seek(SeekFrom::Start(0))?;
    let mut dsd_chunk = [0u8; 28];
    file.read_exact(&mut dsd_chunk)?;
    if &dsd_chunk[0..4] != b"DSD " {
        bail!("Not a valid DSF file");
    }

    let mut fmt_chunk = [0u8; 52];
    file.read_exact(&mut fmt_chunk)?;
    if &fmt_chunk[0..4] != b"fmt " {
        bail!("Missing 'fmt ' chunk in DSF");
    }

    let channels = LittleEndian::read_u32(&fmt_chunk[24..28]) as u16;
    let dsd_rate = LittleEndian::read_u32(&fmt_chunk[28..32]);
    let sample_count = LittleEndian::read_u64(&fmt_chunk[36..44]);
    let block_size = LittleEndian::read_u32(&fmt_chunk[44..48]) as usize;

    let mut data_header = [0u8; 12];
    file.read_exact(&mut data_header)?;
    if &data_header[0..4] != b"data" {
        bail!("Missing 'data' chunk in DSF");
    }
    let data_size = LittleEndian::read_u64(&data_header[4..12]).saturating_sub(12);
    let data_offset = file.stream_position()?;

    let duration_sec = if dsd_rate > 0 { sample_count as f64 / dsd_rate as f64 } else { 0.0 };

    Ok(DsfInfo {
        dsd_rate,
        channels,
        sample_count,
        block_size,
        data_offset,
        data_size,
        duration_sec,
    })
}

#[derive(Debug, Clone)]
pub struct DffInfo {
    pub dsd_rate: u32,
    pub channels: u16,
    pub data_offset: u64,
    pub data_size: u64,
}

pub fn parse_dff(file: &mut (impl Read + Seek)) -> Result<DffInfo> {
    file.seek(SeekFrom::Start(0))?;
    let mut header = [0u8; 12];
    file.read_exact(&mut header)?;
    if &header[0..4] != b"FRM8" || &header[8..12] != b"DSD " {
        bail!("Not a valid DFF (DSDIFF) file");
    }

    let mut dsd_rate = 2822400u32;
    let mut channels = 2u16;
    let mut data_offset = 0u64;
    let mut data_size = 0u64;

    loop {
        let mut chunk_id = [0u8; 4];
        if file.read_exact(&mut chunk_id).is_err() {
            break;
        }
        let chunk_size = file.read_u64::<BigEndian>()?;
        let cur_pos = file.stream_position()?;

        if &chunk_id == b"PROP" {
            let mut prop_type = [0u8; 4];
            file.read_exact(&mut prop_type)?;
            if &prop_type == b"SND " {
                let prop_end = cur_pos + chunk_size;
                while file.stream_position()? + 12 <= prop_end {
                    let mut sub_id = [0u8; 4];
                    file.read_exact(&mut sub_id)?;
                    let sub_size = file.read_u64::<BigEndian>()?;
                    let sub_pos = file.stream_position()?;
                    if &sub_id == b"FS  " {
                        dsd_rate = file.read_u32::<BigEndian>()?;
                    } else if &sub_id == b"CHNL" {
                        channels = file.read_u16::<BigEndian>()?;
                    }
                    file.seek(SeekFrom::Start(sub_pos + sub_size))?;
                }
            }
        } else if &chunk_id == b"DSD " {
            data_offset = cur_pos;
            data_size = chunk_size;
            break;
        }

        file.seek(SeekFrom::Start(cur_pos + chunk_size))?;
    }

    if data_offset == 0 {
        bail!("Missing DSD data chunk in DFF");
    }

    Ok(DffInfo {
        dsd_rate,
        channels,
        data_offset,
        data_size,
    })
}

/// Decimate raw DSF bitstream to 24-bit 88.2kHz PCM arrays (Left, Right)
pub fn decimate_dsf_to_pcm(path: &str, start_sec: f64, max_duration_sec: f64) -> Result<(u32, Vec<f64>, Vec<f64>)> {
    let mut file = File::open(path)?;
    let info = parse_dsf(&mut file)?;

    let block_size = info.block_size;
    let block_pair_size = block_size * 2;
    let samples_per_block = block_size * 8;
    let decimate_ratio = 32usize;
    let out_samples_per_block = samples_per_block / decimate_ratio; // 1024 samples

    let total_blocks = info.sample_count as usize / samples_per_block;
    let start_block = (total_blocks.saturating_sub(1)).min(((start_sec * info.dsd_rate as f64) / samples_per_block as f64).floor() as usize);
    let blocks_to_process = (total_blocks - start_block).min(((max_duration_sec * info.dsd_rate as f64) / samples_per_block as f64).ceil() as usize);

    let total_out_frames = blocks_to_process * out_samples_per_block;
    let mut left = Vec::with_capacity(total_out_frames);
    let mut right = Vec::with_capacity(total_out_frames);

    let mut block_pair = vec![0u8; block_pair_size];

    for b in 0..blocks_to_process {
        let file_pos = info.data_offset + ((start_block + b) * block_pair_size) as u64;
        file.seek(SeekFrom::Start(file_pos))?;
        file.read_exact(&mut block_pair)?;

        for s in 0..out_samples_per_block {
            let mut sum_l = 0i32;
            let mut sum_r = 0i32;
            let start_bit = s * decimate_ratio;

            for bit in 0..decimate_ratio {
                let bit_pos = start_bit + bit;
                let byte_idx = bit_pos / 8;
                let bit_shift = 7 - (bit_pos % 8);

                let bit_l = (block_pair[byte_idx] >> bit_shift) & 1;
                sum_l += if bit_l != 0 { 1 } else { -1 };

                let bit_r = (block_pair[block_size + byte_idx] >> bit_shift) & 1;
                sum_r += if bit_r != 0 { 1 } else { -1 };
            }

            let val_l = ((sum_l as f64 / 32.0) * 1.4).clamp(-1.0, 1.0);
            let val_r = ((sum_r as f64 / 32.0) * 1.4).clamp(-1.0, 1.0);

            left.push(val_l);
            right.push(val_r);
        }
    }

    let out_sample_rate = info.dsd_rate / decimate_ratio as u32; // e.g. 2822400 / 32 = 88200
    Ok((out_sample_rate, left, right))
}
