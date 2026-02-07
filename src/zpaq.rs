use crate::error::{Result, ZparsError};
use std::fs;
use std::path::Path;

const START_TAG_13: [u8; 13] = [
    0x37, 0x6b, 0x53, 0x74, 0xa0, 0x31, 0x83, 0xd3, 0x8c, 0xb2, 0x28, 0xb0, 0xd3,
];
const MAGIC_16: [u8; 16] = [
    0x37, 0x6b, 0x53, 0x74, 0xa0, 0x31, 0x83, 0xd3, 0x8c, 0xb2, 0x28, 0xb0, 0xd3, b'z', b'P', b'Q',
];
const COMP_SIZE: [u8; 10] = [0, 2, 3, 2, 3, 4, 6, 6, 3, 5];

#[derive(Debug, Clone)]
pub struct ZpaqBlockHeader {
    pub start_offset: usize,
    pub level: u8,
    pub zpaql_type: u8,
    pub hsize: u16,
    pub hh: u8,
    pub hm: u8,
    pub ph: u8,
    pub pm: u8,
    pub n_components: u8,
    pub comp_bytes: usize,
    pub hcomp_bytes: usize,
    pub segment_offset: usize,
}

pub fn inspect_file(path: &Path) -> Result<Vec<ZpaqBlockHeader>> {
    let data = fs::read(path)?;
    inspect_bytes(&data)
}

pub fn inspect_bytes(data: &[u8]) -> Result<Vec<ZpaqBlockHeader>> {
    let mut out = Vec::new();
    let mut i = 0usize;

    while i + MAGIC_16.len() + 2 < data.len() {
        let Some(rel) = find_magic(&data[i..]) else {
            break;
        };
        let at = i + rel;

        let Some((block, consumed)) = parse_block_header(data, at)? else {
            i = at + 1;
            continue;
        };

        out.push(block);
        i = at + consumed;
    }

    Ok(out)
}

fn find_magic(haystack: &[u8]) -> Option<usize> {
    haystack
        .windows(MAGIC_16.len())
        .position(|w| w == MAGIC_16.as_slice())
}

fn parse_block_header(data: &[u8], at: usize) -> Result<Option<(ZpaqBlockHeader, usize)>> {
    if at + MAGIC_16.len() + 2 > data.len() {
        return Ok(None);
    }

    if data[at..at + START_TAG_13.len()] != START_TAG_13 {
        return Ok(None);
    }

    let mut p = at + MAGIC_16.len();
    let level = data[p];
    p += 1;
    if level != 1 && level != 2 {
        return Ok(None);
    }

    let zpaql_type = data[p];
    p += 1;
    if zpaql_type != 1 {
        return Ok(None);
    }

    if p + 7 > data.len() {
        return Err(ZparsError::Corrupt("truncated ZPAQL header prefix"));
    }

    let hsize = u16::from_le_bytes([data[p], data[p + 1]]);
    let hh = data[p + 2];
    let hm = data[p + 3];
    let ph = data[p + 4];
    let pm = data[p + 5];
    let n_components = data[p + 6];

    let header_start = p;
    let header_total = hsize as usize + 2;
    if header_start + header_total > data.len() {
        return Err(ZparsError::Corrupt("truncated ZPAQL header"));
    }

    let mut cp = header_start + 7;
    for _ in 0..n_components {
        if cp >= header_start + header_total {
            return Err(ZparsError::Corrupt("COMP overflows header"));
        }
        let t = data[cp] as usize;
        if t >= COMP_SIZE.len() || COMP_SIZE[t] == 0 {
            return Err(ZparsError::Corrupt("invalid component type"));
        }
        let sz = COMP_SIZE[t] as usize;
        if cp + sz > header_start + header_total {
            return Err(ZparsError::Corrupt("component overflows header"));
        }
        cp += sz;
    }

    if cp >= header_start + header_total || data[cp] != 0 {
        return Err(ZparsError::Corrupt("missing COMP END"));
    }
    cp += 1;

    let comp_bytes = cp - (header_start + 2);
    if comp_bytes > hsize as usize {
        return Err(ZparsError::Corrupt("invalid hsize/COMP layout"));
    }

    let hcomp_bytes = hsize as usize - comp_bytes;
    if hcomp_bytes == 0 {
        return Err(ZparsError::Corrupt("missing HCOMP"));
    }

    if data[header_start + header_total - 1] != 0 {
        return Err(ZparsError::Corrupt("missing HCOMP END"));
    }

    let segment_offset = header_start + header_total;
    let consumed = (segment_offset - at).max(1);

    Ok(Some((
        ZpaqBlockHeader {
            start_offset: at,
            level,
            zpaql_type,
            hsize,
            hh,
            hm,
            ph,
            pm,
            n_components,
            comp_bytes,
            hcomp_bytes,
            segment_offset,
        },
        consumed,
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_non_magic_input() {
        let blocks = inspect_bytes(b"hello world").expect("inspect");
        assert!(blocks.is_empty());
    }

    #[test]
    fn parses_minimal_header() {
        // Build one valid block header with no segments.
        // hsize=7 means COMP bytes=6 (hh..n + end), HCOMP bytes=1 (end only).
        let mut buf = Vec::new();
        buf.extend_from_slice(&MAGIC_16);
        buf.push(2); // level
        buf.push(1); // zpaql type
        buf.extend_from_slice(&7u16.to_le_bytes()); // hsize
        buf.extend_from_slice(&[0, 0, 0, 0, 0]); // hh hm ph pm n
        buf.push(0); // COMP END
        buf.push(0); // HCOMP END

        let blocks = inspect_bytes(&buf).expect("inspect");
        assert_eq!(blocks.len(), 1);
        let b = &blocks[0];
        assert_eq!(b.level, 2);
        assert_eq!(b.zpaql_type, 1);
        assert_eq!(b.hsize, 7);
        assert_eq!(b.n_components, 0);
    }
}
