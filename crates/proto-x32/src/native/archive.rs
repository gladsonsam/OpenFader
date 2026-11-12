//! Current mix/routing node archives and guarded import.

use super::*;

/// `/node` read: request path without leading slash. The desk answers
/// addressed `node` (no slash), which the shared session decoder drops
/// (only slashless `meters/N` has a repair fallback), so node traffic
/// uses a short-lived raw socket and correlates by the echoed path
/// prefix. Callers must serialize node traffic (the shared write lane)
/// so a foreign `/node` reply cannot be mistaken for ours; replies are
/// attributed by `"<path> "` / `"<path>\n"` prefix, never by timing.
async fn read_node_text(session: &Arc<OscSession>, path: &str) -> anyhow::Result<String> {
    let peer = session
        .socket()
        .peer_addr()
        .map_err(|e| anyhow::anyhow!("x32 native: no desk peer ({e:#})"))?;
    let bare = path.strip_prefix('/').unwrap_or(path);
    let socket = tokio::net::UdpSocket::bind("0.0.0.0:0")
        .await
        .map_err(|e| anyhow::anyhow!("x32 native: node socket ({e:#})"))?;
    let out =
        openfader_transport::osc::encode_message("/node", vec![OscType::String(bare.into())])?;
    socket
        .send_to(&out, peer)
        .await
        .map_err(|e| anyhow::anyhow!("x32 native: /node send ({e:#})"))?;
    let deadline = tokio::time::Instant::now() + LEAF_TIMEOUT;
    let mut buf = vec![0u8; 4096];
    loop {
        let now = tokio::time::Instant::now();
        if now >= deadline {
            anyhow::bail!("x32 native: /node {path} timed out");
        }
        let (n, _) = tokio::time::timeout(deadline - now, socket.recv_from(&mut buf))
            .await
            .map_err(|_| anyhow::anyhow!("x32 native: /node {path} timed out"))?
            .map_err(|e| anyhow::anyhow!("x32 native: /node recv ({e:#})"))?;
        let Some(text) = decode_node_text(&buf[..n]) else {
            continue;
        };
        if text == path
            || text.starts_with(&format!("{path} "))
            || text.starts_with(&format!("{path}\n"))
        {
            return Ok(text);
        }
    }
}

/// Decode a raw desk datagram into node text, accepting the slashless
/// `node` address the shared decoder rejects (rosc requires a leading
/// slash; only `meters/N` has a repair fallback there). Hand-parsed OSC:
/// `node\0\0\0\0 ,s\0\0 <text>\0...`.
pub(super) fn decode_node_text(buf: &[u8]) -> Option<String> {
    let end = buf.iter().position(|&b| b == 0)?;
    if &buf[..end] != b"node" {
        return None;
    }
    let mut off = (end + 4) & !3;
    let tag_end = buf.get(off..)?.iter().position(|&b| b == 0)?;
    if &buf[off..off + tag_end] != b",s" {
        return None;
    }
    off = (off + tag_end + 4) & !3;
    let text_end = buf.get(off..)?.iter().position(|&b| b == 0)?;
    std::str::from_utf8(&buf[off..off + text_end])
        .ok()
        .map(str::to_string)
}

/// `/` node write: single string `"<path-without-slash> <values>"`.
async fn write_node_text(
    session: &Arc<OscSession>,
    path: &str,
    values: &str,
) -> anyhow::Result<()> {
    let bare = path.strip_prefix('/').unwrap_or(path);
    session
        .send("/", vec![OscType::String(format!("{bare} {values}"))])
        .await?;
    Ok(())
}

/// Node paths accepted for archive export/import. Anything outside this
/// allowlist (scene set, routing set, channel/fx preset templates, plus the
/// `/-show` inventory leaves themselves) is rejected before any write.
fn node_allowlisted(path: &str) -> bool {
    if !(path.starts_with('/') && !path.contains("..") && !path.contains('\0')) {
        return false;
    }
    if path.starts_with("/-show/showfile/") || path.starts_with("/config/") {
        return true;
    }
    if tables::SCENE_NODES.contains(&path) || tables::ROUTE_NODES.contains(&path) {
        return true;
    }
    for fam in ["/ch/", "/auxin/", "/fxrtn/", "/bus/", "/mtx/", "/fx/"] {
        if let Some(rest) = path.strip_prefix(fam) {
            let suffix = rest.split_once('/').map(|(_, s)| s).unwrap_or(rest);
            if tables::CH_PRESET_SUFFIXES.contains(&suffix)
                || tables::FX_PRESET_SUFFIXES.contains(&suffix)
            {
                return true;
            }
        }
    }
    path.starts_with("/headamp/") || path.starts_with("/outputs/") || path.starts_with("/-libs/")
}

/// Validate a local archive document without touching the mixer.
pub fn validate_archive(archive: &Value) -> anyhow::Result<Value> {
    anyhow::ensure!(
        archive["schema"] == ARCHIVE_SCHEMA,
        "x32 native: unknown archive schema"
    );
    let kind = archive["kind"].as_str().unwrap_or("");
    let nodes = archive["nodes"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("x32 native: archive has no nodes array"))?;
    anyhow::ensure!(
        !nodes.is_empty() && nodes.len() <= 4096,
        "x32 native: archive node count out of bounds"
    );
    let allow: &[&str] = match kind {
        "scene" => tables::SCENE_NODES,
        "routing" => tables::ROUTE_NODES,
        _ => anyhow::bail!("x32 native: unknown archive kind {kind:?} (scene|routing)"),
    };
    for node in nodes {
        let path = node["path"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("x32 native: archive node without path"))?;
        let text = node["text"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("x32 native: archive node without text"))?;
        anyhow::ensure!(
            allow.contains(&path) && node_allowlisted(path),
            "x32 native: archive path {path:?} outside the {kind} allowlist"
        );
        anyhow::ensure!(
            text.len() <= 2048 && !text.contains('\0'),
            "x32 native: archive node text out of bounds"
        );
    }
    Ok(json!({
        "schema": ARCHIVE_SCHEMA, "kind": kind,
        "slot": archive.get("slot").cloned().unwrap_or(Value::Null),
        "name": archive.get("name").cloned().unwrap_or(Value::Null),
        "nodes": nodes.len(),
    }))
}

impl X32Driver {
    pub(super) async fn export_archive_into(
        &self,
        session: &Arc<OscSession>,
        state: &Mutex<NativeState>,
        action: &Value,
        expected: &Value,
        check: Check<'_>,
    ) -> anyhow::Result<Value> {
        let kind = action_str(action, "kind")?;
        let paths: &[&str] = match kind {
            "scene" => {
                anyhow::ensure!(
                    action.get("slot").is_none(),
                    "x32 native: stored-scene export is unavailable; reopen Export current mix"
                );
                tables::SCENE_NODES
            }
            "routing" => tables::ROUTE_NODES,
            _ => anyhow::bail!("x32 native: unknown export kind {kind:?} (scene|routing)"),
        };
        anyhow::ensure!(
            valid_export_expectation(expected),
            "x32 native: refresh first"
        );
        let _guard = self.writes.lock().await;
        self.ensure_current(session)?;
        anyhow::ensure!(check(), "x32 native: session changed before export");
        // Bounded concurrent node reads; failures are collected, never hidden.
        let mut nodes = Vec::new();
        let mut failed: Vec<String> = Vec::new();
        for (path, got) in fan_out_simple(paths.to_vec(), |path: &str| {
            let session = session.clone();
            async move { (path.to_string(), read_node_text(&session, path).await) }
        })
        .await
        {
            match got {
                Ok(text) => nodes.push(json!({"path": path, "text": text})),
                Err(_) => failed.push(path),
            }
        }
        anyhow::ensure!(check(), "x32 native: session changed during export");
        self.ensure_current(session)?;
        let archive = json!({
            "schema": ARCHIVE_SCHEMA, "kind": kind, "slot": Value::Null,
            "source": if kind == "scene" { "current_mix" } else { "current_routing" },
            "name": action.get("name").cloned().unwrap_or(Value::Null),
            "nodes": nodes,
        });
        let status = if failed.is_empty() {
            "confirmed"
        } else {
            "uncertain"
        };
        let source_label = if kind == "scene" { "mix" } else { "routing" };
        let message = if failed.is_empty() {
            format!(
                "Exported {} current {source_label} node lines locally. This is an OpenFader-side copy, not a desk USB file.",
                nodes.len()
            )
        } else {
            format!(
                "Partial current {source_label} export: {} node lines captured, {} unreadable ({}). Not a desk USB file; re-export on a reliable link before relying on it.",
                nodes.len(),
                failed.len(),
                failed
                    .iter()
                    .take(8)
                    .cloned()
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        };
        let mut result =
            openfader_core::native_library::receipt(status, message, self.inventory_or_null(state));
        result["archive"] = archive;
        if !failed.is_empty() {
            result["unreadable"] = json!(failed);
        }
        Ok(result)
    }

    pub(super) async fn import_archive_into(
        &self,
        session: &Arc<OscSession>,
        state: &Mutex<NativeState>,
        action: &Value,
        expected: &Value,
        check: Check<'_>,
    ) -> anyhow::Result<Value> {
        let archive = action
            .get("archive")
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("x32 native: missing archive"))?;
        let summary = validate_archive(&archive)?;
        anyhow::ensure!(
            valid_export_expectation(expected),
            "x32 native: refresh first"
        );
        let nodes = archive["nodes"].as_array().cloned().unwrap_or_default();
        let _guard = self.writes.lock().await;
        self.ensure_current(session)?;
        anyhow::ensure!(check(), "x32 native: session changed before import");
        // Phase 1: back up current values of every affected path. Fail
        // closed: without a complete backup there is no safe rollback, so
        // nothing is written.
        let mut backup = Vec::new();
        for (path, got) in fan_out_simple(nodes.clone(), |node: Value| {
            let session = session.clone();
            async move {
                let path = node["path"].as_str().unwrap_or("").to_string();
                let got = read_node_text(&session, &path).await;
                (path, got)
            }
        })
        .await
        {
            match got {
                Ok(text) => backup.push(json!({"path": path, "text": text})),
                Err(error) => {
                    anyhow::bail!(
                        "Import refused: current value of {path:?} unreadable ({error:#}); no safe rollback exists so nothing was written."
                    );
                }
            }
        }
        anyhow::ensure!(check(), "x32 native: session changed during backup");
        self.ensure_current(session)?;
        // Phase 2: write each node line sequentially. A mid-sequence
        // failure stops the sequence; progress (applied vs pending) plus
        // the backup captured above are reported so the caller can roll
        // back by re-importing it. Node writes are fire-and-forget on this
        // wire, so a completed import is still reported uncertain.
        let mut applied: Vec<String> = Vec::new();
        let mut failed: Vec<String> = Vec::new();
        for node in &nodes {
            let path = node["path"].as_str().unwrap_or("");
            let text = node["text"].as_str().unwrap_or("");
            // Node text is `/<path> <values>\n`; strip our own echo prefix
            // and trailing newline before sending the value half.
            let values = text
                .strip_prefix(path)
                .unwrap_or(text)
                .trim_start()
                .trim_end_matches(['\n', '\r'])
                .to_string();
            if !check() || self.ensure_current(session).is_err() {
                failed.push(format!("{path} (session changed; stopped)"));
                break;
            }
            match write_node_text(session, path, &values).await {
                Ok(()) => applied.push(path.to_string()),
                Err(error) => {
                    failed.push(format!("{path} ({error:#})"));
                    break;
                }
            }
        }
        // Phase 3: spot-verify the first, middle and last applied paths.
        let mut unverified: Vec<String> = Vec::new();
        if !applied.is_empty() {
            for path in [
                applied.first().cloned().unwrap_or_default(),
                applied.get(applied.len() / 2).cloned().unwrap_or_default(),
                applied.last().cloned().unwrap_or_default(),
            ] {
                if path.is_empty() {
                    continue;
                }
                if read_node_text(session, &path).await.is_err() {
                    unverified.push(path);
                }
            }
        }
        if let Ok(mut cache) = state.lock() {
            cache.revision += 1;
            cache.generation = cache.revision;
            cache.complete = false;
        }
        let mut result = openfader_core::native_library::receipt(
            "uncertain",
            format!(
                "Import attempted on {} paths: {} writes dispatched, {} failed, {} spot-reads unverified. Node writes are fire-and-forget on this wire; nothing here proves audio state. Roll back by importing the bundled backup.",
                nodes.len(),
                applied.len(),
                failed.len(),
                unverified.len(),
            ),
            self.inventory_or_null(state),
        );
        result["progress"] = json!({
            "attempted": nodes.len(), "applied": applied.len(),
            "failed": failed, "unverified": unverified,
        });
        result["backup"] = json!({"schema": ARCHIVE_SCHEMA, "kind": summary["kind"],
            "nodes": backup, "note": "Pre-import values. Re-import to roll back."});
        Ok(result)
    }
}

fn valid_export_expectation(expected: &Value) -> bool {
    expected.get("provider") == Some(&json!("x32"))
        && expected.get("complete") == Some(&json!(true))
}
