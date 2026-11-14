//! Guarded X-LIVE playback transport. Recorder media writes stay unavailable
//! until a native media-presence contract can be verified independently.
use super::{card, presentation::receipt};
use crate::{X32Driver, QUERY_TIMEOUT};
use openfader_core::{
    console_controls::{
        ConsoleControl, ConsoleControlChoice, ConsoleControlKind, ConsoleControlResult,
        ConsoleControlStatus as Status, ConsoleControlWrite,
    },
    native_library::Check,
};
use openfader_transport::OscSession;
use rosc::OscType;
use std::sync::{atomic::Ordering, Arc};

const STATE: &str = "/-stat/urec/state";
const SESSIONS: &str = "/-urec/sessionmax";
async fn integer(session: &Arc<OscSession>, path: &str) -> anyhow::Result<i32> {
    let reply = session.query_timeout(path, QUERY_TIMEOUT).await?;
    crate::mapping::strict_int(&reply)
}
async fn observed(session: &Arc<OscSession>) -> anyhow::Result<(i32, Option<i32>)> {
    anyhow::ensure!(
        card::observed_type(session).await? == 10,
        "X-LIVE card is not observed"
    );
    let state = integer(session, STATE).await?;
    anyhow::ensure!((0..=3).contains(&state), "unrecognized X-LIVE state");
    let sessions = integer(session, SESSIONS)
        .await
        .ok()
        .filter(|v| (0..=100).contains(v));
    anyhow::ensure!(
        card::observed_type(session).await? == 10,
        "X-LIVE card changed during observation"
    );
    Ok((state, sessions))
}

pub(super) async fn descriptor(
    driver: &X32Driver,
    session: &Arc<OscSession>,
    generation: u64,
) -> ConsoleControl {
    let epoch = driver.console_epoch.load(Ordering::Acquire);
    let observed = observed(session).await.and_then(|value| {
        driver.ensure_current(session)?;
        anyhow::ensure!(
            driver.console_epoch.load(Ordering::Acquire) == epoch,
            "console controls changed during observation"
        );
        Ok(value)
    });
    let mut item = ConsoleControl {
        path: "/card.live_transport".into(),
        label: "X-LIVE playback transport".into(),
        kind: ConsoleControlKind::Action,
        ..Default::default()
    };
    match observed {
        Ok((state, sessions)) => {
            if state != 0 {
                item.choices.push(ConsoleControlChoice {
                    label: "Stop".into(),
                    value: serde_json::json!("stop"),
                });
            }
            if state == 2 {
                item.choices.push(ConsoleControlChoice {
                    label: "Pause playback".into(),
                    value: serde_json::json!("pause"),
                });
            }
            if matches!(state, 0 | 1) && sessions.is_some_and(|n| n > 0) {
                item.choices.push(ConsoleControlChoice {
                    label: "Play".into(),
                    value: serde_json::json!("play"),
                });
            }
            item.value = Some(serde_json::json!(state));
            item.model = Some(serde_json::json!(10));
            item.revision = serde_json::json!({"session": generation, "browser": epoch, "id": "card.live_transport", "model": 10, "state": state, "sessions": sessions});
            item.read_only = item.choices.is_empty();
            if item.read_only {
                item.unavailable_reason =
                    Some("No playback session is independently available.".into());
            }
        }
        Err(e) => {
            item.unavailable_reason = Some(format!("X-LIVE transport unavailable: {e}"));
        }
    }
    item
}

pub(super) async fn transport(
    driver: &X32Driver,
    request: &ConsoleControlWrite,
    check: Check<'_>,
) -> anyhow::Result<ConsoleControlResult> {
    let target = match request.value.as_str() {
        Some("stop") => 0,
        Some("pause") => 1,
        Some("play") => 2,
        _ => {
            return Ok(receipt(
                Status::Rejected,
                None,
                Some("Invalid playback action".into()),
            ))
        }
    };
    let session = driver.session()?;
    let generation = driver.route_generation_for(&session)?;
    let epoch = driver.console_epoch.load(Ordering::Acquire);
    let revision = &request.revision;
    if revision.get("id").and_then(|v| v.as_str()) != Some("card.live_transport")
        || revision.get("session").and_then(|v| v.as_u64()) != Some(generation)
        || revision.get("browser").and_then(|v| v.as_u64()) != Some(epoch)
        || revision.get("model").and_then(|v| v.as_i64()) != Some(10)
    {
        return Ok(receipt(
            Status::Rejected,
            None,
            Some("Stale X-LIVE transport revision".into()),
        ));
    }
    let expected_state = revision
        .get("state")
        .and_then(|v| v.as_i64())
        .and_then(|v| i32::try_from(v).ok());
    let expected_sessions = revision
        .get("sessions")
        .and_then(|v| v.as_i64())
        .and_then(|v| i32::try_from(v).ok());
    let _lane = driver.writes.lock().await;
    driver.ensure_current(&session)?;
    if !check() {
        return Ok(receipt(
            Status::Rejected,
            None,
            Some("Console control authority expired".into()),
        ));
    }
    let (state, sessions) = match observed(&session).await {
        Ok(value) => value,
        Err(e) => {
            return Ok(receipt(
                Status::Rejected,
                None,
                Some(format!("X-LIVE unavailable: {e}")),
            ))
        }
    };
    driver.ensure_current(&session)?;
    let allowed = match target {
        0 => state != 0,
        1 => state == 2,
        2 => matches!(state, 0 | 1) && sessions.is_some_and(|n| n > 0),
        _ => false,
    };
    if !check()
        || !allowed
        || expected_state != Some(state)
        || expected_sessions != sessions
        || driver.console_epoch.load(Ordering::Acquire) != epoch
    {
        return Ok(receipt(
            Status::Rejected,
            None,
            Some("X-LIVE state, session list or authority changed".into()),
        ));
    }
    let next = epoch
        .checked_add(1)
        .ok_or_else(|| anyhow::anyhow!("x32: console revision exhausted"))?;
    driver.console_epoch.store(next, Ordering::Release);
    driver.retire_console_attempt(&session, generation)?;
    if !check() {
        return Ok(receipt(
            Status::Rejected,
            None,
            Some("Console control authority expired before playback action".into()),
        ));
    }
    let sent = session.send(STATE, vec![OscType::Int(target)]).await;
    let retired = driver.retire_console_attempt(&session, generation);
    if let Err(e) = retired {
        return Ok(receipt(
            Status::Uncertain,
            None,
            Some(format!(
                "Playback action attempted but proof retirement failed: {e}"
            )),
        ));
    }
    if let Err(e) = sent {
        return Ok(receipt(
            Status::Uncertain,
            None,
            Some(format!("Playback action not confirmed: {e}")),
        ));
    }
    let read = observed(&session).await;
    if driver.ensure_current(&session).is_err() || !check() {
        return Ok(receipt(
            Status::Uncertain,
            None,
            Some("Session or authority changed after playback action".into()),
        ));
    }
    match read {
        Ok((state, _)) if state == target => Ok(receipt(
            Status::Verified,
            None,
            Some("X-LIVE transport state was read back; audio output was not verified.".into()),
        )),
        Ok((state, _)) => Ok(receipt(
            Status::Uncertain,
            None,
            Some(format!("X-LIVE reported state {state} after the action")),
        )),
        Err(e) => Ok(receipt(
            Status::Uncertain,
            None,
            Some(format!("X-LIVE state unavailable after action: {e}")),
        )),
    }
}
