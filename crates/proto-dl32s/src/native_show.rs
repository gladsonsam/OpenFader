//! DL32S v1 stored-show inspection. Keep unknown fields in original bytes.
use crate::native_archive::{crc32, NativeArchive};
use anyhow::{ensure, Result};
use serde_json::{json, Value};
use std::collections::HashSet;

fn word(bytes: &[u8], offset: usize) -> Result<u32> {
    Ok(u32::from_le_bytes(
        bytes
            .get(offset..offset + 4)
            .ok_or_else(|| anyhow::anyhow!("Truncated native show field"))?
            .try_into()?,
    ))
}
fn text(bytes: &[u8]) -> Result<&str> {
    let n = bytes.iter().position(|b| *b == 0).unwrap_or(bytes.len());
    // Keep all bytes, including bytes after the terminator, in the archive.
    Ok(std::str::from_utf8(&bytes[..n])?)
}
/// An inspection view, not a complete mixer state or restorable backup.
pub fn inspect(archive: &NativeArchive) -> Result<Value> {
    let data = archive
        .entries()
        .get("Show.dat")
        .ok_or_else(|| anyhow::anyhow!("Native Show.dat missing"))?;
    // 76-byte preamble, 65 show words (incl PID0), 16-byte expansion,
    // 100 x 72-byte index rows (incl Default), 65 image words, CRC.
    ensure!(
        data.len() == 7816 && word(data, 4)? == 0x10000,
        "Unsupported native show version/geometry; original archive remains opaque"
    );
    ensure!(
        crc32(&data[4..data.len() - 4]) == word(data, data.len() - 4)?,
        "Native Show.dat CRC mismatch"
    );
    let count = word(data, 348)?;
    ensure!(count <= 99, "Native snapshot count exceeds capacity");
    let mut files = HashSet::new();
    let mut slots = Vec::new();
    for slot in 1..=count {
        let row = 352 + slot as usize * 72;
        let name = text(&data[row..row + 64])?;
        let file = word(data, row + 64)?;
        let crc = word(data, row + 68)?;
        ensure!(
            file != 0 && files.insert(file),
            "Duplicate/protected native snapshot file number"
        );
        let snapshot = archive
            .entries()
            .get(&format!("Snapshot{file}.dat"))
            .ok_or_else(|| anyhow::anyhow!("Referenced native snapshot file missing"))?;
        ensure!(
            snapshot.len() >= 24 && snapshot.len().is_multiple_of(4),
            "Truncated native snapshot"
        );
        ensure!(
            word(snapshot, 0)? == 0x10000
                && word(snapshot, 8)? == 6
                && word(snapshot, 12)? == 5258
                && word(snapshot, 16)? == 5264
                && word(snapshot, 20)? == 1216
                && snapshot.len() == 25920,
            "Unsupported native snapshot version/geometry"
        );
        ensure!(
            word(snapshot, 4)? == crc && crc32(&snapshot[24..]) == crc,
            "Native snapshot/index CRC mismatch"
        );
        slots.push(json!({"slot":slot,"name":name,"file_number":file,"crc":crc}));
    }
    Ok(json!({"format":"dl32s_show_v1", "show_id":word(data,0)?,
        "version":word(data,4)?, "name":text(&data[8..72])?,
        "capacity":99,"slots":slots,"complete":true,
        "archive_bytes":archive.bytes().len(), "archive_crc32":crc32(archive.bytes()),
        "members":archive.entries().iter().map(|(name,bytes)|json!({"name":name,"size":bytes.len(),"crc32":crc32(bytes)})).collect::<Vec<_>>(),
        "current_edits":"unverified", "restore_proven":false,
        "limitations":"Exact original archive preserved, including unknown members and bytes. Index covers stored user snapshots only. No current unsaved edits, device/global settings, safes behavior or exact native restoration are certified. Model is inferred from geometry; no embedded model identity is asserted."}))
}
