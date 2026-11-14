//! Lazy, live native definitions. No node path is writable without a fresh DD definition.
use crate::{
    console_metadata::{
        child, descriptor, encode, model_path, readback_agrees, valid_path, value_of,
    },
    native::{self, Definition, Kind},
    WingDriver, QUERY_TIMEOUT,
};
use anyhow::{ensure, Context};
use openfader_core::{
    console_controls::{
        ConsoleControl, ConsoleControlKind as CK, ConsoleControlNode, ConsoleControlResult,
        ConsoleControlStatus as Status, ConsoleControlWrite,
    },
    native_library::Check,
};
use openfader_transport::OscSession;
use rosc::OscType;
use serde_json::{json, Value};
use std::sync::{atomic::Ordering, Arc};

impl WingDriver {
    pub(crate) async fn native_defs(
        &self,
        session: &Arc<OscSession>,
        path: &str,
    ) -> anyhow::Result<Vec<Definition>> {
        let msg = session
            .query_with_args_timeout(path, vec![OscType::Blob(vec![0xdd])], QUERY_TIMEOUT, |m| {
                matches!(m.args.as_slice(), [OscType::Blob(_)])
            })
            .await?;
        self.ensure_current(session)?;
        let [OscType::Blob(data)] = msg.args.as_slice() else {
            unreachable!()
        };
        native::addressed_definitions(data)
    }
    async fn current_model(
        &self,
        session: &Arc<OscSession>,
        path: &str,
    ) -> anyhow::Result<Option<String>> {
        let Some(path) = model_path(path) else {
            return Ok(None);
        };
        let msg = session.query_timeout(&path, QUERY_TIMEOUT).await?;
        self.ensure_current(session)?;
        let [OscType::String(s)] = msg.args.as_slice() else {
            anyhow::bail!("invalid model selector")
        };
        Ok(Some(s.clone()))
    }
    async fn action_context(
        &self,
        session: &Arc<OscSession>,
        path: &str,
    ) -> anyhow::Result<Option<Value>> {
        let paths: &[&str] = match path {
            "/$ctl/lib/$action" => &[
                "/$ctl/lib/$actshow",
                "/$ctl/lib/$actionidx",
                "/$ctl/lib/$actidx",
            ],
            "/$ctl/lib/$actionidx" => &["/$ctl/lib/$actshow"],
            "/play/$action" => &["/play/$actstate", "/play/$actlist", "/play/$actfile"],
            "/rec/$action" => &["/rec/$actstate", "/rec/path", "/rec/$actfile"],
            _ => return Ok(None),
        };
        let parent = path.rsplit_once('/').unwrap().0;
        let defs = self.native_defs(session, parent).await?;
        let mut context = serde_json::Map::new();
        for p in paths {
            let name = p.rsplit('/').next().unwrap();
            let def = defs
                .iter()
                .find(|d| d.address_name().is_ok_and(|child| child == name))
                .context("action state definition unavailable")?;
            let msg = session.query_timeout(p, QUERY_TIMEOUT).await?;
            self.ensure_current(session)?;
            context.insert(name.into(), value_of(def, &msg)?);
        }
        Ok(Some(Value::Object(context)))
    }
    async fn route_context(
        &self,
        session: &Arc<OscSession>,
        path: &str,
    ) -> anyhow::Result<Option<String>> {
        if !(path.starts_with("/io/out/") || path.contains("/in/conn/")) {
            return Ok(None);
        }
        let (parent, name) = path.rsplit_once('/').context("invalid route path")?;
        let sibling = match name {
            "grp" => "in",
            "in" => "grp",
            "altgrp" => "altin",
            "altin" => "altgrp",
            _ => return Ok(None),
        };
        let msg = session
            .query_timeout(&format!("{parent}/{sibling}"), QUERY_TIMEOUT)
            .await?;
        self.ensure_current(session)?;
        Ok(Some(format!("{:?}", msg.args)))
    }
    async fn observed_control(
        &self,
        session: &Arc<OscSession>,
        generation: u64,
        path: &str,
        def: &Definition,
    ) -> anyhow::Result<ConsoleControl> {
        let mut c = descriptor(path.into(), def);
        let revision = self.native_revision.load(Ordering::Acquire);
        if matches!(def.kind, Kind::Node) {
            return Ok(c);
        }
        let model = self.current_model(session, path).await?;
        let context = self.route_context(session, path).await?;
        let action_context = self.action_context(session, path).await?;
        let msg = session.query_timeout(path, QUERY_TIMEOUT).await?;
        self.ensure_current(session)?;
        let value = value_of(def, &msg)?;
        ensure!(
            self.current_model(session, path).await? == model,
            "model changed during native read"
        );
        ensure!(
            self.route_context(session, path).await? == context,
            "route context changed during native read"
        );
        if matches!(path, "/$ctl/lib/$action" | "/play/$action" | "/rec/$action") {
            let ctx = action_context
                .as_ref()
                .and_then(Value::as_object)
                .context("action context unavailable")?;
            let ready = match path {
                "/$ctl/lib/$action" => {
                    ctx.get("$actshow")
                        .and_then(Value::as_str)
                        .is_some_and(|v| !v.is_empty())
                        && ctx
                            .get("$actionidx")
                            .and_then(Value::as_i64)
                            .is_some_and(|v| v > 0)
                }
                "/play/$action" => {
                    ctx.get("$actlist")
                        .and_then(Value::as_str)
                        .is_some_and(|v| !v.is_empty())
                        && ctx
                            .get("$actfile")
                            .and_then(Value::as_str)
                            .is_some_and(|v| !v.is_empty())
                }
                "/rec/$action" => ctx
                    .get("path")
                    .and_then(Value::as_str)
                    .is_some_and(|v| !v.is_empty()),
                _ => false,
            };
            if !ready {
                c.read_only = true;
                c.unavailable_reason = Some("Open a show or available media and select a valid target before using this action.".into());
            }
        }
        if path == "/$ctl/lib/$actionidx"
            && !action_context
                .as_ref()
                .and_then(|v| v.get("$actshow"))
                .and_then(Value::as_str)
                .is_some_and(|s| !s.is_empty())
        {
            c.read_only = true;
            c.unavailable_reason = Some("Open a show before selecting a scene.".into());
        }
        ensure!(
            self.native_revision.load(Ordering::Acquire) == revision,
            "native state changed during read"
        );
        c.value = Some(value.clone());
        c.model = model.clone().map(Value::String);
        c.revision = json!({"generation":generation,"native_revision":revision,"path":path,"definition":format!("{def:?}"),"value":value,"model":model,"route_context":context,"action_context":action_context});
        Ok(c)
    }
    pub async fn browse_node(&self, path: &str) -> anyhow::Result<ConsoleControlNode> {
        ensure!(valid_path(path), "invalid native node path");
        let session = self.session()?;
        let generation = self.generation.load(Ordering::Acquire);
        let defs = self.native_defs(&session, path).await?;
        let mut items = Vec::with_capacity(defs.len());
        for def in defs {
            let p = child(path, &def.address_name()?);
            ensure!(valid_path(&p), "WING: unaddressable native child path");
            let item = match self.observed_control(&session, generation, &p, &def).await {
                Ok(c) => c,
                Err(e) => {
                    let mut c = descriptor(p, &def);
                    c.read_only = true;
                    c.unavailable_reason = Some(format!("Native value unavailable: {e:#}"));
                    c
                }
            };
            items.push(item);
        }
        self.ensure_current(&session)?;
        ensure!(
            self.generation.load(Ordering::Acquire) == generation,
            "WING session changed"
        );
        Ok(ConsoleControlNode {
            path: path.into(),
            label: if path == "/" {
                "WING".into()
            } else {
                path.rsplit('/').next().unwrap_or(path).into()
            },
            items,
            unavailable_reason: None,
        })
    }
    pub async fn write_console_control(
        &self,
        req: &ConsoleControlWrite,
        check: Check<'_>,
    ) -> anyhow::Result<ConsoleControlResult> {
        ensure!(
            valid_path(&req.path) && req.path != "/",
            "invalid native path"
        );
        let session = self.session()?;
        let generation = self.generation.load(Ordering::Acquire);
        let _guard = self.writes.lock().await;
        ensure!(check(), "access or session changed");
        self.ensure_current(&session)?;
        let (parent, name) = req.path.rsplit_once('/').context("invalid native path")?;
        let parent = if parent.is_empty() { "/" } else { parent };
        let defs = self.native_defs(&session, parent).await?;
        let def = defs
            .iter()
            .find(|d| d.address_name().is_ok_and(|child| child == name))
            .context("native definition unavailable")?;
        let before = self
            .observed_control(&session, generation, &req.path, def)
            .await?;
        ensure!(
            !before.read_only,
            "native control is read-only or requires a dedicated action"
        );
        ensure!(
            before.revision == req.revision,
            "native value, domain, model or session changed"
        );
        if before.kind == CK::Action {
            ensure!(
                before.choices.iter().any(|c| c.value == req.value),
                "unsupported native action"
            );
        }
        if before.kind == CK::Action {
            let command = req.value.as_str().context("invalid action")?;
            let ctx = before
                .revision
                .get("action_context")
                .context("action context unavailable")?;
            if req.path == "/$ctl/lib/$action" {
                ensure!(
                    ctx.get("$actidx") != ctx.get("$actionidx"),
                    "Selected scene is already active; reload cannot be independently confirmed"
                );
            } else {
                let target = match command {
                    "PLAY" => "PLAY",
                    "REC" => "REC",
                    "PAUSE" => "PAUSE",
                    "STOP" => "STOP",
                    _ => anyhow::bail!("unsupported action"),
                };
                ensure!(
                    ctx.get("$actstate").and_then(Value::as_str) != Some(target),
                    "Action would not produce a verifiable state change"
                );
            }
        }
        let arg = encode(def, &req.value)?;
        let current_defs = self.native_defs(&session, parent).await?;
        ensure!(
            current_defs
                .iter()
                .find(|d| d.address_name().is_ok_and(|child| child == name))
                == Some(def),
            "native definition changed"
        );
        ensure!(
            self.current_model(&session, &req.path).await?
                == before
                    .model
                    .as_ref()
                    .and_then(Value::as_str)
                    .map(str::to_string),
            "model changed before write"
        );
        let final_observation = self
            .observed_control(&session, generation, &req.path, def)
            .await?;
        ensure!(
            final_observation.revision == req.revision,
            "native value or route context changed before write"
        );
        ensure!(check(), "access or session changed");
        self.ensure_current(&session)?;
        let result = async {
            // Advance the barrier under the cache lock before dispatch, so
            // earlier in-flight definition reads cannot restore old proof.
            {
                let mut cache = self.color_domains.lock().unwrap();
                self.native_revision.fetch_add(1, Ordering::AcqRel);
                cache.clear();
            }
            session.send(&req.path, vec![arg]).await?;
            let after = self
                .observed_control(&session, generation, &req.path, def)
                .await?;
            ensure!(
                check() && self.generation.load(Ordering::Acquire) == generation,
                "access or session changed after write"
            );
            ensure!(
                after.model == before.model,
                "model changed during native write"
            );
            if req.path == "/$ctl/lib/$actionidx" {
                ensure!(
                    after.revision.get("action_context") == before.revision.get("action_context"),
                    "show changed during selection"
                );
            }
            ensure!(
                after.revision.get("route_context") == before.revision.get("route_context"),
                "route context changed during native write"
            );
            if before.kind == CK::Action {
                let command = req.value.as_str().context("invalid action")?;
                let target = match (req.path.as_str(), command) {
                    ("/play/$action", "PLAY") => Some("PLAY"),
                    ("/play/$action", "PAUSE") => Some("PAUSE"),
                    ("/play/$action", "STOP") => Some("STOP"),
                    ("/rec/$action", "REC") => Some("REC"),
                    ("/rec/$action", "PAUSE") => Some("PAUSE"),
                    ("/rec/$action", "STOP") => Some("STOP"),
                    _ => None,
                };
                if let Some(target) = target {
                    let state = after
                        .revision
                        .get("action_context")
                        .and_then(|v| v.get("$actstate"))
                        .and_then(Value::as_str);
                    ensure!(
                        state == Some(target),
                        "native action state could not be verified"
                    );
                } else {
                    ensure!(
                        after
                            .revision
                            .get("action_context")
                            .and_then(|v| v.get("$actshow"))
                            == before
                                .revision
                                .get("action_context")
                                .and_then(|v| v.get("$actshow")),
                        "show changed during scene action"
                    );
                    let active = session
                        .query_timeout("/$ctl/lib/$actidx", QUERY_TIMEOUT)
                        .await?;
                    let active_def = current_defs
                        .iter()
                        .find(|d| d.address_name().is_ok_and(|child| child == "$actidx"))
                        .context("active scene definition unavailable")?;
                    let active = value_of(active_def, &active)?;
                    ensure!(
                        Some(&active)
                            == before
                                .revision
                                .get("action_context")
                                .and_then(|v| v.get("$actionidx")),
                        "scene action state could not be verified"
                    );
                }
                return Ok::<_, anyhow::Error>((after, false));
            }
            let observed = after
                .value
                .as_ref()
                .context("native readback unavailable")?;
            let (agrees, quantized) = readback_agrees(def, &req.value, observed)?;
            ensure!(
                agrees,
                "native readback mismatch at {}: requested {}, observed {:?}, native kind {:?}",
                req.path,
                req.value,
                after.value,
                def.kind
            );
            Ok::<_, anyhow::Error>((after, quantized))
        }
        .await;
        Ok(match result {
            Ok((control, quantized)) => ConsoleControlResult {
                status: Status::Verified,
                control: Some(control),
                message: quantized
                    .then(|| "Value was set to the console's nearest native step.".to_string()),
            },
            Err(e) => ConsoleControlResult {
                status: Status::Uncertain,
                control: None,
                message: Some(format!("Native write may have applied: {e:#}")),
            },
        })
    }
}
