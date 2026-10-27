//! Lossless, read-only native ZIP container. Original bytes are the export;
//! decoded entries are inspection views only. No native restore is implied.
use anyhow::{ensure, Result};
use std::collections::BTreeMap;

const LIMIT: usize = 32 * 1024 * 1024;
#[derive(Debug)]
pub struct NativeArchive {
    original: Vec<u8>,
    entries: BTreeMap<String, Vec<u8>>,
}
fn u16le(bytes: &[u8], offset: usize) -> Result<usize> {
    let b = bytes
        .get(offset..offset + 2)
        .ok_or_else(|| anyhow::anyhow!("Truncated ZIP field"))?;
    Ok(u16::from_le_bytes(b.try_into()?) as usize)
}
fn u32le(bytes: &[u8], offset: usize) -> Result<usize> {
    let b = bytes
        .get(offset..offset + 4)
        .ok_or_else(|| anyhow::anyhow!("Truncated ZIP field"))?;
    Ok(u32::from_le_bytes(b.try_into()?) as usize)
}
pub(crate) fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = !0u32;
    for &byte in bytes {
        crc ^= byte as u32;
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0xedb88320u32 & (0u32.wrapping_sub(crc & 1)));
        }
    }
    !crc
}
impl NativeArchive {
    pub fn parse(bytes: Vec<u8>) -> Result<Self> {
        ensure!(
            (22..=LIMIT).contains(&bytes.len()),
            "Native ZIP size outside supported bounds"
        );
        let end = (bytes.len().saturating_sub(65557)..=bytes.len() - 22)
            .rev()
            .find(|&p| {
                bytes[p..p + 4] == *b"PK\x05\x06"
                    && u16le(&bytes, p + 20).is_ok_and(|n| p + 22 + n == bytes.len())
            })
            .ok_or_else(|| anyhow::anyhow!("ZIP end record missing"))?;
        ensure!(
            u16le(&bytes, end + 4)? == 0 && u16le(&bytes, end + 6)? == 0,
            "Split ZIP unsupported"
        );
        let count = u16le(&bytes, end + 10)?;
        ensure!(
            count > 0 && count <= 2048 && count == u16le(&bytes, end + 8)?,
            "Invalid ZIP entry count"
        );
        let start = u32le(&bytes, end + 16)?;
        let length = u32le(&bytes, end + 12)?;
        ensure!(
            start.checked_add(length) == Some(end),
            "Invalid/ZIP64 central directory"
        );
        let mut cursor = start;
        let mut total = 0usize;
        let mut ranges = Vec::new();
        let mut entries = BTreeMap::new();
        for _ in 0..count {
            ensure!(
                bytes.get(cursor..cursor + 4) == Some(b"PK\x01\x02"),
                "Invalid ZIP directory entry"
            );
            let flags = u16le(&bytes, cursor + 8)?;
            let method = u16le(&bytes, cursor + 10)?;
            ensure!(
                flags & !0x808 == 0 && matches!(method, 0 | 8),
                "Encrypted/unsupported ZIP encoding"
            );
            let crc = u32le(&bytes, cursor + 16)? as u32;
            let compressed = u32le(&bytes, cursor + 20)?;
            let size = u32le(&bytes, cursor + 24)?;
            let name_len = u16le(&bytes, cursor + 28)?;
            let extra = u16le(&bytes, cursor + 30)?;
            let comment = u16le(&bytes, cursor + 32)?;
            ensure!(
                u16le(&bytes, cursor + 34)? == 0,
                "Split ZIP member unsupported"
            );
            let local = u32le(&bytes, cursor + 42)?;
            let next = cursor
                .checked_add(46 + name_len + extra + comment)
                .ok_or_else(|| anyhow::anyhow!("ZIP directory overflow"))?;
            ensure!(next <= end, "Truncated ZIP directory");
            let raw_name = &bytes[cursor + 46..cursor + 46 + name_len];
            let name = std::str::from_utf8(raw_name)?;
            ensure!(
                !name.is_empty()
                    && !name.starts_with('/')
                    && !name.contains(['\\', ':'])
                    && !name.chars().any(char::is_control)
                    && name
                        .trim_end_matches('/')
                        .split('/')
                        .all(|s| !s.is_empty() && s != "." && s != ".."),
                "Unsafe native ZIP member name"
            );
            ensure!(!entries.contains_key(name), "Duplicate native ZIP member");
            ensure!(
                bytes.get(local..local + 4) == Some(b"PK\x03\x04") && local < start,
                "Invalid ZIP local entry"
            );
            ensure!(
                u16le(&bytes, local + 6)? == flags && u16le(&bytes, local + 8)? == method,
                "ZIP header mismatch"
            );
            let local_name = u16le(&bytes, local + 26)?;
            let data_start = local
                .checked_add(30 + local_name + u16le(&bytes, local + 28)?)
                .ok_or_else(|| anyhow::anyhow!("ZIP local overflow"))?;
            let data_end = data_start
                .checked_add(compressed)
                .ok_or_else(|| anyhow::anyhow!("ZIP data overflow"))?;
            ensure!(
                data_end <= start
                    && bytes.get(local + 30..local + 30 + local_name) == Some(raw_name),
                "ZIP local name/size mismatch"
            );
            if flags & 8 == 0 {
                ensure!(
                    u32le(&bytes, local + 14)? == crc as usize
                        && u32le(&bytes, local + 18)? == compressed
                        && u32le(&bytes, local + 22)? == size,
                    "ZIP local integrity mismatch"
                );
            }
            let mut range_end = data_end;
            if flags & 8 != 0 {
                let descriptor = if bytes.get(data_end..data_end + 4) == Some(b"PK\x07\x08") {
                    data_end + 4
                } else {
                    data_end
                };
                range_end = descriptor + 12;
                ensure!(
                    range_end <= start
                        && u32le(&bytes, descriptor)? == crc as usize
                        && u32le(&bytes, descriptor + 4)? == compressed
                        && u32le(&bytes, descriptor + 8)? == size,
                    "ZIP descriptor mismatch"
                );
            }
            ensure!(
                ranges.iter().all(|&(a, b)| range_end <= a || local >= b),
                "Overlapping ZIP entries"
            );
            ranges.push((local, range_end));
            total = total
                .checked_add(size)
                .ok_or_else(|| anyhow::anyhow!("ZIP inflated size overflow"))?;
            ensure!(total <= LIMIT, "Native ZIP inflated contents exceed 32 MiB");
            let packed = &bytes[data_start..data_end];
            let data = if method == 0 {
                ensure!(compressed == size, "Stored ZIP size mismatch");
                packed.to_vec()
            } else {
                super::native_inflate::inflate(packed, size)?
            };
            ensure!(crc32(&data) == crc, "Native ZIP member CRC mismatch");
            entries.insert(name.to_owned(), data);
            cursor = next;
        }
        ensure!(cursor == end, "ZIP directory size mismatch");
        Ok(Self {
            original: bytes,
            entries,
        })
    }
    /// Byte-exact including unknown members, compression, extra fields, comments,
    /// names and ordering. Never rebuild from the interpreted show index.
    pub fn bytes(&self) -> &[u8] {
        &self.original
    }
    pub fn entries(&self) -> &BTreeMap<String, Vec<u8>> {
        &self.entries
    }
}
