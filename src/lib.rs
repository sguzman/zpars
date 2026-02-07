pub mod codec;
pub mod error;
pub mod zpaq;

pub use codec::{CompressionOptions, DecompressionOptions, compress, decompress};
pub use error::{Result, ZparsError};
pub use zpaq::{
    ZpaqBlockHeader, ZpaqExtractedSegment,
    archive_is_fully_unmodeled_file as zpaq_is_fully_unmodeled_file,
    extract_unmodeled_bytes as extract_zpaq_unmodeled_bytes,
    extract_unmodeled_file as extract_zpaq_unmodeled_file, inspect_bytes as inspect_zpaq_bytes,
    inspect_file as inspect_zpaq_file,
};
