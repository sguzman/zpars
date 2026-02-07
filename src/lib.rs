pub mod codec;
pub mod error;
pub mod zpaq;

pub use codec::{CompressionOptions, DecompressionOptions, compress, decompress};
pub use error::{Result, ZparsError};
pub use zpaq::{
    ZpaqBlockHeader, inspect_bytes as inspect_zpaq_bytes, inspect_file as inspect_zpaq_file,
};
