use std::fmt;

#[derive(Debug)]
pub enum Error {
    Io(std::io::Error),
    TooShort { need: usize, actual: usize },
    BadMagic,
    BadPacketId(u8),
    StringTooLong(usize),
    InvalidUtf8,
    BadAddress,
    Zlib,
    BatchTooLarge,
    // 世界存档读写失败
    Storage(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(e) => write!(f, "io error: {e}"),
            Self::TooShort { need, actual } => {
                write!(f, "packet too short: need {need} bytes, got {actual}")
            }
            Self::BadMagic => write!(f, "raknet magic mismatch"),
            Self::BadPacketId(id) => write!(f, "unexpected packet id: 0x{id:02x}"),
            Self::StringTooLong(len) => write!(f, "string too long for u16 length: {len} bytes"),
            Self::InvalidUtf8 => write!(f, "invalid utf-8 in string"),
            Self::BadAddress => write!(f, "unsupported raknet address"),
            Self::Zlib => write!(f, "zlib inflate/deflate failed"),
            Self::BatchTooLarge => write!(f, "batch payload too large"),
            Self::Storage(e) => write!(f, "world storage error: {e}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}

pub type Result<T> = std::result::Result<T, Error>;
