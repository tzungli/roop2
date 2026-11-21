use byteorder::{BigEndian, ByteOrder};

pub fn is_isobmff(data: &[u8]) -> bool {
    data.len() > 12 && &data[4..8] == b"ftyp"
}

struct BoxInfo<'a> {
    size: usize,
    header_size: usize,
    box_type: &'a [u8; 4],
}

fn box_at<'a>(data: &'a [u8], off: usize) -> Option<BoxInfo<'a>> {
    if off + 8 > data.len() {
        return None;
    }
    let raw_size = BigEndian::read_u32(&data[off..off + 4]) as usize;
    let mut header_size = 8;
    let size = if raw_size == 1 {
        if off + 16 > data.len() {
            return None;
        }
        header_size = 16;
        BigEndian::read_u64(&data[off + 8..off + 16]) as usize
    } else if raw_size == 0 {
        data.len() - off
    } else {
        raw_size
    };

    if size < header_size || off + size > data.len() {
        return None;
    }

    let box_type = match data[off + 4..off + 8].try_into() {
        Ok(t) => t,
        Err(_) => return None,
    };

    Some(BoxInfo { size, header_size, box_type })
}

pub fn extract_flac_from_isobmff(data: &[u8]) -> Option<Vec<u8>> {
    let mut stream_info_block: Option<&[u8]> = None;

    fn walk_moov<'a>(data: &'a [u8], mut off: usize, end: usize, stream_info: &mut Option<&'a [u8]>) {
        while off + 8 <= end && stream_info.is_none() {
            let b = match box_at(data, off) {
                Some(b) => b,
                None => break,
            };

            match b.box_type {
                b"trak" | b"mdia" | b"minf" | b"stbl" => {
                    walk_moov(data, off + b.header_size, off + b.size, stream_info);
                }
                b"stsd" => {
                    if off + 16 <= off + b.size {
                        let entry_count = BigEndian::read_u32(&data[off + 12..off + 16]) as usize;
                        let mut eo = off + 16;
                        for _ in 0..entry_count {
                            if eo + 8 > off + b.size { break; }
                            let esize = BigEndian::read_u32(&data[eo..eo + 4]) as usize;
                            if esize < 8 || eo + esize > data.len() { break; }
                            if &data[eo + 4..eo + 8] == b"fLaC" {
                                for p in (eo + 8)..=(eo + esize).saturating_sub(38) {
                                    if data[p] == 0x80 && data[p + 1] == 0x00 && data[p + 2] == 0x00 && data[p + 3] == 0x22 {
                                        *stream_info = Some(&data[p..p + 38]);
                                        break;
                                    }
                                }
                            }
                            eo += esize;
                        }
                    }
                }
                _ => {}
            }
            off += b.size;
        }
    }

    let mut mdat_payloads = Vec::new();
    let mut off = 0;
    while off + 8 <= data.len() {
        let b = match box_at(data, off) {
            Some(b) => b,
            None => break,
        };
        if b.box_type == b"moov" {
            walk_moov(data, off + b.header_size, off + b.size, &mut stream_info_block);
        } else if b.box_type == b"mdat" {
            mdat_payloads.push(&data[off + b.header_size..off + b.size]);
        }
        off += b.size;
    }

    let stream_info = stream_info_block?;
    if mdat_payloads.is_empty() {
        return None;
    }

    let mut out = Vec::with_capacity(4 + stream_info.len() + mdat_payloads.iter().map(|p| p.len()).sum::<usize>());
    out.extend_from_slice(b"fLaC");
    out.extend_from_slice(stream_info);
    for p in mdat_payloads {
        out.extend_from_slice(p);
    }

    Some(out)
}
