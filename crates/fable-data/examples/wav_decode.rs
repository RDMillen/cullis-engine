//! Decodes a WAVE file (Xbox ADPCM or 16-bit PCM) to raw 16-bit samples:
//! `cargo run --example wav_decode -- <in.wav> <out.raw>`.

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let (input, output) = (
        args.next().ok_or("usage: wav_decode <in.wav> <out.raw>")?,
        args.next().ok_or("usage: wav_decode <in.wav> <out.raw>")?,
    );
    let wave = fable_data::lut::Wave::parse(&std::fs::read(input)?)?;
    let samples = wave.decode()?;
    let bytes: Vec<u8> = samples.iter().flat_map(|s| s.to_le_bytes()).collect();
    std::fs::write(output, bytes)?;
    Ok(())
}
