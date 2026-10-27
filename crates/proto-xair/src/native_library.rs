//! XR18 native snapshot slots. These are separate from OpenFader scenes.
use super::*;
use openfader_core::native_library::{receipt, Check};
use serde_json::{json, Value};
use std::sync::atomic::Ordering;
use std::time::Duration;

#[derive(Default)]
pub(super) struct Inventory {
    generation: u64,
    revision: u64,
    names: Option<Vec<String>>,
    scopes: Option<Vec<String>>,
    last_slot: Option<usize>,
}

impl XAirDriver {
    pub(super) fn native_inventory(&self) -> Value {
        let generation = self.connection_generation.load(Ordering::Acquire);
        let connected = self.connected.load(Ordering::Acquire);
        let cache = self.native.lock().unwrap();
        let names = if cache.generation == generation && connected {
            cache.names.as_ref()
        } else {
            None
        };
        let slots: Vec<_> = names
            .into_iter()
            .flat_map(|names| names.iter().enumerate())
            .map(|(i, name)| json!({"slot":i + 1,"name":name,"scope":cache.scopes.as_ref().and_then(|s| s.get(i))}))
            .collect();
        json!({"authority":"mixer", "provider":"xair", "label":"XR18 native snapshots", "connected":connected,
            "generation":generation, "revision":cache.revision, "capacity":64, "last_slot":if names.is_some() {cache.last_slot} else {None},
            "complete":names.is_some(), "slots":slots,
            "operations":if names.is_some() {vec!["inspect", "refresh", "recall", "replace", "rename", "delete", "set_scope"]} else {vec!["inspect", "refresh"]},
            "limitations":"Names are read from the mixer; an empty name does not prove a slot is empty. Native recall scope is stored on the mixer. Saves and recalls are sent once and their audio or full contents are not independently verified. Refresh after changes."
        })
    }

    pub(super) async fn native_action(
        &self,
        action: &Value,
        expected: &Value,
        check: Check<'_>,
    ) -> anyhow::Result<Value> {
        let session = self.session().await?.as_ref().unwrap().clone();
        let generation = self.connection_generation.load(Ordering::Acquire);
        let valid = || check() && self.connection_generation.load(Ordering::Acquire) == generation;
        anyhow::ensure!(
            valid() && self.native_inventory() == *expected,
            "Native session or inventory changed"
        );
        let op = action["op"].as_str().unwrap_or("");
        if op == "refresh" {
            let mut names = Vec::with_capacity(64);
            let mut scopes = Vec::with_capacity(64);
            let last_slot = read_index(&session).await?;
            for slot in 1..=64 {
                anyhow::ensure!(valid(), "Native session changed during refresh");
                names.push(read_name(&session, slot).await?);
                scopes.push(read_scope(&session, slot).await?);
            }
            anyhow::ensure!(
                valid() && self.native_inventory() == *expected,
                "Native session or inventory changed during refresh"
            );
            let mut cache = self.native.lock().unwrap();
            cache.generation = generation;
            cache.revision += 1;
            cache.names = Some(names);
            cache.scopes = Some(scopes);
            cache.last_slot = Some(last_slot);
            drop(cache);
            return Ok(receipt(
                "confirmed",
                "Snapshot names, scopes and last loaded or saved slot read from the mixer.",
                self.native_inventory(),
            ));
        }
        let slot = action["slot"]
            .as_u64()
            .filter(|s| (1..=64).contains(s))
            .ok_or_else(|| anyhow::anyhow!("Snapshot slot must be 1–64"))?
            as usize;
        let previous = expected["slots"]
            .as_array()
            .and_then(|slots| slots.get(slot - 1))
            .and_then(|s| s["name"].as_str())
            .ok_or_else(|| anyhow::anyhow!("Refresh snapshot slots first"))?;
        anyhow::ensure!(
            read_name(&session, slot).await? == previous,
            "Slot name changed; refresh first"
        );
        if op == "set_scope" {
            let prior = expected["slots"][slot - 1]["scope"]
                .as_str()
                .ok_or_else(|| anyhow::anyhow!("Refresh snapshot scopes first"))?;
            let next = action["scope"]
                .as_str()
                .ok_or_else(|| anyhow::anyhow!("Missing snapshot scope"))?;
            anyhow::ensure!(valid_scope(next), "Invalid snapshot scope");
            anyhow::ensure!(
                read_scope(&session, slot).await? == prior,
                "Snapshot scope changed; refresh first"
            );
            let last = expected["last_slot"]
                .as_u64()
                .ok_or_else(|| anyhow::anyhow!("Snapshot index unavailable"))?;
            anyhow::ensure!(
                read_index(&session).await? as u64 == last,
                "Last snapshot changed; refresh first"
            );
            anyhow::ensure!(
                valid() && self.native_inventory() == *expected,
                "Native session or inventory changed"
            );
            let held_session = self.session().await?;
            anyhow::ensure!(
                valid()
                    && held_session
                        .as_ref()
                        .is_some_and(|current| std::sync::Arc::ptr_eq(current, &session)),
                "Native session changed"
            );
            let sent = session
                .send(
                    &format!("/-snap/{slot:02}/scope"),
                    vec![OscType::String(next.to_owned())],
                )
                .await;
            drop(held_session);
            if sent.is_err() || !valid() {
                let mut cache = self.native.lock().unwrap();
                cache.names = None;
                cache.scopes = None;
                cache.last_slot = None;
                cache.revision += 1;
                drop(cache);
                return Ok(receipt(
                    "uncertain",
                    "Scope command delivery uncertain; refresh before retrying.",
                    self.native_inventory(),
                ));
            }
            let actual = read_scope(&session, slot).await;
            let actual_name = read_name(&session, slot).await;
            let actual_index = read_index(&session).await;
            if !valid()
                || actual.as_deref().ok() != Some(next)
                || actual_name.as_deref().ok() != Some(previous)
                || actual_index.ok() != Some(last as usize)
            {
                let mut cache = self.native.lock().unwrap();
                cache.names = None;
                cache.scopes = None;
                cache.last_slot = None;
                cache.revision += 1;
                drop(cache);
                return Ok(receipt(
                    "uncertain",
                    "Scope readback or snapshot context changed; refresh before retrying.",
                    self.native_inventory(),
                ));
            }
            let mut cache = self.native.lock().unwrap();
            if cache.generation != generation
                || cache.revision != expected["revision"].as_u64().unwrap_or(u64::MAX)
            {
                cache.names = None;
                cache.scopes = None;
                cache.last_slot = None;
                cache.revision += 1;
                drop(cache);
                return Ok(receipt(
                    "uncertain",
                    "Snapshot inventory changed during readback; refresh first.",
                    self.native_inventory(),
                ));
            }
            cache.scopes.as_mut().unwrap()[slot - 1] = next.to_owned();
            cache.revision += 1;
            drop(cache);
            return Ok(receipt(
                "confirmed",
                "Snapshot scope read back from the mixer.",
                self.native_inventory(),
            ));
        }
        let name = if op == "rename" {
            let name = action["name"]
                .as_str()
                .ok_or_else(|| anyhow::anyhow!("Missing snapshot name"))?;
            openfader_core::native_library::validate_name(name)?;
            anyhow::ensure!(
                name.len() <= 31,
                "XR18 snapshot names are limited to 31 bytes"
            );
            Some(name)
        } else {
            None
        };
        anyhow::ensure!(
            matches!(op, "recall" | "replace" | "rename" | "delete"),
            "Unsupported snapshot operation"
        );
        anyhow::ensure!(
            valid() && self.native_inventory() == *expected,
            "Native session or inventory changed"
        );
        if op == "replace" {
            let sent = session
                .send("/-snap/name", vec![OscType::String(previous.to_string())])
                .await;
            if sent.is_err() || !valid() {
                let mut cache = self.native.lock().unwrap();
                cache.names = None;
                cache.revision += 1;
                drop(cache);
                return Ok(receipt("uncertain", "Snapshot name setup may have been sent; save was not dispatched. Refresh first.", self.native_inventory()));
            }
        }
        let (path, args) = match op {
            "recall" => ("/-snap/load".to_string(), vec![OscType::Int(slot as i32)]),
            "replace" => ("/-snap/save".to_string(), vec![OscType::Int(slot as i32)]),
            "delete" => ("/-snap/delete".to_string(), vec![OscType::Int(slot as i32)]),
            _ => (
                format!("/-snap/{slot:02}/name"),
                vec![OscType::String(name.unwrap().to_string())],
            ),
        };
        // Once dispatch is attempted UDP cannot establish whether the desk applied it.
        let sent = session.send(&path, args).await;
        {
            let mut cache = self.native.lock().unwrap();
            cache.names = None;
            cache.revision += 1;
        }
        let message = match sent {
            Ok(()) => {
                "Command sent; the mixer result is unconfirmed. Refresh before another operation."
                    .to_string()
            }
            Err(e) => {
                format!("Command delivery is uncertain: {e}. Refresh before another operation.")
            }
        };
        Ok(receipt("uncertain", message, self.native_inventory()))
    }
}

async fn read_name(session: &OscSession, slot: usize) -> anyhow::Result<String> {
    let msg = session
        .query_timeout(
            &format!("/-snap/{slot:02}/name"),
            Duration::from_millis(500),
        )
        .await?;
    match msg.args.as_slice() {
        [OscType::String(name)] => Ok(name.clone()),
        _ => anyhow::bail!("XR18 snapshot slot {slot} returned an invalid name"),
    }
}

fn valid_scope(scope: &str) -> bool {
    scope.len() == 59 && scope.bytes().all(|b| matches!(b, b'+' | b'-'))
}
async fn read_scope(session: &OscSession, slot: usize) -> anyhow::Result<String> {
    let msg = session
        .query_timeout(
            &format!("/-snap/{slot:02}/scope"),
            Duration::from_millis(500),
        )
        .await?;
    match msg.args.as_slice() {
        [OscType::String(s)] if valid_scope(s) => Ok(s.clone()),
        _ => anyhow::bail!("Snapshot slot {slot} scope invalid"),
    }
}
async fn read_index(session: &OscSession) -> anyhow::Result<usize> {
    let msg = session
        .query_timeout("/-snap/index", Duration::from_millis(500))
        .await?;
    match msg.args.as_slice() {
        [OscType::Int(i @ 1..=64)] => Ok(*i as usize),
        _ => anyhow::bail!("Snapshot index unavailable"),
    }
}
