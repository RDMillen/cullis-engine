use std::fmt;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug)]
pub enum Error {
    Io(std::io::Error),
    BadMagic { expected: [u8; 4], found: [u8; 4] },
    UnexpectedEof { offset: usize, wanted: usize },
    Invalid { offset: usize, reason: String },
    Text { line: usize, reason: String },
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Io(e) => write!(f, "I/O error: {e}"),
            Error::BadMagic { expected, found } => write!(
                f,
                "bad magic: expected {:?}, found {:?}",
                String::from_utf8_lossy(expected),
                String::from_utf8_lossy(found)
            ),
            Error::UnexpectedEof { offset, wanted } => {
                write!(
                    f,
                    "unexpected end of data at {offset:#x} (wanted {wanted} bytes)"
                )
            }
            Error::Invalid { offset, reason } => write!(f, "invalid data at {offset:#x}: {reason}"),
            Error::Text { line, reason } => write!(f, "line {line}: {reason}"),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Error::Io(e) => Some(e),
            _ => None,
        }
    }
}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::Io(e)
    }
}
