//! Typed X32 console controls and guarded live writes.
mod actions;
mod browse;
mod card;
mod codecs;
mod dp48;
mod iq;
mod observations;
mod presentation;
mod recorder;
mod specs;
mod user_assign;
use crate::{X32Driver, QUERY_TIMEOUT};
use codecs::{agrees, encode, mapping, parse_observed_value, parse_value};
use presentation::{descriptor, receipt};
use specs::control;
async fn observed_model(
    session: &std::sync::Arc<openfader_transport::OscSession>,
    row: &specs::ConsoleControl,
) -> anyhow::Result<i32> {
    match row.kind {
        specs::ConsoleControlKind::CardChoice { .. } => card::observed_type(session).await,
        specs::ConsoleControlKind::IQSound { model_path } => {
            iq::observed_model(session, model_path).await
        }
        _ => anyhow::bail!("x32: control has no model guard"),
    }
}
fn bind_model(
    row: &specs::ConsoleControl,
    item: openfader_core::console_controls::ConsoleControl,
    model: i32,
) -> openfader_core::console_controls::ConsoleControl {
    match row.kind {
        specs::ConsoleControlKind::IQSound { .. } => iq::bind_model(item, model),
        _ => card::bind_model(item, model),
    }
}
impl X32Driver {
    fn retire_console_attempt(
        &self,
        session: &std::sync::Arc<openfader_transport::OscSession>,
        generation: u64,
    ) -> anyhow::Result<()> {
        crate::routing::retire_console_proof(generation)?;
        let link = self
            .link
            .read()
            .map_err(|_| anyhow::anyhow!("x32: session unavailable"))?;
        let current = link
            .as_ref()
            .filter(|link| std::sync::Arc::ptr_eq(&link.session, session))
            .ok_or_else(|| anyhow::anyhow!("x32: console session changed"))?;
        current
            .route_refresh
            .send(crate::routing::RouteRefresh::All)
            .map_err(|_| anyhow::anyhow!("x32: route maintenance unavailable"))?;
        current
            .solo_refresh
            .send(crate::solo::SoloRefresh::All)
            .map_err(|_| anyhow::anyhow!("x32: solo maintenance unavailable"))?;
        Ok(())
    }
    pub async fn console_write(
        &self,
        request: &openfader_core::console_controls::ConsoleControlWrite,
        check: openfader_core::native_library::Check<'_>,
    ) -> anyhow::Result<openfader_core::console_controls::ConsoleControlResult> {
        use openfader_core::console_controls::ConsoleControlStatus as Status;
        if request.path == "/card.live_transport" {
            return recorder::transport(self, request, check).await;
        }
        if request.path == "/dp48.broadcast" {
            return actions::broadcast(self, request, check).await;
        }
        let row = match control(&request.path) {
            Ok(row) => row,
            Err(e) => return Ok(receipt(Status::Rejected, None, Some(e.to_string()))),
        };
        let desired = match parse_value(row, &request.value).and_then(|v| encode(row, &v)) {
            Ok(v) => v,
            Err(e) => return Ok(receipt(Status::Rejected, None, Some(e.to_string()))),
        };
        let session = self.session()?;
        let generation = self.route_generation_for(&session)?;
        let browser_epoch = self
            .console_epoch
            .load(std::sync::atomic::Ordering::Acquire);
        let eq_path = if matches!(row.kind, specs::ConsoleControlKind::IQEq(_)) {
            let lower = row.path;
            let upper = row.path.replace("/iQeqset", "/IQeqset");
            match request.revision.get("native_path").and_then(|v| v.as_str()) {
                Some(path) if path == lower || path == upper => Some(path.to_owned()),
                _ => {
                    return Ok(receipt(
                        Status::Rejected,
                        None,
                        Some("Missing or invalid observed iQ EQ path".into()),
                    ))
                }
            }
        } else {
            None
        };
        let guard_required = matches!(
            row.kind,
            specs::ConsoleControlKind::CardChoice { .. }
                | specs::ConsoleControlKind::IQSound { .. }
        );
        let expected_model = if guard_required {
            let model = request
                .revision
                .get("model")
                .and_then(|v| v.as_i64())
                .and_then(|v| i32::try_from(v).ok());
            let valid = match (row.kind, model) {
                (specs::ConsoleControlKind::CardChoice { card_types, .. }, Some(model)) => {
                    card_types.contains(&model)
                }
                (specs::ConsoleControlKind::IQSound { .. }, Some(model)) => iq::profiles(model)
                    .is_some_and(|values| {
                        request
                            .value
                            .as_u64()
                            .is_some_and(|index| index < values.len() as u64)
                    }),
                _ => false,
            };
            if !valid {
                return Ok(receipt(
                    Status::Rejected,
                    None,
                    Some("Missing, stale or incompatible model guard".into()),
                ));
            }
            model
        } else {
            None
        };
        let expected = match request
            .revision
            .get("value")
            .and_then(|v| parse_observed_value(row, v).ok())
        {
            Some(v) => v,
            None => {
                return Ok(receipt(
                    Status::Rejected,
                    None,
                    Some("Missing or invalid observed revision".into()),
                ))
            }
        };
        if request.revision.get("session").and_then(|v| v.as_u64()) != Some(generation)
            || request.revision.get("browser").and_then(|v| v.as_u64()) != Some(browser_epoch)
            || request.revision.get("id").and_then(|v| v.as_str()) != Some(row.id)
        {
            return Ok(receipt(
                Status::Rejected,
                None,
                Some("Stale console control revision".into()),
            ));
        }
        let mut m = mapping(row);
        if let Some(path) = &eq_path {
            m.path.clone_from(path);
        }
        let _guard = self.writes.lock().await;
        self.ensure_current(&session)?;
        if !check() {
            return Ok(receipt(
                Status::Rejected,
                None,
                Some("Console control authority expired".into()),
            ));
        }
        if let Some(path) = &eq_path {
            if iq::resolve_eq_path(&session, row).await.ok().map(|v| v.0) != Some(path.clone()) {
                return Ok(receipt(
                    Status::Rejected,
                    None,
                    Some("Observed iQ EQ path changed or became ambiguous".into()),
                ));
            }
            self.ensure_current(&session)?;
            if !check() {
                return Ok(receipt(
                    Status::Rejected,
                    None,
                    Some("Console control authority expired before write".into()),
                ));
            }
        }
        if let Some(expected_model) = expected_model {
            if observed_model(&session, row).await.ok() != Some(expected_model) {
                return Ok(receipt(
                    Status::Rejected,
                    None,
                    Some("Control model changed or is unavailable".into()),
                ));
            }
            self.ensure_current(&session)?;
            if !check() {
                return Ok(receipt(
                    Status::Rejected,
                    None,
                    Some("Console control authority expired before write".into()),
                ));
            }
        }
        let actual = match session
            .query_timeout(&m.path, QUERY_TIMEOUT)
            .await
            .and_then(|reply| m.decode(&reply))
        {
            Ok(v) => v,
            Err(e) => {
                return Ok(receipt(
                    Status::Rejected,
                    None,
                    Some(format!("Could not revalidate current value: {e}")),
                ))
            }
        };
        self.ensure_current(&session)?;
        if !check() {
            return Ok(receipt(
                Status::Rejected,
                None,
                Some("Console control authority expired before write".into()),
            ));
        }
        if actual != expected {
            let item = descriptor(row, generation, browser_epoch, Some(actual), None);
            let item = if let Some(path) = &eq_path {
                iq::bind_eq(item, path)
            } else {
                item
            };
            return Ok(receipt(
                Status::Rejected,
                Some(if let Some(model) = expected_model {
                    bind_model(row, item, model)
                } else {
                    item
                }),
                Some("Control changed; refresh before editing".into()),
            ));
        }
        if let Some(model) = expected_model {
            if observed_model(&session, row).await.ok() != Some(model) {
                return Ok(receipt(
                    Status::Rejected,
                    None,
                    Some("Control model changed before write".into()),
                ));
            }
            self.ensure_current(&session)?;
            if !check() {
                return Ok(receipt(
                    Status::Rejected,
                    None,
                    Some("Console control authority expired before write".into()),
                ));
            }
        }
        if let Some(path) = &eq_path {
            if iq::resolve_eq_path(&session, row).await.ok().map(|v| v.0) != Some(path.clone()) {
                return Ok(receipt(
                    Status::Rejected,
                    None,
                    Some("Observed iQ EQ path changed before write".into()),
                ));
            }
            self.ensure_current(&session)?;
            if !check() {
                return Ok(receipt(
                    Status::Rejected,
                    None,
                    Some("Console control authority expired before write".into()),
                ));
            }
        }
        if self
            .console_epoch
            .load(std::sync::atomic::Ordering::Acquire)
            != browser_epoch
        {
            return Ok(receipt(
                Status::Rejected,
                None,
                Some("Console controls changed; refresh before editing".into()),
            ));
        }
        // Console writes share `writes`, so this checked increment has one writer.
        let next_epoch = browser_epoch
            .checked_add(1)
            .ok_or_else(|| anyhow::anyhow!("x32: console revision exhausted"))?;
        self.console_epoch
            .store(next_epoch, std::sync::atomic::Ordering::Release);
        self.retire_console_attempt(&session, generation)?;
        let sent = session.send(&m.path, vec![desired.0]).await;
        let retired = self.retire_console_attempt(&session, generation);
        if let Err(e) = retired {
            return Ok(receipt(
                Status::Uncertain,
                None,
                Some(format!(
                    "Write attempted but provider proof retirement failed: {e}"
                )),
            ));
        }
        if let Err(e) = sent {
            return Ok(receipt(
                Status::Uncertain,
                None,
                Some(format!("Write could not be confirmed: {e}")),
            ));
        }
        let read = match session
            .query_timeout(&m.path, QUERY_TIMEOUT)
            .await
            .and_then(|reply| m.decode(&reply))
        {
            Ok(value) => value,
            Err(e) => {
                return Ok(receipt(
                    Status::Uncertain,
                    None,
                    Some(format!("Write readback unavailable: {e}")),
                ))
            }
        };
        if let Err(e) = self.ensure_current(&session) {
            return Ok(receipt(
                Status::Uncertain,
                None,
                Some(format!("Connection changed after write: {e}")),
            ));
        }
        if !check() {
            return Ok(receipt(
                Status::Uncertain,
                None,
                Some("Console control authority expired after write".into()),
            ));
        }
        if let Some(model) = expected_model {
            if observed_model(&session, row).await.ok() != Some(model)
                || self.ensure_current(&session).is_err()
                || !check()
            {
                return Ok(receipt(
                    Status::Uncertain,
                    None,
                    Some("Control model or authority changed after write".into()),
                ));
            }
        }
        if let Some(path) = &eq_path {
            if iq::resolve_eq_path(&session, row).await.ok().map(|v| v.0) != Some(path.clone())
                || self.ensure_current(&session).is_err()
                || !check()
            {
                return Ok(receipt(
                    Status::Uncertain,
                    None,
                    Some("Observed iQ EQ path or authority changed after write".into()),
                ));
            }
        }
        if !agrees(row, &desired.1, &read).unwrap_or(false) {
            let item = descriptor(row, generation, next_epoch, Some(read), None);
            let item = if let Some(path) = &eq_path {
                iq::bind_eq(item, path)
            } else {
                item
            };
            return Ok(receipt(
                Status::Uncertain,
                Some(if let Some(model) = expected_model {
                    bind_model(row, item, model)
                } else {
                    item
                }),
                Some("Write readback differs from the requested value".into()),
            ));
        }
        let item = descriptor(row, generation, next_epoch, Some(read), None);
        let item = if let Some(path) = &eq_path {
            iq::bind_eq(item, path)
        } else {
            item
        };
        Ok(receipt(
            Status::Verified,
            Some(if let Some(model) = expected_model {
                bind_model(row, item, model)
            } else {
                item
            }),
            None,
        ))
    }
}
