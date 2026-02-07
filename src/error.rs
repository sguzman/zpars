use thiserror::Error;

pub type Result<T> = std::result::Result<T, ZparsError>;

#[derive(Debug, Error)]
pub enum ZparsError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("invalid format: {0}")]
    InvalidFormat(&'static str),

    #[error("corrupt stream: {0}")]
    Corrupt(&'static str),

    #[error("unsupported version: {0}")]
    UnsupportedVersion(u8),

    #[error("invalid option: {0}")]
    InvalidOption(&'static str),
}
