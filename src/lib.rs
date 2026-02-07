pub mod codec;
pub mod error;

pub use codec::{CompressionOptions, DecompressionOptions, compress, decompress};
pub use error::{Result, ZparsError};
