//! Ogg Vorbis (the music in `Sound/*.ogg`), decoded whole into a [`Sound`].

use std::io::Cursor;

use lewton::inside_ogg::OggStreamReader;

use crate::Sound;

/// Decodes an Ogg Vorbis file.
pub fn decode(bytes: &[u8]) -> Result<Sound, String> {
    let mut reader = OggStreamReader::new(Cursor::new(bytes)).map_err(|e| e.to_string())?;
    let channels = u16::from(reader.ident_hdr.audio_channels);
    let rate = reader.ident_hdr.audio_sample_rate;
    let mut samples = Vec::new();
    while let Some(packet) = reader.read_dec_packet_itl().map_err(|e| e.to_string())? {
        samples.extend_from_slice(&packet);
    }
    Ok(Sound::new(samples, rate, channels))
}

#[cfg(test)]
mod tests {
    #[test]
    fn rejects_what_is_not_ogg() {
        assert!(super::decode(b"RIFF....WAVEfmt ").is_err());
    }
}
