//! LZO1X decompression, written from the public bitstream description in the
//! Linux kernel's `Documentation/staging/lzo.rst`.

use crate::{Error, Result};

/// Decompresses one LZO1X stream that must expand to exactly `out_len` bytes and
/// end with the end-of-stream marker at the end of `src`.
pub fn decompress(src: &[u8], out_len: usize) -> Result<Vec<u8>> {
    let mut d = Decoder {
        src,
        pos: 0,
        out: Vec::with_capacity(out_len),
        out_len,
    };
    d.run()?;
    if d.pos != src.len() {
        return Err(d.err(format!(
            "{} trailing bytes after end of stream",
            src.len() - d.pos
        )));
    }
    if d.out.len() != out_len {
        return Err(d.err(format!(
            "decompressed to {} bytes, expected {out_len}",
            d.out.len()
        )));
    }
    Ok(d.out)
}

struct Decoder<'a> {
    src: &'a [u8],
    pos: usize,
    out: Vec<u8>,
    out_len: usize,
}

impl Decoder<'_> {
    fn err(&self, reason: impl Into<String>) -> Error {
        Error::Invalid {
            offset: self.pos,
            reason: format!("LZO: {}", reason.into()),
        }
    }

    fn byte(&mut self) -> Result<usize> {
        let b = *self
            .src
            .get(self.pos)
            .ok_or_else(|| self.err("unexpected end of input"))?;
        self.pos += 1;
        Ok(b.into())
    }

    fn le16(&mut self) -> Result<usize> {
        Ok(self.byte()? | (self.byte()? << 8))
    }

    /// Length with zero-byte extension: each zero adds 255, then the final non-zero byte is added.
    fn extended(&mut self, base: usize) -> Result<usize> {
        let mut len = base;
        loop {
            match self.byte()? {
                0 => len += 255,
                b => return Ok(len + b),
            }
        }
    }

    fn literals(&mut self, n: usize) -> Result<()> {
        let end = self.pos + n;
        let bytes = self
            .src
            .get(self.pos..end)
            .ok_or_else(|| self.err("literal run past end of input"))?;
        if self.out.len() + n > self.out_len {
            return Err(self.err("output overrun"));
        }
        self.out.extend_from_slice(bytes);
        self.pos = end;
        Ok(())
    }

    fn copy_match(&mut self, distance: usize, len: usize) -> Result<()> {
        if distance == 0 || distance > self.out.len() {
            return Err(self.err(format!(
                "match distance {distance} beyond {} bytes of output",
                self.out.len()
            )));
        }
        if self.out.len() + len > self.out_len {
            return Err(self.err("output overrun"));
        }
        let start = self.out.len() - distance;
        // Byte by byte: matches may overlap their own output.
        for i in 0..len {
            let b = self.out[start + i];
            self.out.push(b);
        }
        Ok(())
    }

    fn run(&mut self) -> Result<()> {
        // `state` is how many literals the previous instruction copied (4 means "4 or more").
        let mut state = 0;
        let first = *self.src.first().ok_or_else(|| self.err("empty input"))?;
        if first == 17 {
            return Err(self.err("versioned (kernel) bitstream is not supported"));
        }
        if first > 17 {
            self.pos = 1;
            let n = usize::from(first) - 17;
            self.literals(n)?;
            state = n.min(4);
        }
        loop {
            let inst = self.byte()?;
            let (distance, len, next_state) = match inst {
                0..=15 if state == 0 => {
                    let n = if inst == 0 {
                        self.extended(18)?
                    } else {
                        inst + 3
                    };
                    self.literals(n)?;
                    state = 4;
                    continue;
                }
                0..=15 if state < 4 => {
                    let h = self.byte()?;
                    ((h << 2) + (inst >> 2) + 1, 2, inst & 3)
                }
                0..=15 => {
                    let h = self.byte()?;
                    ((h << 2) + (inst >> 2) + 2049, 3, inst & 3)
                }
                16..=31 => {
                    let len = if inst & 7 == 0 {
                        self.extended(9)?
                    } else {
                        (inst & 7) + 2
                    };
                    let ds = self.le16()?;
                    let distance = 16384 + ((inst & 8) << 11) + (ds >> 2);
                    if distance == 16384 {
                        return Ok(());
                    }
                    (distance, len, ds & 3)
                }
                32..=63 => {
                    let len = if inst & 31 == 0 {
                        self.extended(33)?
                    } else {
                        (inst & 31) + 2
                    };
                    let ds = self.le16()?;
                    ((ds >> 2) + 1, len, ds & 3)
                }
                64..=127 => {
                    let h = self.byte()?;
                    (
                        (h << 3) + ((inst >> 2) & 7) + 1,
                        3 + ((inst >> 5) & 1),
                        inst & 3,
                    )
                }
                _ => {
                    let h = self.byte()?;
                    (
                        (h << 3) + ((inst >> 2) & 7) + 1,
                        5 + ((inst >> 5) & 3),
                        inst & 3,
                    )
                }
            };
            self.copy_match(distance, len)?;
            self.literals(next_state)?;
            state = next_state;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Hand-assembled streams, following the instruction encodings in lzo.rst.
    const EOS: [u8; 3] = [0x11, 0x00, 0x00];

    #[test]
    fn literal_only_first_byte() {
        // 22 => copy 5 literals.
        let mut src = vec![22, b'h', b'e', b'l', b'l', b'o'];
        src.extend(EOS);
        assert_eq!(decompress(&src, 5).unwrap(), b"hello");
    }

    #[test]
    fn short_match_with_trailing_literals() {
        // "abcd" then 0b01_0_000_01: match of length 3 at distance 1 ("ddd"),
        // with 1 trailing literal "e".
        let mut src = vec![21, b'a', b'b', b'c', b'd', 0b0100_0001, 0, b'e'];
        src.extend(EOS);
        assert_eq!(decompress(&src, 8).unwrap(), b"abcdddde");
    }

    #[test]
    fn long_match_with_extended_length() {
        // "ab" then M3 (32 | 0) with extended length 33 + 2 = 35, distance 2, no trailing literals.
        let mut src = vec![19, b'a', b'b', 32, 2, 0x04, 0x00];
        src.extend(EOS);
        let out = decompress(&src, 37).unwrap();
        assert_eq!(out[..], b"ab".repeat(19)[..37]);
    }

    #[test]
    fn rejects_bad_distance_and_wrong_length() {
        let mut src = vec![19, b'a', b'b', 0b0101_1100, 0];
        src.extend(EOS);
        assert!(
            decompress(&src, 10).is_err(),
            "distance 8 with 2 bytes of output"
        );
        let mut ok = vec![22, b'h', b'e', b'l', b'l', b'o'];
        ok.extend(EOS);
        assert!(decompress(&ok, 6).is_err(), "length mismatch");
    }
}
