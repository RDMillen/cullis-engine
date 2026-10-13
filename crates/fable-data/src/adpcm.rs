//! IMA ADPCM as packed by the Xbox (WAVE format 0x69), used by the voice banks.
//!
//! The data is in blocks of 36 bytes per channel. A block starts with a 4-byte
//! header per channel (i16 starting sample, u8 step index, u8 reserved); the
//! rest holds the channels' 4-bit codes in turns of 4 bytes per channel, low
//! nibble first. Each code moves the sample by `((2 * magnitude + 1) * step) >> 3`
//! (the standard IMA tables below) and moves the step index. A block gives 64
//! samples per channel: the header sample, then the first 63 codes (the 64th
//! code is not used). Checked sample for sample against ffmpeg's
//! `adpcm_ima_xbox` decoder on the voice banks.

use crate::{Error, Result};

/// Bytes per channel in a block, and samples per channel it gives.
const BLOCK: usize = 36;
const SAMPLES: usize = 64;

const INDEX_STEP: [i32; 16] = [-1, -1, -1, -1, 2, 4, 6, 8, -1, -1, -1, -1, 2, 4, 6, 8];

const STEPS: [i32; 89] = [
    7, 8, 9, 10, 11, 12, 13, 14, 16, 17, 19, 21, 23, 25, 28, 31, 34, 37, 41, 45, 50, 55, 60, 66,
    73, 80, 88, 97, 107, 118, 130, 143, 157, 173, 190, 209, 230, 253, 279, 307, 337, 371, 408, 449,
    494, 544, 598, 658, 724, 796, 876, 963, 1060, 1166, 1282, 1411, 1552, 1707, 1878, 2066, 2272,
    2499, 2749, 3024, 3327, 3660, 4026, 4428, 4871, 5358, 5894, 6484, 7132, 7845, 8630, 9493,
    10442, 11487, 12635, 13899, 15289, 16818, 18500, 20350, 22385, 24623, 27086, 29794, 32767,
];

/// One channel's decoder state.
#[derive(Debug, Clone, Copy)]
struct Channel {
    sample: i32,
    index: i32,
}

impl Channel {
    fn code(&mut self, code: u8) -> i16 {
        let step = STEPS[self.index as usize];
        let diff = ((2 * i32::from(code & 7) + 1) * step) >> 3;
        if code & 8 != 0 {
            self.sample -= diff;
        } else {
            self.sample += diff;
        }
        self.sample = self.sample.clamp(-32768, 32767);
        self.index = (self.index + INDEX_STEP[code as usize]).clamp(0, 88);
        self.sample as i16
    }
}

/// Decodes Xbox ADPCM `data` with `channels` channels to interleaved 16-bit
/// samples. A trailing partial block is ignored.
pub fn decode_xbox(data: &[u8], channels: usize) -> Result<Vec<i16>> {
    if channels == 0 {
        return Err(Error::Invalid {
            offset: 0,
            reason: "ADPCM with no channels".into(),
        });
    }
    let block = BLOCK * channels;
    let mut out = Vec::with_capacity(data.len() / block * SAMPLES * channels);
    let mut per = vec![Vec::with_capacity(SAMPLES + 1); channels];
    for b in data.chunks_exact(block) {
        let mut state: Vec<Channel> = (0..channels)
            .map(|c| {
                let h = &b[c * 4..c * 4 + 4];
                Channel {
                    sample: i32::from(i16::from_le_bytes([h[0], h[1]])),
                    index: i32::from(h[2]).min(88),
                }
            })
            .collect();
        for (c, s) in per.iter_mut().zip(&state) {
            c.clear();
            c.push(s.sample as i16);
        }
        // The codes: 8 turns of 4 bytes per channel.
        let codes = &b[4 * channels..];
        for turn in codes.chunks_exact(4 * channels) {
            for (c, bytes) in turn.as_chunks::<4>().0.iter().enumerate() {
                for &byte in bytes {
                    let first = state[c].code(byte & 0x0f);
                    let second = state[c].code(byte >> 4);
                    per[c].push(first);
                    per[c].push(second);
                }
            }
        }
        for i in 0..SAMPLES {
            for c in &per {
                out.push(c[i]);
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_block_starts_at_its_header_sample_and_follows_its_codes() {
        // Mono: header sample 1000, step index 0 (step 7); codes 0x7 then 0xf.
        let mut block = vec![0u8; 36];
        block[..2].copy_from_slice(&1000i16.to_le_bytes());
        block[4] = 0xf7;
        let out = decode_xbox(&block, 1).unwrap();
        assert_eq!(out.len(), 64);
        assert_eq!(out[0], 1000);
        // Code 7: + ((2*7+1)*7)>>3 = +13, index 0 + 8 = 8 (step 16).
        assert_eq!(out[1], 1013);
        // Code 0xf: - ((2*7+1)*16)>>3 = -30.
        assert_eq!(out[2], 983);
    }

    #[test]
    fn stereo_interleaves_channels() {
        let mut block = vec![0u8; 72];
        block[..2].copy_from_slice(&5i16.to_le_bytes());
        block[4..6].copy_from_slice(&(-5i16).to_le_bytes());
        let out = decode_xbox(&block, 2).unwrap();
        assert_eq!(out.len(), 128);
        assert_eq!(&out[..2], &[5, -5]);
    }
}
