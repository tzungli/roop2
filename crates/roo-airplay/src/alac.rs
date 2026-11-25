//! Lossless Uncompressed (Verbatim) ALAC frame encoder for AirPlay / RAOP.
//! Every AirPlay 1 & AirPlay 2 RAOP receiver supports `AppleLossless` (`cn=1`).
//! Encoding stereo 16-bit PCM in ALAC verbatim mode (`isNotCompressed = 1`)
//! produces bit-perfect lossless audio with zero external encoder dependencies.

pub const RAOP_FRAMES_PER_PACKET: usize = 352;
pub const RAOP_SAMPLE_RATE: u32 = 44100;

struct BitWriter {
    buf: Vec<u8>,
    cur: u8,
    bits_used: u8,
}

impl BitWriter {
    fn with_capacity(cap: usize) -> Self {
        Self {
            buf: Vec::with_capacity(cap),
            cur: 0,
            bits_used: 0,
        }
    }

    #[inline]
    fn write_bits(&mut self, value: u32, num_bits: u8) {
        for shift in (0..num_bits).rev() {
            let bit = ((value >> shift) & 1) as u8;
            self.cur = (self.cur << 1) | bit;
            self.bits_used += 1;
            if self.bits_used == 8 {
                self.buf.push(self.cur);
                self.cur = 0;
                self.bits_used = 0;
            }
        }
    }

    fn finish(mut self) -> Vec<u8> {
        if self.bits_used > 0 {
            self.cur <<= 8 - self.bits_used;
            self.buf.push(self.cur);
        }
        self.buf
    }
}

/// Encode up to 352 stereo `i16` PCM frames into a valid lossless ALAC verbatim packet.
pub fn encode_alac_verbatim_stereo(left: &[i16], right: &[i16]) -> Vec<u8> {
    let n = left.len().min(right.len()).min(RAOP_FRAMES_PER_PACKET);
    let has_size = n != RAOP_FRAMES_PER_PACKET;
    let mut bw = BitWriter::with_capacity(n * 4 + 12);

    // ALAC Channel Pair Element (CPE) header:
    // - 3 bits: element tag = 1 (ID_CPE, stereo)
    // - 4 bits: element instance tag = 0
    // - 12 bits: unused = 0
    // - 1 bit: hasSize (1 if frame count != default 352)
    // - 2 bits: unusedBytes = 0
    // - 1 bit: isNotCompressed = 1 (verbatim 16-bit big-endian PCM)
    bw.write_bits(1, 3);
    bw.write_bits(0, 4);
    bw.write_bits(0, 12);
    bw.write_bits(if has_size { 1 } else { 0 }, 1);
    bw.write_bits(0, 2);
    bw.write_bits(1, 1);

    if has_size {
        bw.write_bits(n as u32, 32);
    }

    for i in 0..n {
        bw.write_bits(left[i] as u16 as u32, 16);
        bw.write_bits(right[i] as u16 as u32, 16);
    }

    // 3 bits: ID_END = 7
    bw.write_bits(7, 3);
    bw.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_alac_verbatim_full_packet_size_and_header() {
        let l = vec![0i16; RAOP_FRAMES_PER_PACKET];
        let r = vec![0i16; RAOP_FRAMES_PER_PACKET];
        let pkt = encode_alac_verbatim_stereo(&l, &r);
        // Header (23 bits) + 352*32 bits + END (3 bits) = 11290 bits -> 1412 bytes
        assert_eq!(pkt.len(), 1412);
        // First 3 bits = 001 (CPE), next 4 = 0000 -> byte 0 = 0x20
        assert_eq!(pkt[0], 0x20);
        // Byte 1 = 0x00, Byte 2 = 0x02 (bit 22 = isNotCompressed = 1 -> 0b0000_0010)
        assert_eq!(pkt[1], 0x00);
        assert_eq!(pkt[2] & 0xFE, 0x02);
    }

    #[test]
    fn test_alac_verbatim_partial_packet_has_size() {
        let l = vec![100i16; 100];
        let r = vec![-100i16; 100];
        let pkt = encode_alac_verbatim_stereo(&l, &r);
        // Header (23 + 32 = 55 bits) + 100*32 bits + END (3 bits) = 3258 bits -> 408 bytes
        assert_eq!(pkt.len(), 408);
        // hasSize bit (bit 19) is set -> byte 2 = 0b0001_0010 (0x12)
        assert_eq!(pkt[2] & 0xFE, 0x12);
    }
}
