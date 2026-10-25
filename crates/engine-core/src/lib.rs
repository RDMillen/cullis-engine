//! The engine's foundations, below the simulation: the fixed-step clock
//! ([`time`]), the asset cache over the game's archives ([`assets`]), and the
//! engine's logging facade (the [`log`] crate, re-exported).
//!
//! No rendering, windowing or platform dependency, so ports can reuse it.

pub mod assets;
pub mod time;

pub use log;

/// What can go wrong in the engine core.
#[derive(Debug)]
pub enum Error {
    /// A file could not be read.
    Io(std::io::Error),
    /// A file did not parse.
    Data(fable_data::Error),
    /// An archive has no bank by that name.
    NoBank {
        archive: &'static str,
        bank: &'static str,
    },
    /// A bank has no entry with that id or name.
    NoEntry { bank: &'static str, entry: String },
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Io(e) => write!(f, "{e}"),
            Error::Data(e) => write!(f, "{e}"),
            Error::NoBank { archive, bank } => write!(f, "{archive} has no bank {bank}"),
            Error::NoEntry { bank, entry } => write!(f, "{bank} has no entry {entry}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::Io(e)
    }
}

impl From<fable_data::Error> for Error {
    fn from(e: fable_data::Error) -> Self {
        Error::Data(e)
    }
}

pub type Result<T> = std::result::Result<T, Error>;
