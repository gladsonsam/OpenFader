//! Explicit X32 actions with native preconditions and honest receipts.
use super::presentation::receipt;
use crate::{X32Driver, QUERY_TIMEOUT};
use openfader_core::console_controls::{
    ConsoleControl, ConsoleControlChoice, ConsoleControlKind, ConsoleControlResult,
    ConsoleControlStatus as Status, ConsoleControlWrite,
};
use openfader_core::native_library::Check;
use openfader_transport::OscSession;
use rosc::OscType;
use std::sync::{atomic::Ordering, Arc};

const SCOPE: &str = "/config/dp48/scope";
const BROADCAST: &str = "/config/dp48/broadcast";

async fn scope(session: &Arc<OscSession>) -> anyhow::Result<i32> {
    let reply = session.query_timeout(SCOPE, QUERY_TIMEOUT).await?;
    let value = crate::mapping::strict_int(&reply)?;
    anyhow::ensure!(
        (0..=15).contains(&value) && (value & 12 == 0 || value & 2 != 0),
        "x32: invalid DP48 scope"
    );
    Ok(value)
}

pub(super) async fn broadcast_descriptor(
    driver: &X32Driver,
    session: &Arc<OscSession>,
    generation: u64,
) -> ConsoleControl {
    let epoch = driver.console_epoch.load(Ordering::Acquire);
    let result = scope(session).await.and_then(|value| {
        driver.ensure_current(session)?;
        anyhow::ensure!(
            driver.console_epoch.load(Ordering::Acquire) == epoch,
            "console controls changed during observation"
        );
        Ok(value)
    });
    let (revision, unavailable) = match result {
        Ok(value) => (
            serde_json::json!({"session": generation, "browser": epoch, "id": "dp48.broadcast", "scope": value}),
            None,
        ),
        Err(e) => (
            serde_json::Value::Null,
            Some(format!("DP48 scope is not observed: {e}")),
        ),
    };
    ConsoleControl {
        path: "/dp48.broadcast".into(),
        label: "Broadcast DP48 scope".into(),
        kind: ConsoleControlKind::Action,
        choices: vec![ConsoleControlChoice {
            label: "Broadcast".into(),
            value: serde_json::json!("broadcast"),
        }],
        read_only: unavailable.is_some(),
        unavailable_reason: unavailable,
        revision,
        ..Default::default()
    }
}

pub(super) async fn broadcast(
    driver: &X32Driver,
    request: &ConsoleControlWrite,
    check: Check<'_>,
) -> anyhow::Result<ConsoleControlResult> {
    if request.value.as_str() != Some("broadcast") {
        return Ok(receipt(
            Status::Rejected,
            None,
            Some("Invalid DP48 action".into()),
        ));
    }
    let session = driver.session()?;
    let generation = driver.route_generation_for(&session)?;
    let epoch = driver.console_epoch.load(Ordering::Acquire);
    let revision = &request.revision;
    if revision.get("id").and_then(|v| v.as_str()) != Some("dp48.broadcast")
        || revision.get("session").and_then(|v| v.as_u64()) != Some(generation)
        || revision.get("browser").and_then(|v| v.as_u64()) != Some(epoch)
    {
        return Ok(receipt(
            Status::Rejected,
            None,
            Some("Stale DP48 action revision".into()),
        ));
    }
    let expected = match revision
        .get("scope")
        .and_then(|v| v.as_i64())
        .and_then(|v| i32::try_from(v).ok())
    {
        Some(v) => v,
        None => {
            return Ok(receipt(
                Status::Rejected,
                None,
                Some("Missing DP48 scope guard".into()),
            ))
        }
    };
    let _lane = driver.writes.lock().await;
    driver.ensure_current(&session)?;
    if !check() {
        return Ok(receipt(
            Status::Rejected,
            None,
            Some("Console control authority expired".into()),
        ));
    }
    let observed = match scope(&session).await {
        Ok(v) => v,
        Err(e) => {
            return Ok(receipt(
                Status::Rejected,
                None,
                Some(format!("DP48 scope unavailable: {e}")),
            ))
        }
    };
    driver.ensure_current(&session)?;
    if !check() || observed != expected || driver.console_epoch.load(Ordering::Acquire) != epoch {
        return Ok(receipt(
            Status::Rejected,
            None,
            Some("DP48 scope or authority changed before broadcast".into()),
        ));
    }
    let next = epoch
        .checked_add(1)
        .ok_or_else(|| anyhow::anyhow!("x32: console revision exhausted"))?;
    driver.console_epoch.store(next, Ordering::Release);
    driver.retire_console_attempt(&session, generation)?;
    let mut received = session.subscribe_received_updates();
    if !check() {
        return Ok(receipt(
            Status::Rejected,
            None,
            Some("Console control authority expired before broadcast".into()),
        ));
    }
    let started = std::time::Instant::now();
    let sent = session.send(BROADCAST, vec![OscType::Int(1)]).await;
    let retired = driver.retire_console_attempt(&session, generation);
    if let Err(e) = retired {
        return Ok(receipt(
            Status::Uncertain,
            None,
            Some(format!(
                "Broadcast attempted but proof retirement failed: {e}"
            )),
        ));
    }
    if let Err(e) = sent {
        return Ok(receipt(
            Status::Uncertain,
            None,
            Some(format!("Broadcast send was not confirmed: {e}")),
        ));
    }
    let acknowledged = tokio::time::timeout(QUERY_TIMEOUT, async {
        loop {
            let (at, message) = received.recv().await?;
            if at >= started
                && message.addr == BROADCAST
                && crate::mapping::strict_int(&message).ok() == Some(0)
            {
                return Ok::<_, tokio::sync::broadcast::error::RecvError>(());
            }
        }
    })
    .await;
    if driver.ensure_current(&session).is_err() || !check() {
        return Ok(receipt(
            Status::Uncertain,
            None,
            Some("Session or authority changed after broadcast".into()),
        ));
    }
    match acknowledged {
        Ok(Ok(())) => Ok(receipt(Status::Accepted, None, Some("Desk acknowledged the broadcast; connected DP48 devices were not independently verified.".into()))),
        _ => Ok(receipt(Status::Uncertain, None, Some("No correlated broadcast acknowledgement was observed.".into()))),
    }
}
