//! Lazy X32 console-control navigation and live observations.
use super::{
    actions, card,
    codecs::mapping,
    dp48, iq,
    presentation::descriptor,
    recorder,
    specs::{self, CONTROLS},
    user_assign,
};
use crate::{X32Driver, QUERY_TIMEOUT};
impl X32Driver {
    pub async fn console_node(
        &self,
        path: &str,
    ) -> anyhow::Result<openfader_core::console_controls::ConsoleControlNode> {
        use openfader_core::console_controls::{
            ConsoleControl as Item, ConsoleControlKind as Kind, ConsoleControlNode as Node,
        };
        let session = self.session()?;
        let generation = self.route_generation_for(&session)?;
        let branches = [
            ("card", "Expansion card"),
            ("assign", "User assignments"),
            ("iq", "iQ speakers"),
            ("remote", "Remote and MIDI"),
            ("dp48", "DP48"),
            ("settings", "Console settings"),
            ("tape", "USB recorder"),
            ("automix", "Automix"),
            ("mono", "Main output"),
            ("link", "Stereo linking"),
            ("monitor", "Monitor and solo"),
            ("talk", "Talkback"),
            ("osc", "Oscillator"),
        ];
        if path.is_empty() || path == "/" {
            return Ok(Node {
                path: path.into(),
                label: "Console controls".into(),
                items: branches
                    .into_iter()
                    .map(|(key, label)| Item {
                        path: format!("/{key}"),
                        label: label.into(),
                        kind: Kind::Branch,
                        ..Default::default()
                    })
                    .collect(),
                unavailable_reason: None,
            });
        }
        let branch = path.strip_prefix('/').unwrap_or(path);
        let assign_group = branch
            .strip_prefix("assign/")
            .filter(|g| ["a", "b", "c"].contains(g));
        let iq_page = branch
            .strip_prefix("iq/")
            .and_then(|n| n.parse::<usize>().ok())
            .filter(|n| (1..=4).contains(n));
        let dp48_page = branch
            .strip_prefix("dp48/assign/")
            .and_then(|n| n.parse::<usize>().ok())
            .filter(|n| (1..=6).contains(n));
        let label = if assign_group.is_some() {
            "User assignments"
        } else if iq_page.is_some() {
            "iQ speakers"
        } else if dp48_page.is_some() {
            "DP48 assignments"
        } else if branch == "dp48/names" {
            "DP48 group names"
        } else {
            branches
                .iter()
                .find(|(key, _)| *key == branch)
                .map(|(_, label)| *label)
                .ok_or_else(|| anyhow::anyhow!("x32: unknown console control branch"))?
        };
        let mut items = Vec::new();
        let card_model = if branch == "card" {
            let result = card::observed_type(&session).await.ok();
            self.ensure_current(&session)?;
            result
        } else {
            None
        };
        for row in CONTROLS
            .iter()
            .chain(dp48::CONTROLS.iter())
            .chain(iq::CONTROLS.iter())
            .chain(user_assign::CONTROLS.iter())
            .chain(card::CONTROLS.iter())
            .filter(|row| {
                if let Some(group) = assign_group {
                    return row.id.starts_with(&format!("assign.{group}."));
                }
                if let Some(page) = iq_page {
                    return row
                        .id
                        .strip_prefix("iq.")
                        .and_then(|id| id.rsplit('.').next())
                        .and_then(|n| n.parse::<usize>().ok())
                        .is_some_and(|n| ((page - 1) * 4 + 1..=page * 4).contains(&n));
                }
                if let Some(page) = dp48_page {
                    return row
                        .id
                        .strip_prefix("dp48.assign.")
                        .and_then(|n| n.parse::<usize>().ok())
                        .is_some_and(|n| ((page - 1) * 8 + 1..=page * 8).contains(&n));
                }
                if branch == "dp48/names" {
                    return row.id.starts_with("dp48.name.");
                }
                row.id.split('.').next() == Some(branch)
                    && row.category == label
                    && !row.id.starts_with("dp48.assign.")
                    && !row.id.starts_with("dp48.name.")
                    && !row.id.starts_with("iq.")
                    && !row.id.starts_with("assign.")
            })
        {
            self.ensure_current(&session)?;
            let browser_epoch = self
                .console_epoch
                .load(std::sync::atomic::Ordering::Acquire);
            if matches!(row.kind, specs::ConsoleControlKind::IQEq(_)) {
                let resolved = iq::resolve_eq_path(&session, row).await;
                self.ensure_current(&session)?;
                let item = if self
                    .console_epoch
                    .load(std::sync::atomic::Ordering::Acquire)
                    != browser_epoch
                {
                    descriptor(
                        row,
                        generation,
                        browser_epoch,
                        None,
                        Some("Console controls changed during observation; refresh".into()),
                    )
                } else {
                    match resolved {
                        Ok((native_path, value)) => iq::bind_eq(
                            descriptor(row, generation, browser_epoch, Some(value), None),
                            &native_path,
                        ),
                        Err(error) => descriptor(
                            row,
                            generation,
                            browser_epoch,
                            None,
                            Some(error.to_string()),
                        ),
                    }
                };
                items.push(item);
                continue;
            }
            if let specs::ConsoleControlKind::CardChoice { card_types, .. } = row.kind {
                if !card_model.is_some_and(|model| card_types.contains(&model)) {
                    items.push(descriptor(row, generation, browser_epoch, None, Some("Installed card identity is unavailable or this control belongs to a different card.".into())));
                    continue;
                }
            }
            let iq_model = if let specs::ConsoleControlKind::IQSound { model_path } = row.kind {
                let observed = iq::observed_model(&session, model_path).await.ok();
                self.ensure_current(&session)?;
                if !observed.is_some_and(|model| iq::profiles(model).is_some()) {
                    items.push(descriptor(
                        row,
                        generation,
                        browser_epoch,
                        None,
                        Some(
                            "No supported iQ model is independently observed for this speaker."
                                .into(),
                        ),
                    ));
                    continue;
                }
                observed
            } else {
                None
            };
            let m = mapping(row);
            let read = session
                .query_timeout(&m.path, QUERY_TIMEOUT)
                .await
                .and_then(|reply| m.decode(&reply));
            self.ensure_current(&session)?;
            let read = if let (Some(model), specs::ConsoleControlKind::IQSound { model_path }) =
                (iq_model, row.kind)
            {
                let current = iq::observed_model(&session, model_path).await;
                self.ensure_current(&session)?;
                if current.ok() != Some(model) {
                    Err(anyhow::anyhow!("iQ model changed during observation"))
                } else if matches!(&read, Ok(openfader_core::ParamValue::Float(v)) if *v >= iq::profiles(model).unwrap_or(&[]).len() as f32)
                {
                    Err(anyhow::anyhow!(
                        "Sound profile is outside the observed model's domain"
                    ))
                } else {
                    read
                }
            } else {
                read
            };
            let read = if self
                .console_epoch
                .load(std::sync::atomic::Ordering::Acquire)
                != browser_epoch
            {
                Err(anyhow::anyhow!(
                    "Console controls changed during observation; refresh"
                ))
            } else {
                read
            };
            let item = match read {
                Ok(value) => descriptor(row, generation, browser_epoch, Some(value), None),
                Err(error) => descriptor(
                    row,
                    generation,
                    browser_epoch,
                    None,
                    Some(format!("Not observed: {error}")),
                ),
            };
            items.push(if let Some(model) = iq_model {
                iq::bind_model(item, model)
            } else if matches!(row.kind, specs::ConsoleControlKind::CardChoice { .. }) {
                card::bind_model(item, card_model.expect("card type validated"))
            } else {
                item
            });
        }
        self.append_console_observations(&session, branch, &mut items)
            .await?;
        if branch == "dp48" {
            for page in 1..=6 {
                items.push(Item {
                    path: format!("/dp48/assign/{page}"),
                    label: format!("Inputs {}–{}", (page - 1) * 8 + 1, page * 8),
                    kind: Kind::Branch,
                    ..Default::default()
                });
            }
            items.push(Item {
                path: "/dp48/names".into(),
                label: "Group names".into(),
                kind: Kind::Branch,
                ..Default::default()
            });
            items.push(actions::broadcast_descriptor(self, &session, generation).await);
        }
        if branch == "tape" {
            items.push(Item { path: "/tape/transport".into(), label: "Recorder transport".into(), kind: Kind::Branch,
                unavailable_reason: Some("Recorder transport actions are not yet available; status and playback settings remain readable.".into()), ..Default::default() });
        }
        if branch == "card" {
            items.push(recorder::descriptor(self, &session, generation).await);
            items.push(Item { path: "/card.live_record".into(), label: "X-LIVE record".into(), kind: Kind::Action,
                unavailable_reason: Some("Recording requires a verified writable SD-media presence contract; playback and stop are available when observed.".into()), ..Default::default() });
        }
        if branch == "iq" {
            for page in 1..=4 {
                items.push(Item {
                    path: format!("/iq/{page}"),
                    label: format!("Speakers {}–{}", (page - 1) * 4 + 1, page * 4),
                    kind: Kind::Branch,
                    ..Default::default()
                });
            }
        }
        if branch == "assign" {
            for (group, label) in [("a", "Set A"), ("b", "Set B"), ("c", "Set C")] {
                items.push(Item {
                    path: format!("/assign/{group}"),
                    label: label.into(),
                    kind: Kind::Branch,
                    ..Default::default()
                });
            }
            items.push(Item { path: "/assign/page-jump".into(), label: "Page-jump assignments".into(), kind: Kind::Branch,
                unavailable_reason: Some("The native page-jump code format is ambiguous in the protocol reference; those assignments cannot yet be written.".into()), ..Default::default() });
        }
        Ok(Node {
            path: path.into(),
            label: label.into(),
            items,
            unavailable_reason: None,
        })
    }
}
