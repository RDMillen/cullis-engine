use crate::{Error, Result};

/// Little-endian cursor over an in-memory byte slice.
pub(crate) struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }

    pub fn pos(&self) -> usize {
        self.pos
    }

    pub fn bytes(&mut self, n: usize) -> Result<&'a [u8]> {
        let end = self
            .pos
            .checked_add(n)
            .filter(|&end| end <= self.data.len())
            .ok_or(Error::UnexpectedEof {
                offset: self.pos,
                wanted: n,
            })?;
        let out = &self.data[self.pos..end];
        self.pos = end;
        Ok(out)
    }

    pub fn array<const N: usize>(&mut self) -> Result<[u8; N]> {
        Ok(self.bytes(N)?.try_into().unwrap())
    }

    pub fn u32(&mut self) -> Result<u32> {
        Ok(u32::from_le_bytes(self.array()?))
    }

    /// A `u32` length followed by that many Latin-1 bytes.
    pub fn len_string(&mut self) -> Result<String> {
        let len = self.u32()? as usize;
        Ok(latin1(self.bytes(len)?))
    }

    /// Latin-1 bytes up to (and consuming) a NUL terminator.
    pub fn c_string(&mut self) -> Result<String> {
        let rest = &self.data[self.pos..];
        let len = rest
            .iter()
            .position(|&b| b == 0)
            .ok_or(Error::UnexpectedEof {
                offset: self.pos,
                wanted: rest.len() + 1,
            })?;
        let s = latin1(&rest[..len]);
        self.pos += len + 1;
        Ok(s)
    }
}

pub(crate) fn latin1(bytes: &[u8]) -> String {
    bytes.iter().map(|&b| b as char).collect()
}

pub(crate) fn put_u32(out: &mut Vec<u8>, v: u32) {
    out.extend_from_slice(&v.to_le_bytes());
}

/// Writes a `u32` length and Latin-1 bytes. Panics on characters above U+00FF,
/// which `latin1` never produces.
pub(crate) fn put_len_string(out: &mut Vec<u8>, s: &str) {
    put_u32(out, s.chars().count() as u32);
    out.extend(
        s.chars()
            .map(|c| u8::try_from(c).expect("non-Latin-1 character")),
    );
}
