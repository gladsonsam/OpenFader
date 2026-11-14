//! Read-only WING open-show inventory. Native files remain filesystem-owned.
use crate::{WingDriver, QUERY_TIMEOUT};
use anyhow::{ensure, Context};
use openfader_core::native_library::{receipt, Check};
use rosc::OscType;
use serde_json::{json, Value};
use std::sync::atomic::Ordering;

#[derive(Default)]
pub(crate) struct State {
    generation: u64,
    observed_native_revision: u64,
    revision: u64,
    complete: bool,
    show: String,
    scenes: Vec<String>,
    error: Option<String>,
}
impl WingDriver {
    pub(crate) fn show_inventory(&self) -> Value {
        let state = self.show_library.lock().unwrap();
        let generation = self.generation.load(Ordering::Acquire);
        let connected = self.link.read().unwrap().is_some();
        let valid = connected
            && state.generation == generation
            && state.observed_native_revision == self.native_revision.load(Ordering::Acquire)
            && state.complete;
        let rows: Vec<Value> = if valid {
            state
                .scenes
                .iter()
                .enumerate()
                .map(|(i, name)| json!({"slot":i+1,"name":name}))
                .collect()
        } else {
            vec![]
        };
        let count = if valid { state.scenes.len() } else { 0 };
        let capacity_label = if valid {
            format!("{count} observed scenes")
        } else {
            "Scene count unavailable".to_string()
        };
        json!({"authority":"mixer","provider":"wing","label":"WING open show","connected":connected,"generation":generation,"revision":state.revision,"complete":valid,
            "capacity":count.max(1),"capacity_label":capacity_label,"slots":rows,"operations":["inspect","refresh"],
            "banks": if valid && !state.scenes.is_empty() {vec![json!({"id":"scenes","title":"Open show scenes","row_key":"slot","slot_min":1,"slot_max":count,"rows":rows})]} else {vec![]},
            "headers":if valid {vec![format!("Open show: {}",state.show)]} else {vec![]},
            "banks_note":state.error.as_deref(),"refresh_title":"Read open show?","refresh_description":"Read the opened show's scene list from the mixer.",
            "limitations":"Show files and presets are stored on the console filesystem. File operations are unavailable over this interface; scene recall is available in native console controls."})
    }
    pub(crate) async fn show_library_action(
        &self,
        action: &Value,
        expected: &Value,
        check: Check<'_>,
    ) -> anyhow::Result<Value> {
        ensure!(
            action.get("op").and_then(Value::as_str) == Some("refresh"),
            "WING: unsupported native library operation"
        );
        ensure!(
            check() && &self.show_inventory() == expected,
            "WING: native library session or inventory changed"
        );
        let session = self.session()?;
        let generation = self.generation.load(Ordering::Acquire);
        let revision = self.native_revision.load(Ordering::Acquire);
        let result = async {
            let show = session
                .query_timeout("/$ctl/lib/$actshow", QUERY_TIMEOUT)
                .await?;
            let [OscType::String(show)] = show.args.as_slice() else {
                anyhow::bail!("invalid open show response")
            };
            ensure!(
                !show.is_empty(),
                "Open a show on the console before reading its scene list"
            );
            let msg = session
                .query_with_args_timeout(
                    "/$ctl/lib",
                    vec![OscType::String("?".into())],
                    QUERY_TIMEOUT,
                    |m| matches!(m.args.as_slice(), [OscType::String(_)]),
                )
                .await?;
            let [OscType::String(text)] = msg.args.as_slice() else {
                unreachable!()
            };
            // The protocol specifies the node description request for the full
            // list. Only a complete, explicit list is accepted; never infer an
            // empty show from a missing or unrecognised response.
            let mut rows = text.lines().filter_map(|line| {
                let rest = line
                    .trim_matches(|c: char| c.is_ascii_whitespace())
                    .strip_prefix("$scenes")?;
                rest.starts_with(|c: char| c.is_ascii_whitespace())
                    .then_some(rest)
            });
            let row = rows
                .next()
                .context("console did not return a complete scene list")?;
            ensure!(rows.next().is_none(), "ambiguous scene list");
            let rest = row
                .trim_start_matches(|c: char| c.is_ascii_whitespace())
                .strip_prefix("list")
                .context("unsupported scene list format")?;
            ensure!(
                rest.starts_with(|c: char| c.is_ascii_whitespace()),
                "unsupported scene list format"
            );
            let list = rest
                .trim_matches(|c: char| c.is_ascii_whitespace())
                .strip_prefix('[')
                .and_then(|v| v.strip_suffix(']'))
                .context("incomplete scene list")?;
            let scenes: Vec<String> = if list.is_empty() {
                vec![]
            } else {
                list.split(',').map(|s| s.trim().to_owned()).collect()
            };
            ensure!(
                scenes.len() <= 16384
                    && scenes.iter().all(|s| !s.is_empty()
                        && s.len() <= 256
                        && !s.contains(['[', ']', '\'', '"'])),
                "unsupported or ambiguous scene list"
            );
            let again = session
                .query_timeout("/$ctl/lib/$actshow", QUERY_TIMEOUT)
                .await?;
            ensure!(
                again.args.as_slice() == [OscType::String(show.clone())],
                "open show changed during refresh"
            );
            self.ensure_current(&session)?;
            ensure!(
                check()
                    && self.generation.load(Ordering::Acquire) == generation
                    && self.native_revision.load(Ordering::Acquire) == revision,
                "session or native state changed during refresh"
            );
            Ok::<_, anyhow::Error>((show.clone(), scenes))
        }
        .await;
        let mut state = self.show_library.lock().unwrap();
        state.revision = state.revision.wrapping_add(1);
        state.generation = generation;
        state.observed_native_revision = revision;
        match result {
            Ok((show, scenes)) => {
                state.show = show;
                state.scenes = scenes;
                state.complete = true;
                state.error = None;
            }
            Err(error) => {
                state.show.clear();
                state.scenes.clear();
                state.complete = false;
                state.error = Some(format!("{error:#}"));
            }
        }
        let complete = state.complete;
        let message = state
            .error
            .clone()
            .unwrap_or_else(|| "Open show scene list read from the mixer.".into());
        drop(state);
        Ok(receipt(
            if complete { "confirmed" } else { "uncertain" },
            message,
            self.show_inventory(),
        ))
    }
}
