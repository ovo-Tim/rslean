use thiserror::Error;

#[derive(Error, Debug)]
pub enum OleanError {
    #[error("invalid .olean header: {0}")]
    InvalidHeader(String),

    #[error("unsupported .olean version: {0} (expected 2)")]
    UnsupportedVersion(u8),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("deserialization error: {0}")]
    Deserialize(String),

    #[error("invalid object tag {tag} at position {pos}")]
    InvalidTag { tag: u8, pos: usize },

    #[error("out of bounds read at position {pos}, region size {size}")]
    OutOfBounds { pos: usize, size: usize },

    #[error("MPZ values not yet supported")]
    MpzNotSupported,
}

pub type OleanResult<T> = Result<T, OleanError>;
