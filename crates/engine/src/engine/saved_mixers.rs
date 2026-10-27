//! Atomic saved-mixer address persistence and reload.

use super::*;

/// Known desk addresses file (`desks.json`) under the data dir.
pub(super) fn desk_file(data_dir: Option<&std::path::Path>) -> Option<PathBuf> {
    data_dir.map(|d| d.join("desks.json"))
}

/// Load the remembered driver -> desk address map (best-effort: a missing
/// or corrupt file reads back empty, persistence must never block start).
/// Broadcast discovery never crosses subnets, so without this a manually
/// added desk vanishes from Scan after an app restart.
pub(super) fn load_desks(data_dir: Option<&std::path::Path>) -> HashMap<String, String> {
    let Some(path) = desk_file(data_dir) else {
        return HashMap::new();
    };
    let raw = std::fs::read_to_string(&path).unwrap_or_default();
    if raw.trim().is_empty() {
        return HashMap::new();
    }
    let v: Value = serde_json::from_str(&raw).unwrap_or(Value::Null);
    let mut out = HashMap::new();
    if let Some(obj) = v.get("desks").and_then(Value::as_object) {
        for (driver, ip) in obj {
            let Some(ip) = ip.as_str() else {
                continue;
            };
            let ip = ip.trim();
            if !ip.is_empty() {
                out.insert(driver.clone(), ip.to_string());
            }
        }
    }
    out
}

/// Persist the remembered driver -> desk address map (best-effort:
/// failures are logged and swallowed — a read-only data dir must never
/// break connect/probe). Atomic temp-file + rename (no direct truncate)
/// so a racing `forget_saved_mixers` clear never sees a torn file.
/// Untouched by the local-library kernel, which owns `scenes.json`
/// alone.
pub(super) fn persist_desks(data_dir: Option<&std::path::Path>, addrs: &HashMap<String, String>) {
    let Some(dir) = data_dir else {
        return;
    };
    match write_desks_atomic(dir, addrs) {
        Ok(DesksWriteOutcome::Committed) => {}
        Ok(DesksWriteOutcome::Uncertain(error)) | Err(error) => {
            eprintln!("engine: desks persist failed: {error}");
        }
    }
}

pub(super) enum DesksWriteOutcome {
    Committed,
    Uncertain(String),
}

/// Atomic durable write of the remembered desk map (`desks.json`):
/// serialize, `create_new` a unique owned sibling temp
/// (`.desks.json.tmp-<pid>-<nanos>-<attempt>`), hold that handle through
/// `write_all` + `flush` + `sync_all`, rename over the target, then sync
/// the directory where supported. Pre-replace failures return `Err` with
/// map and disk preserved (temp cleanup errors are appended, never
/// silently dropped); the single post-replace failure (directory sync)
/// returns an explicit `Uncertain` outcome (the rename landed — no
/// rollback claim). No caller infers write provenance from error text.
pub(super) fn write_desks_atomic(
    dir: &std::path::Path,
    addrs: &HashMap<String, String>,
) -> Result<DesksWriteOutcome, String> {
    let doc = json!({ "version": 1, "desks": addrs });
    let bytes = serde_json::to_vec_pretty(&doc)
        .map_err(|e| format!("saved mixers serialize failed: {e}"))?;
    if let Err(e) = std::fs::create_dir_all(dir) {
        return Err(format!(
            "saved mixers dir {} not creatable: {e}",
            dir.display()
        ));
    }
    let target = dir.join("desks.json");
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let pid = std::process::id();
    let mut temp_path: Option<PathBuf> = None;
    let mut temp_file: Option<std::fs::File> = None;
    for attempt in 0..8u64 {
        let candidate = dir.join(format!(".desks.json.tmp-{pid}-{nanos}-{attempt}"));
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&candidate)
        {
            Ok(f) => {
                temp_path = Some(candidate);
                temp_file = Some(f);
                break;
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => {
                return Err(format!(
                    "saved mixers temp {} not creatable: {e}",
                    candidate.display()
                ));
            }
        }
    }
    let (Some(temp), Some(mut f)) = (temp_path, temp_file) else {
        return Err("saved mixers temp file not creatable (name collision)".to_string());
    };
    let remove_temp = |temp: &std::path::Path| {
        if let Err(e) = std::fs::remove_file(temp) {
            if e.kind() != std::io::ErrorKind::NotFound {
                return format!("; temp cleanup {} failed: {e}", temp.display());
            }
        }
        String::new()
    };
    if let Err(e) = (|| -> std::io::Result<()> {
        use std::io::Write as _;
        f.write_all(&bytes)?;
        f.flush()?;
        f.sync_all()?;
        Ok(())
    })() {
        let tail = remove_temp(&temp);
        return Err(format!(
            "saved mixers temp write {} failed: {e}{tail}",
            temp.display()
        ));
    }
    drop(f);
    if let Err(e) = std::fs::rename(&temp, &target) {
        let tail = remove_temp(&temp);
        return Err(format!(
            "saved mixers rename {} -> {} failed: {e}{tail}",
            temp.display(),
            target.display()
        ));
    }
    if let Err(e) = sync_desks_dir(dir) {
        return Ok(DesksWriteOutcome::Uncertain(format!(
            "saved mixers dir sync {} failed (saved mixers may have cleared): {e}",
            dir.display()
        )));
    }
    Ok(DesksWriteOutcome::Committed)
}

#[cfg(unix)]
pub(super) fn sync_desks_dir(dir: &std::path::Path) -> std::io::Result<()> {
    let f = std::fs::File::open(dir)?;
    f.sync_all()
}

#[cfg(not(unix))]
pub(super) fn sync_desks_dir(_dir: &std::path::Path) -> std::io::Result<()> {
    Ok(())
}
