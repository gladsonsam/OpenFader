//! Crash-recoverable, narrowly scoped routing changes. No scene is recalled.
//!
//! The provider must perform fresh reads and confirmed writes on the *same*
//! engine connection used by the mixer. This module does not assert that any
//! particular desk mapping has been verified. It is not yet connected to a
//! desk mapping: the engine provider serializes local writers and supplies a
//! non-portable live-session identity. Read/write/read is
//! not an atomic compare-and-swap against an independent remote controller.
use serde_json::{json, Value};
use std::{
    fs::{self, File},
    io::{self, Write},
    path::{Path, PathBuf},
};

#[async_trait::async_trait]
pub trait RoutingProvider: Send {
    fn check_cancelled(&self) -> anyhow::Result<()> {
        Ok(())
    }
    /// Non-portable identity of the pinned live session.
    async fn identity(&mut self) -> anyhow::Result<String>;
    /// Keys comprising the comprehensive routing snapshot.
    fn snapshot_keys(&self) -> Vec<String> {
        Vec::new()
    }
    async fn read(&mut self, key: &str) -> anyhow::Result<i32>;
    async fn write_confirmed(&mut self, key: &str, value: i32) -> anyhow::Result<()>;
    async fn write_if_expected(
        &mut self,
        key: &str,
        expected: i32,
        value: i32,
    ) -> anyhow::Result<()> {
        self.check_cancelled()?;
        anyhow::ensure!(
            self.read(key).await? == expected,
            "routing conflict: {}",
            key
        );
        self.check_cancelled()?;
        self.write_confirmed(key, value).await
    }
}

#[derive(Debug, Clone)]
struct Change {
    key: String,
    before: i32,
    after: i32,
    attempted: bool,
    skipped: bool,
}

/// A journal belongs to another live session and requires explicit rebind.
#[derive(Debug)]
pub struct RecoveryRequired;
impl std::fmt::Display for RecoveryRequired {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("routing recovery requires explicit rebind")
    }
}
impl std::error::Error for RecoveryRequired {}

#[derive(Debug, PartialEq, Eq)]
pub enum ArchivePrecondition {
    ExplicitAcceptanceRequired,
    OutputMustBeStopped,
}
impl std::fmt::Display for ArchivePrecondition {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "archive precondition: {self:?}")
    }
}
impl std::error::Error for ArchivePrecondition {}

/// An existing journal always blocks another transition until it is restored.
pub struct RoutingTransaction {
    path: PathBuf,
}
impl RoutingTransaction {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }
    pub fn pending(&self) -> bool {
        self.path.exists()
    }

    pub fn recovery_keys(&self) -> anyhow::Result<Vec<String>> {
        let (_, changes) = self.load()?;
        anyhow::ensure!(!changes.is_empty(), "empty routing journal");
        Ok(changes.into_iter().map(|change| change.key).collect())
    }

    /// Capture all values before making any changes, then durably publish the
    /// entire intention before the first write. Duplicate keys are rejected.
    pub async fn enter(
        &self,
        provider: &mut impl RoutingProvider,
        desired: &[(String, i32)],
    ) -> anyhow::Result<()> {
        if self.pending() {
            anyhow::bail!("routing recovery is required");
        }
        if desired.is_empty() {
            anyhow::bail!("empty routing transition");
        }
        provider.check_cancelled()?;
        let identity = provider.identity().await?;
        anyhow::ensure!(!identity.is_empty(), "mixer identity unavailable");
        let mut seen = std::collections::HashSet::new();
        let mut changes = Vec::new();
        for (key, after) in desired {
            if !seen.insert(key.clone()) {
                anyhow::bail!("duplicate routing key");
            }
            provider.check_cancelled()?;
            let before = provider.read(key).await?;
            // Desired selectors that are already correct are still snapshot
            // dependencies. Keep them in the journal so rebind can validate
            // the same complete route that was checked before entry.
            changes.push(Change {
                key: key.clone(),
                before,
                after: *after,
                attempted: false,
                skipped: false,
            });
        }
        for key in provider.snapshot_keys() {
            if seen.insert(key.clone()) {
                provider.check_cancelled()?;
                let before = provider.read(&key).await?;
                changes.push(Change {
                    key,
                    before,
                    after: before,
                    attempted: false,
                    skipped: false,
                });
            }
        }
        provider.check_cancelled()?;
        anyhow::ensure!(provider.identity().await? == identity, "session changed");
        self.save(&identity, &changes)?;
        for index in 0..changes.len() {
            let change = &changes[index];
            if change.before == change.after {
                continue;
            }
            provider.check_cancelled()?;
            anyhow::ensure!(
                provider.identity().await? == identity,
                "mixer identity changed"
            );
            // Detect changes made after the snapshot rather than overwriting them.
            provider.check_cancelled()?;
            if provider.read(&change.key).await? != change.before {
                anyhow::bail!("routing changed concurrently: {}", change.key);
            }
            // Persist the intent before the write: a failed or interrupted write
            // may still have reached the desk.
            changes[index].attempted = true;
            self.replace(&identity, &changes)?;
            if let Err(error) = provider.check_cancelled() {
                changes[index].skipped = true;
                self.replace(&identity, &changes)?;
                return Err(error);
            }
            let change = &changes[index];
            provider
                .write_if_expected(&change.key, change.before, change.after)
                .await?;
            provider.check_cancelled()?;
            if provider.read(&change.key).await? != change.after {
                anyhow::bail!("routing write not confirmed: {}", change.key);
            }
        }
        Ok(())
    }
    /// Idempotent recovery after a complete or interrupted transition. A value
    /// modified by another client is never silently overwritten.
    pub async fn restore(&self, provider: &mut impl RoutingProvider) -> anyhow::Result<()> {
        if !self.pending() {
            return Ok(());
        }
        let (identity, changes) = self.load()?;
        provider.check_cancelled()?;
        if provider.identity().await? != identity {
            return Err(RecoveryRequired.into());
        }
        // Check the entire set before restoring anything, so an already visible
        // conflict does not cause a partial recovery. Independent controllers can
        // still race these reads; OSC has no atomic compare-and-swap.
        for change in &changes {
            provider.check_cancelled()?;
            let current = provider.read(&change.key).await?;
            anyhow::ensure!(
                current == change.before
                    || (change.attempted && !change.skipped && current == change.after),
                "routing conflict: {}",
                change.key
            );
        }
        for change in changes.iter().rev() {
            provider.check_cancelled()?;
            anyhow::ensure!(
                provider.identity().await? == identity,
                "mixer identity changed"
            );
            provider.check_cancelled()?;
            let current = provider.read(&change.key).await?;
            if current == change.before {
                continue;
            }
            anyhow::ensure!(change.attempted, "routing conflict: {}", change.key);
            if current != change.after {
                anyhow::bail!("routing conflict: {}", change.key);
            }
            provider.check_cancelled()?;
            provider
                .write_if_expected(&change.key, change.after, change.before)
                .await?;
            provider.check_cancelled()?;
            provider.check_cancelled()?;
            if provider.read(&change.key).await? != change.before {
                anyhow::bail!("routing restore not confirmed: {}", change.key);
            }
        }
        provider.check_cancelled()?;
        fs::remove_file(&self.path)?;
        sync_parent(&self.path)?;
        Ok(())
    }
    /// Explicit user-selected recovery: validate every recorded selector against
    /// its known before/after state before binding the journal to this session.
    /// This does not establish physical mixer identity or write routing.
    pub async fn rebind_for_recovery(
        &self,
        provider: &mut impl RoutingProvider,
    ) -> anyhow::Result<()> {
        let (_, changes) = self.load()?;
        let keys = provider.snapshot_keys();
        anyhow::ensure!(
            !keys.is_empty() && keys.iter().all(|k| changes.iter().any(|c| &c.key == k)),
            "comprehensive routing snapshot unavailable"
        );
        provider.check_cancelled()?;
        let identity = provider.identity().await?;
        for change in &changes {
            provider.check_cancelled()?;
            let current = provider.read(&change.key).await?;
            anyhow::ensure!(
                current == change.before
                    || (change.attempted && !change.skipped && current == change.after),
                "routing conflict: {}",
                change.key
            );
        }
        provider.check_cancelled()?;
        anyhow::ensure!(provider.identity().await? == identity, "session changed");
        self.replace(&identity, &changes)
    }
    /// Archive an already restored legacy journal after explicit acceptance of
    /// any unrecorded routing state. This operation never writes to the mixer.
    pub async fn archive_current_if_restored(
        &self,
        provider: &mut impl RoutingProvider,
        explicit_acceptance: bool,
        output_stopped: bool,
    ) -> anyhow::Result<PathBuf> {
        if !explicit_acceptance {
            return Err(ArchivePrecondition::ExplicitAcceptanceRequired.into());
        }
        if !output_stopped {
            return Err(ArchivePrecondition::OutputMustBeStopped.into());
        }
        let (_, changes) = self.load()?;
        anyhow::ensure!(!changes.is_empty(), "empty routing journal");
        provider.check_cancelled()?;
        let identity = provider.identity().await?;
        anyhow::ensure!(!identity.is_empty(), "mixer identity unavailable");
        for change in &changes {
            provider.check_cancelled()?;
            anyhow::ensure!(
                provider.read(&change.key).await? == change.before,
                "routing conflict: {}",
                change.key
            );
        }
        provider.check_cancelled()?;
        anyhow::ensure!(provider.identity().await? == identity, "session changed");
        provider.check_cancelled()?;
        let parent = self
            .path
            .parent()
            .ok_or_else(|| anyhow::anyhow!("journal has no parent"))?;
        let archive = parent.join("recoveryarchive");
        fs::create_dir_all(&archive)?;
        sync_parent(&archive)?;
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let target = loop {
            let stamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)?
                .as_nanos();
            let candidate = archive.join(format!(
                "{}-{}-{}.json",
                stamp,
                std::process::id(),
                NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            ));
            match fs::hard_link(&self.path, &candidate) {
                Ok(()) => break candidate,
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(e) => return Err(e.into()),
            }
        };
        // The hard link atomically reserves a unique name without replacing an
        // existing archive and preserves the original bytes on the same device.
        if let Err(e) = sync_parent(&target).and_then(|()| fs::remove_file(&self.path)) {
            let _ = fs::remove_file(&target);
            return Err(e.into());
        }
        if let Err(e) = sync_parent(&self.path) {
            // Restore the active name if durability of its removal failed.
            fs::hard_link(&target, &self.path)?;
            sync_parent(&self.path)?;
            return Err(e.into());
        }
        Ok(target)
    }
    fn save(&self, identity: &str, changes: &[Change]) -> anyhow::Result<()> {
        let parent = self
            .path
            .parent()
            .ok_or_else(|| anyhow::anyhow!("journal has no parent"))?;
        fs::create_dir_all(parent)?;
        // create_new prevents replacing an unresolved recovery journal.
        let mut f = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&self.path)?;
        let value = json!({"version": 3, "identity": identity, "changes": changes.iter().map(|c| json!({"key": c.key, "before": c.before, "after": c.after, "attempted": c.attempted, "skipped": c.skipped})).collect::<Vec<_>>()});
        serde_json::to_writer(&mut f, &value)?;
        f.write_all(b"\n")?;
        f.sync_all()?;
        sync_parent(&self.path)?;
        Ok(())
    }
    fn replace(&self, identity: &str, changes: &[Change]) -> anyhow::Result<()> {
        let temp = self.path.with_extension("routing-tmp");
        let value = json!({"version": 3, "identity": identity, "changes": changes.iter().map(|c| json!({"key": c.key, "before": c.before, "after": c.after, "attempted": c.attempted, "skipped": c.skipped})).collect::<Vec<_>>()});
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(&temp)?;
        serde_json::to_writer(&mut file, &value)?;
        file.write_all(b"\n")?;
        file.sync_all()?;
        fs::rename(&temp, &self.path)?;
        sync_parent(&self.path)?;
        Ok(())
    }
    fn load(&self) -> anyhow::Result<(String, Vec<Change>)> {
        let value: Value = serde_json::from_reader(File::open(&self.path)?)?;
        if value["version"] != 3 {
            anyhow::bail!("unsupported routing journal");
        }
        let entries = value["changes"]
            .as_array()
            .ok_or_else(|| anyhow::anyhow!("invalid routing journal"))?;
        let mut seen = std::collections::HashSet::new();
        let identity = value["identity"]
            .as_str()
            .filter(|s| !s.is_empty())
            .ok_or_else(|| anyhow::anyhow!("invalid mixer identity"))?
            .to_owned();
        let changes = entries
            .iter()
            .map(|v| {
                let key = v["key"]
                    .as_str()
                    .filter(|s| !s.is_empty())
                    .ok_or_else(|| anyhow::anyhow!("invalid key"))?
                    .to_owned();
                if !seen.insert(key.clone()) {
                    anyhow::bail!("duplicate routing key");
                }
                let parse = |name: &str| -> anyhow::Result<i32> {
                    Ok(i32::try_from(
                        v[name]
                            .as_i64()
                            .ok_or_else(|| anyhow::anyhow!("invalid value"))?,
                    )?)
                };
                Ok(Change {
                    key,
                    before: parse("before")?,
                    after: parse("after")?,
                    skipped: match v.get("skipped") {
                        None => false,
                        Some(value) => value
                            .as_bool()
                            .ok_or_else(|| anyhow::anyhow!("invalid skipped"))?,
                    },
                    attempted: v["attempted"]
                        .as_bool()
                        .ok_or_else(|| anyhow::anyhow!("invalid step"))?,
                })
            })
            .collect::<anyhow::Result<Vec<_>>>()?;
        Ok((identity, changes))
    }
}
fn sync_parent(path: &Path) -> io::Result<()> {
    #[cfg(unix)]
    {
        File::open(path.parent().unwrap())?.sync_all()?;
    }
    #[cfg(not(unix))]
    {
        let _ = path;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    #[derive(Default)]
    struct Fake {
        values: HashMap<String, i32>,
        fail_after: Option<usize>,
        writes: usize,
        identity: Option<String>,
        cancel_after_writes: Option<usize>,
        snapshot: Vec<String>,
        change_identity: bool,
        identity_reads: usize,
    }
    #[async_trait::async_trait]
    impl RoutingProvider for Fake {
        fn snapshot_keys(&self) -> Vec<String> {
            self.snapshot.clone()
        }
        fn check_cancelled(&self) -> anyhow::Result<()> {
            anyhow::ensure!(self.cancel_after_writes != Some(self.writes), "cancelled");
            Ok(())
        }
        async fn identity(&mut self) -> anyhow::Result<String> {
            self.identity_reads += 1;
            if self.change_identity && self.identity_reads > 1 {
                return Ok("changed".into());
            }
            Ok(self.identity.clone().unwrap_or_else(|| "fake-desk".into()))
        }
        async fn read(&mut self, key: &str) -> anyhow::Result<i32> {
            if key == "input_pair/6/bank" {
                let left = self.values["left"];
                anyhow::ensure!(left == self.values["right"], "unequal banks");
                return Ok(left);
            }
            Ok(*self
                .values
                .get(key)
                .ok_or_else(|| anyhow::anyhow!("missing"))?)
        }
        async fn write_confirmed(&mut self, key: &str, value: i32) -> anyhow::Result<()> {
            if self.fail_after == Some(self.writes) {
                anyhow::bail!("injected failure");
            }
            self.writes += 1;
            if key == "input_pair/6/bank" {
                self.values.insert("left".into(), value);
                self.values.insert("right".into(), value);
                return Ok(());
            }
            self.values.insert(key.to_owned(), value);
            Ok(())
        }
    }
    fn journal() -> RoutingTransaction {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        RoutingTransaction::new(std::env::temp_dir().join(format!(
            "openfader-routing-{}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        )))
    }
    #[tokio::test]
    async fn archive_legacy_only_when_restored_and_explicitly_accepted() {
        let txn = journal();
        let original = br#"{"version":3,"identity":"old","changes":[{"key":"a","before":1,"after":3,"attempted":true},{"key":"b","before":2,"after":4,"attempted":true}]}
"#;
        fs::write(&txn.path, original).unwrap();
        let mut p = Fake::default();
        p.values
            .extend([("a".into(), 1), ("b".into(), 2), ("usb".into(), 9)]);
        p.snapshot = vec!["a".into(), "b".into(), "usb".into()];
        assert!(txn.rebind_for_recovery(&mut p).await.is_err());
        assert!(txn
            .archive_current_if_restored(&mut p, false, true)
            .await
            .unwrap_err()
            .downcast_ref::<ArchivePrecondition>()
            .is_some());
        assert!(txn
            .archive_current_if_restored(&mut p, true, false)
            .await
            .unwrap_err()
            .downcast_ref::<ArchivePrecondition>()
            .is_some());
        for value in [3, 99] {
            p.values.insert("a".into(), value);
            assert!(txn
                .archive_current_if_restored(&mut p, true, true)
                .await
                .is_err());
            assert_eq!(fs::read(&txn.path).unwrap(), original);
        }
        p.values.insert("a".into(), 1);
        p.cancel_after_writes = Some(0);
        assert!(txn
            .archive_current_if_restored(&mut p, true, true)
            .await
            .is_err());
        p.cancel_after_writes = None;
        p.change_identity = true;
        p.identity_reads = 0;
        assert!(txn
            .archive_current_if_restored(&mut p, true, true)
            .await
            .is_err());
        assert_eq!(fs::read(&txn.path).unwrap(), original);
        p.change_identity = false;
        let archive = txn
            .archive_current_if_restored(&mut p, true, true)
            .await
            .unwrap();
        assert_eq!(fs::read(&archive).unwrap(), original);
        assert!(!txn.pending());
        assert_eq!(p.writes, 0);
        fs::remove_file(archive).unwrap();
    }
    #[tokio::test]
    async fn archive_rejects_duplicate_keys() {
        let txn = journal();
        let original = br#"{"version":3,"identity":"old","changes":[{"key":"a","before":1,"after":2,"attempted":true},{"key":"a","before":1,"after":2,"attempted":true}]}"#;
        fs::write(&txn.path, original).unwrap();
        let mut p = Fake::default();
        assert!(txn
            .archive_current_if_restored(&mut p, true, true)
            .await
            .is_err());
        assert_eq!(fs::read(&txn.path).unwrap(), original);
        assert_eq!(p.writes, 0);
        fs::remove_file(txn.path).unwrap();
    }
    #[tokio::test]
    async fn desired_noop_remains_in_comprehensive_snapshot_for_rebind() {
        let txn = journal();
        let mut p = Fake::default();
        p.values
            .extend([("bank".into(), 0), ("usb".into(), 7), ("analog".into(), 3)]);
        p.snapshot = vec!["bank".into(), "usb".into(), "analog".into()];
        txn.enter(&mut p, &[("bank".into(), 1), ("usb".into(), 7)])
            .await
            .unwrap();
        let (_, changes) = txn.load().unwrap();
        assert_eq!(changes.len(), 3);
        let usb = changes.iter().find(|c| c.key == "usb").unwrap();
        assert_eq!((usb.before, usb.after, usb.attempted), (7, 7, false));
        assert_eq!(p.writes, 1);
        p.identity = Some("new-session".into());
        txn.rebind_for_recovery(&mut p).await.unwrap();
        txn.restore(&mut p).await.unwrap();
        assert_eq!(p.values["bank"], 0);
        assert_eq!(p.values["usb"], 7);
    }
    #[tokio::test]
    async fn paired_attempt_rolls_back_both_after_later_failure() {
        let txn = journal();
        let mut p = Fake::default();
        p.values
            .extend([("left".into(), 0), ("right".into(), 0), ("later".into(), 0)]);
        p.fail_after = Some(1);
        assert!(txn
            .enter(
                &mut p,
                &[("input_pair/6/bank".into(), 1), ("later".into(), 1)]
            )
            .await
            .is_err());
        assert_eq!((p.values["left"], p.values["right"]), (1, 1));
        p.fail_after = None;
        txn.restore(&mut p).await.unwrap();
        assert_eq!((p.values["left"], p.values["right"]), (0, 0));
    }
    #[tokio::test]
    async fn paired_peer_edit_blocks_restore() {
        let txn = journal();
        let mut p = Fake::default();
        p.values.extend([("left".into(), 0), ("right".into(), 0)]);
        txn.enter(&mut p, &[("input_pair/6/bank".into(), 1)])
            .await
            .unwrap();
        p.values.insert("right".into(), 0);
        assert!(txn.restore(&mut p).await.is_err());
        assert_eq!((p.values["left"], p.values["right"]), (1, 0));
        fs::remove_file(txn.path).unwrap();
    }
    #[tokio::test]
    async fn cancellation_stops_after_first_write_and_retains_journal() {
        let txn = journal();
        let mut p = Fake::default();
        p.values.extend([("a".into(), 1), ("b".into(), 2)]);
        p.cancel_after_writes = Some(1);
        assert!(txn
            .enter(&mut p, &[("a".into(), 3), ("b".into(), 4)])
            .await
            .is_err());
        assert_eq!(p.writes, 1);
        assert!(txn.pending());
        p.cancel_after_writes = None;
        txn.restore(&mut p).await.unwrap();
        assert_eq!(p.values["a"], 1);
        assert_eq!(p.values["b"], 2);
    }

    #[tokio::test]
    async fn known_skipped_attempt_cannot_restore_coincidental_value() {
        let txn = journal();
        let mut p = Fake::default();
        p.values.insert("a".into(), 2);
        txn.save(
            "fake-desk",
            &[Change {
                key: "a".into(),
                before: 1,
                after: 2,
                attempted: true,
                skipped: true,
            }],
        )
        .unwrap();
        assert!(txn.restore(&mut p).await.is_err());
        assert_eq!(p.writes, 0);
        fs::remove_file(txn.path).unwrap();
    }

    #[tokio::test]
    async fn interrupted_transition_recovers_only_routing() {
        let txn = journal();
        let mut p = Fake::default();
        p.values
            .extend([("a".into(), 1), ("b".into(), 2), ("fader".into(), 7)]);
        p.fail_after = Some(1);
        assert!(txn
            .enter(&mut p, &[("a".into(), 3), ("b".into(), 4)])
            .await
            .is_err());
        p.values.insert("fader".into(), 9);
        p.fail_after = None;
        RoutingTransaction::new(&txn.path)
            .restore(&mut p)
            .await
            .unwrap();
        assert_eq!(p.values["a"], 1);
        assert_eq!(p.values["b"], 2);
        assert_eq!(p.values["fader"], 9);
        txn.restore(&mut p).await.unwrap();
        assert!(!txn.pending());
    }
    #[tokio::test]
    async fn conflict_leaves_journal_for_manual_recovery() {
        let txn = journal();
        let mut p = Fake::default();
        p.values.insert("a".into(), 1);
        txn.enter(&mut p, &[("a".into(), 2)]).await.unwrap();
        p.values.insert("a".into(), 3);
        assert!(txn.restore(&mut p).await.is_err());
        assert!(txn.pending());
        assert_eq!(p.values["a"], 3);
        fs::remove_file(txn.path).unwrap();
    }
    #[tokio::test]
    async fn different_mixer_cannot_restore_journal() {
        let txn = journal();
        let mut p = Fake::default();
        p.values.insert("a".into(), 1);
        txn.enter(&mut p, &[("a".into(), 2)]).await.unwrap();
        p.identity = Some("other-desk".into());
        assert!(txn.restore(&mut p).await.is_err());
        assert_eq!(p.values["a"], 2);
        assert!(txn.pending());
        p.identity = None;
        txn.restore(&mut p).await.unwrap();
    }

    #[tokio::test]
    async fn preflight_conflict_does_not_partially_restore() {
        let txn = journal();
        let mut p = Fake::default();
        p.values.extend([("a".into(), 1), ("b".into(), 2)]);
        txn.enter(&mut p, &[("a".into(), 3), ("b".into(), 4)])
            .await
            .unwrap();
        p.values.insert("a".into(), 9);
        assert!(txn.restore(&mut p).await.is_err());
        assert_eq!(p.values["b"], 4);
        std::fs::remove_file(txn.path).unwrap();
    }

    #[tokio::test]
    async fn incomplete_snapshot_never_writes_or_creates_journal() {
        let txn = journal();
        let mut p = Fake::default();
        p.values.insert("a".into(), 1);
        assert!(txn
            .enter(&mut p, &[("a".into(), 2), ("missing".into(), 3)])
            .await
            .is_err());
        assert_eq!(p.writes, 0);
        assert_eq!(p.values["a"], 1);
        assert!(!txn.pending());
    }

    #[tokio::test]
    async fn failed_restore_remains_recoverable() {
        let txn = journal();
        let mut p = Fake::default();
        p.values.insert("a".into(), 1);
        p.values.insert("b".into(), 3);
        txn.enter(&mut p, &[("a".into(), 2), ("b".into(), 4)])
            .await
            .unwrap();
        p.fail_after = Some(3);
        assert!(txn.restore(&mut p).await.is_err());
        assert!(txn.pending());
        assert_eq!(p.values["b"], 3);
        assert_eq!(p.values["a"], 2);
        p.fail_after = None;
        txn.restore(&mut p).await.unwrap();
        assert_eq!(p.values["a"], 1);
        assert_eq!(p.values["b"], 3);
        assert!(!txn.pending());
    }
}
