use crate::error::{Result, ZparsError};
use std::cmp::min;
use std::io::{Read, Write};
use tracing::{debug, trace};

const MAGIC: &[u8; 4] = b"ZPS1";
const VERSION: u8 = 1;

#[derive(Debug, Clone)]
pub struct CompressionOptions {
    pub block_size: usize,
    pub min_match: usize,
    pub secondary_match: usize,
    pub search_log: u8,
    pub table_log: u8,
}

impl Default for CompressionOptions {
    fn default() -> Self {
        Self {
            block_size: 1 << 20,
            min_match: 4,
            secondary_match: 0,
            search_log: 3,
            table_log: 20,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct DecompressionOptions;

#[derive(Debug, Clone)]
struct BlockHeader {
    uncompressed_len: u32,
    compressed_len: u32,
}

pub fn compress<R: Read, W: Write>(
    mut input: R,
    mut output: W,
    options: &CompressionOptions,
) -> Result<()> {
    validate_options(options)?;
    write_stream_header(&mut output, options)?;

    let mut block_index = 0usize;
    let mut in_block = vec![0u8; options.block_size];
    loop {
        let n = input.read(&mut in_block)?;
        if n == 0 {
            break;
        }
        let raw = &in_block[..n];
        let encoded = encode_lz77_block(raw, options);

        let header = BlockHeader {
            uncompressed_len: n as u32,
            compressed_len: encoded.len() as u32,
        };
        write_block_header(&mut output, &header)?;
        output.write_all(&encoded)?;

        debug!(
            block = block_index,
            in_bytes = n,
            out_bytes = encoded.len(),
            ratio = encoded.len() as f64 / n as f64,
            "compressed block"
        );
        block_index += 1;
    }

    write_block_header(
        &mut output,
        &BlockHeader {
            uncompressed_len: 0,
            compressed_len: 0,
        },
    )?;
    Ok(())
}

pub fn decompress<R: Read, W: Write>(
    mut input: R,
    mut output: W,
    _options: &DecompressionOptions,
) -> Result<()> {
    let options = read_stream_header(&mut input)?;
    let mut block_index = 0usize;
    loop {
        let header = read_block_header(&mut input)?;
        if header.uncompressed_len == 0 && header.compressed_len == 0 {
            break;
        }

        let mut payload = vec![0u8; header.compressed_len as usize];
        input.read_exact(&mut payload)?;

        let decoded = decode_lz77_block(&payload, header.uncompressed_len as usize, &options)?;
        output.write_all(&decoded)?;

        debug!(
            block = block_index,
            in_bytes = payload.len(),
            out_bytes = decoded.len(),
            ratio = payload.len() as f64 / decoded.len() as f64,
            "decompressed block"
        );
        block_index += 1;
    }
    Ok(())
}

fn write_stream_header<W: Write>(mut out: W, options: &CompressionOptions) -> Result<()> {
    out.write_all(MAGIC)?;
    out.write_all(&[VERSION])?;
    out.write_all(&(options.block_size as u32).to_le_bytes())?;
    out.write_all(&[options.min_match as u8])?;
    out.write_all(&[options.secondary_match as u8])?;
    out.write_all(&[options.search_log])?;
    out.write_all(&[options.table_log])?;
    Ok(())
}

fn read_stream_header<R: Read>(mut input: R) -> Result<CompressionOptions> {
    let mut magic = [0u8; 4];
    input.read_exact(&mut magic)?;
    if &magic != MAGIC {
        return Err(ZparsError::InvalidFormat("bad magic"));
    }

    let mut version = [0u8; 1];
    input.read_exact(&mut version)?;
    if version[0] != VERSION {
        return Err(ZparsError::UnsupportedVersion(version[0]));
    }

    let mut block_size = [0u8; 4];
    input.read_exact(&mut block_size)?;
    let block_size = u32::from_le_bytes(block_size) as usize;

    let mut fields = [0u8; 4];
    input.read_exact(&mut fields)?;
    let opts = CompressionOptions {
        block_size,
        min_match: fields[0] as usize,
        secondary_match: fields[1] as usize,
        search_log: fields[2],
        table_log: fields[3],
    };
    validate_options(&opts)?;
    Ok(opts)
}

fn write_block_header<W: Write>(mut out: W, header: &BlockHeader) -> Result<()> {
    out.write_all(&header.uncompressed_len.to_le_bytes())?;
    out.write_all(&header.compressed_len.to_le_bytes())?;
    Ok(())
}

fn read_block_header<R: Read>(mut input: R) -> Result<BlockHeader> {
    let mut bytes = [0u8; 8];
    input.read_exact(&mut bytes)?;
    Ok(BlockHeader {
        uncompressed_len: u32::from_le_bytes(bytes[0..4].try_into().expect("fixed size")),
        compressed_len: u32::from_le_bytes(bytes[4..8].try_into().expect("fixed size")),
    })
}

fn validate_options(options: &CompressionOptions) -> Result<()> {
    if options.min_match == 0 || options.min_match > 64 {
        return Err(ZparsError::InvalidOption("min-match must be 1..=64"));
    }
    if options.secondary_match > 64 {
        return Err(ZparsError::InvalidOption("secondary-match must be 0..=64"));
    }
    if options.block_size == 0 {
        return Err(ZparsError::InvalidOption("block-size must be > 0"));
    }
    if options.search_log > 10 {
        return Err(ZparsError::InvalidOption("search-log must be <= 10"));
    }
    if !(8..=28).contains(&options.table_log) {
        return Err(ZparsError::InvalidOption("table-log must be 8..=28"));
    }
    Ok(())
}

fn encode_lz77_block(input: &[u8], options: &CompressionOptions) -> Vec<u8> {
    let mut out = Vec::with_capacity(input.len() / 2 + 16);
    let mut i = 0usize;
    let mut lit_start = 0usize;

    let table_size = 1usize << options.table_log;
    let search = SearchContext {
        min_match: options.min_match,
        mask: table_size - 1,
        bucket: (1usize << options.search_log).saturating_sub(1),
    };

    let mut h1_table = vec![0u32; table_size];
    let mut h2_table = if options.secondary_match > 0 {
        Some(vec![0u32; table_size])
    } else {
        None
    };

    while i < input.len() {
        let mut best_len = 0usize;
        let mut best_off = 0usize;

        if i + options.min_match <= input.len() {
            if let Some(ref h2) = h2_table
                && i + options.secondary_match <= input.len()
            {
                let hh = hash_slice(&input[i..i + options.secondary_match]) & search.mask;
                search_candidates(input, i, &mut best_len, &mut best_off, h2, hh, &search);
            }

            let h = hash_slice(&input[i..i + options.min_match]) & search.mask;
            search_candidates(
                input,
                i,
                &mut best_len,
                &mut best_off,
                &h1_table,
                h,
                &search,
            );
        }

        let emit_match = if best_off == 0 {
            false
        } else {
            let extra = usize::from(best_off >= (1 << 16)) + usize::from(best_off >= (1 << 24));
            best_len >= options.min_match + extra
        };

        if emit_match {
            emit_literals(&mut out, &input[lit_start..i]);
            emit_match_tokens(&mut out, best_len, best_off, options.min_match);
            trace!(at = i, len = best_len, off = best_off, "emit match");

            for p in i..min(i + best_len, input.len()) {
                update_tables(input, p, options, &mut h1_table, h2_table.as_mut());
            }
            i += best_len;
            lit_start = i;
        } else {
            update_tables(input, i, options, &mut h1_table, h2_table.as_mut());
            i += 1;
        }
    }

    if lit_start < input.len() {
        emit_literals(&mut out, &input[lit_start..]);
    }

    out
}

fn decode_lz77_block(
    input: &[u8],
    expected_len: usize,
    options: &CompressionOptions,
) -> Result<Vec<u8>> {
    let mut out = Vec::with_capacity(expected_len);
    let mut i = 0usize;
    while i < input.len() {
        let code = input[i];
        i += 1;
        let kind = code >> 6;
        let low = (code & 0x3f) as usize;

        if kind == 0 {
            let lit_len = low + 1;
            if i + lit_len > input.len() {
                return Err(ZparsError::Corrupt("literal run exceeds input"));
            }
            out.extend_from_slice(&input[i..i + lit_len]);
            i += lit_len;
            continue;
        }

        let off_bytes = kind as usize + 1;
        if i + off_bytes > input.len() {
            return Err(ZparsError::Corrupt("offset exceeds input"));
        }

        let mut off_m1 = 0usize;
        for _ in 0..off_bytes {
            off_m1 = (off_m1 << 8) | (input[i] as usize);
            i += 1;
        }
        let off = off_m1 + 1;
        let len = low + options.min_match;

        if off == 0 || off > out.len() {
            return Err(ZparsError::Corrupt("invalid match offset"));
        }

        let start = out.len() - off;
        for j in 0..len {
            let b = out[start + j];
            out.push(b);
        }
    }

    if out.len() != expected_len {
        return Err(ZparsError::Corrupt("decoded size mismatch"));
    }

    Ok(out)
}

struct SearchContext {
    min_match: usize,
    bucket: usize,
    mask: usize,
}

fn search_candidates(
    input: &[u8],
    i: usize,
    best_len: &mut usize,
    best_off: &mut usize,
    table: &[u32],
    hash: usize,
    search: &SearchContext,
) {
    for k in 0..=search.bucket {
        let p1 = table[(hash ^ k) & search.mask];
        if p1 == 0 {
            continue;
        }
        let p = (p1 - 1) as usize;
        if p >= i {
            continue;
        }

        let off = i - p;
        if off > ((1usize << 24) - 1) {
            continue;
        }

        let max = min(input.len() - i, 255 + search.min_match);
        let mut len = 0usize;
        while len < max && input[p + len] == input[i + len] {
            len += 1;
        }

        if len > *best_len || (len == *best_len && off < *best_off) {
            *best_len = len;
            *best_off = off;
        }

        if *best_len >= search.min_match + 63 {
            break;
        }
    }
}

fn update_tables(
    input: &[u8],
    pos: usize,
    options: &CompressionOptions,
    h1: &mut [u32],
    h2: Option<&mut Vec<u32>>,
) {
    if pos + options.min_match <= input.len() {
        let idx = hash_slice(&input[pos..pos + options.min_match]) & (h1.len() - 1);
        h1[idx] = (pos + 1) as u32;
    }

    if let Some(table) = h2
        && options.secondary_match > 0
        && pos + options.secondary_match <= input.len()
    {
        let idx = hash_slice(&input[pos..pos + options.secondary_match]) & (table.len() - 1);
        table[idx] = (pos + 1) as u32;
    }
}

fn emit_literals(out: &mut Vec<u8>, literals: &[u8]) {
    let mut i = 0usize;
    while i < literals.len() {
        let chunk = min(64usize, literals.len() - i);
        out.push((chunk as u8) - 1);
        out.extend_from_slice(&literals[i..i + chunk]);
        i += chunk;
    }
}

fn emit_match_tokens(out: &mut Vec<u8>, mut len: usize, off: usize, min_match: usize) {
    let off_m1 = off - 1;
    let off_bytes = if off_m1 < (1 << 16) {
        2
    } else if off_m1 < (1 << 24) {
        3
    } else {
        4
    };

    while len > 0 {
        let len1 = if len > min_match * 2 + 63 {
            min_match + 63
        } else if len > min_match + 63 {
            len - min_match
        } else {
            len
        };

        let code = (((off_bytes - 1) as u8) << 6) | ((len1 - min_match) as u8 & 0x3f);
        out.push(code);

        for shift in (0..off_bytes).rev() {
            out.push(((off_m1 >> (shift * 8)) & 0xff) as u8);
        }

        len -= len1;
    }
}

fn hash_slice(s: &[u8]) -> usize {
    let mut x = 0x9e37_79b9u32;
    for &b in s {
        x ^= b as u32;
        x = x.wrapping_mul(0x85eb_ca6b);
        x ^= x >> 13;
    }
    x as usize
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::{Rng, SeedableRng};

    fn roundtrip(data: &[u8], options: CompressionOptions) {
        let mut compressed = Vec::new();
        compress(data, &mut compressed, &options).expect("compress");

        let mut restored = Vec::new();
        decompress(compressed.as_slice(), &mut restored, &DecompressionOptions)
            .expect("decompress");

        assert_eq!(data, restored);
    }

    #[test]
    fn roundtrip_small_text() {
        let data = b"zpaq zpaq zpaq zpaq rust rust rust";
        roundtrip(data, CompressionOptions::default());
    }

    #[test]
    fn roundtrip_repetitive_large() {
        let mut data = vec![0u8; 200_000];
        for (i, b) in data.iter_mut().enumerate() {
            *b = b"abcd"[i % 4];
        }
        roundtrip(&data, CompressionOptions::default());
    }

    #[test]
    fn roundtrip_random() {
        let mut rng = rand::rngs::StdRng::seed_from_u64(42);
        let mut data = vec![0u8; 128_000];
        rng.fill(&mut data[..]);

        let opts = CompressionOptions {
            block_size: 32 * 1024,
            min_match: 4,
            secondary_match: 6,
            search_log: 4,
            table_log: 16,
        };
        roundtrip(&data, opts);
    }

    #[test]
    fn rejects_invalid_magic() {
        let input = b"bad!";
        let mut sink = Vec::new();
        let err =
            decompress(input.as_slice(), &mut sink, &DecompressionOptions).expect_err("must fail");
        assert!(matches!(err, ZparsError::InvalidFormat(_)));
    }
}
