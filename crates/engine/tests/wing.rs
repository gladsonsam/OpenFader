//! The new provider is reachable through the engine's erased registry and
//! supplies the same app-facing capability contracts as existing providers.
use openfader_engine::drivers::registered_drivers;

#[tokio::test]
async fn wing_registry_exposes_topology_fx_choices_and_read_only_metadata() {
    let drivers = registered_drivers(None);
    let wing = drivers.iter().find(|r| r.name == "wing").unwrap();
    let driver = &wing.driver;
    assert!(driver.is_hardware());
    assert_eq!(driver.channel_count(), 40);
    assert_eq!(driver.strip_count(), 92);
    assert_eq!(driver.bus_count(), 16);
    assert_eq!(driver.mute_group_count(), 8);
    assert_eq!(driver.group_assign_caps(), (true, false));
    assert_eq!(driver.fx_slot_count(), 16);
    assert!(driver.param_read_only("wing:eq_bands"));
    assert!(driver.param_read_only("source"));
    assert!(!driver.param_read_only("source_a"));
    assert!(!driver.param_read_only("fader"));
    assert!(driver.param_read_only("wing:delay_state"));
    assert!(!driver.param_read_only("wing:delay_value"));
    assert!(driver.param_read_only("wing:input_mode"));
    assert!(driver.param_read_only("wing:source_mode_writable"));
    assert!(!driver.param_read_only("wing:source_mode"));
    assert!(driver
        .capture_param_override(openfader_core::StripId(0), "name")
        .await
        .is_none());
    assert!(driver
        .capture_param_override(openfader_core::StripId(0), "wing:source_mode")
        .await
        .unwrap()
        .is_err());

    use openfader_core::ParamValue;
    assert_eq!(
        driver.param_readback_equivalent(
            "wing:delay_value",
            &ParamValue::Text("MS:12".into()),
            &ParamValue::Text("MS:12.0".into())
        ),
        Some(true)
    );
    assert_eq!(
        driver.param_readback_equivalent(
            "wing:delay_value",
            &ParamValue::Text("M:12".into()),
            &ParamValue::Text("MS:12".into())
        ),
        Some(false)
    );
    assert_eq!(
        driver.param_readback_equivalent(
            "name",
            &ParamValue::Text("12".into()),
            &ParamValue::Text("12.0".into())
        ),
        None
    );

    let types = driver.fx_type_caps();
    assert_eq!(types.len(), 65);
    let hall = types.iter().find(|t| t.id == "HALL").unwrap();
    assert_eq!(hall.slots, (1..=8).collect::<Vec<_>>());
    assert!(hall
        .params
        .iter()
        .any(|p| p.label == "DECAY" && p.scale == "log"));
    let enhancer = types.iter().find(|t| t.id == "ENHANCE").unwrap();
    assert!(enhancer.slots.contains(&16));
    assert!(types
        .iter()
        .flat_map(|t| &t.params)
        .any(|p| !p.choices.is_empty()));
    let registry = wing.param_registry();
    assert!(registry.find("eq8_gain").is_some());
    assert!(registry.find("dca16_assign").is_some());
    assert!(registry.find("source_select").is_some());
    let restrictions = driver.scope_restrictions();
    for id in [
        "hpf_on",
        "hpf_freq",
        "hpf_slope",
        "wing:hcf_on",
        "wing:hcf_freq",
        "wing:filter_model",
    ] {
        let kinds = restrictions.iter().find(|(key, _)| *key == id).unwrap().1;
        assert_eq!(kinds, &[openfader_core::StripKind::In]);
    }
    let slope = registry.find("hpf_slope").unwrap();
    assert_eq!((slope.min, slope.max), (0., 3.));
    assert!(registry
        .validate("hpf_slope", &openfader_core::ParamValue::Float(3.))
        .is_ok());

    let restrictions = driver.scope_restrictions();
    for id in [
        "delay_on",
        "wing:delay_unit",
        "wing:delay_value",
        "wing:delay_state",
    ] {
        let scopes = restrictions.iter().find(|(key, _)| *key == id).unwrap().1;
        assert!(scopes.contains(&openfader_core::StripKind::Ret));
        assert!(scopes.contains(&openfader_core::StripKind::Mtx));
        assert!(!scopes.contains(&openfader_core::StripKind::Dca));
    }

    for id in ["_mg1", "_mg8"] {
        let def = registry.find(id).unwrap();
        assert_eq!(def.kind, openfader_core::params::ParamKind::Bool);
        for kind in [
            openfader_core::StripKind::In,
            openfader_core::StripKind::Ret,
            openfader_core::StripKind::Bus,
            openfader_core::StripKind::Main,
            openfader_core::StripKind::Mtx,
        ] {
            assert!(def.scopes.contains(&kind));
        }
        assert!(!def.scopes.contains(&openfader_core::StripKind::Dca));
    }
    assert_eq!(driver.routing_targets().len(), 2);
    assert!(driver
        .routing_groups()
        .iter()
        .any(|g| g.key == "A" && g.count == 48));
}

#[test]
fn wing_scene_scopes_keep_bus_matrix_main_and_fx_recall_independent() {
    use openfader_engine::RecallScope;
    let drivers = registered_drivers(None);
    let wing = drivers.iter().find(|r| r.name == "wing").unwrap();
    let scope = |group: &str| RecallScope {
        groups: Some(vec![group.into()]),
        ..Default::default()
    };
    // Every send leaf uses its destination family, including the slots that
    // the historical 6-bus/4-FX convention would put in the FX group.
    for slot in 1..=28 {
        let expected = match slot {
            1..=16 => "sends_bus",
            17..=24 => "sends_matrix",
            _ => "sends_main",
        };
        for suffix in ["level", "on", "pan", "tap", "pre"] {
            let id = format!("send{slot}_{suffix}");
            for group in [
                "sends_bus",
                "sends_matrix",
                "sends_main",
                "sends_fx",
                "config",
            ] {
                assert_eq!(
                    scope(group).allows_param_for(wing.driver.as_ref(), &id),
                    group == expected,
                    "{id} in {group}"
                );
            }
        }
    }
    for id in ["source_a", "source_b", "source_select"] {
        assert!(scope("source").allows_param_for(wing.driver.as_ref(), id));
        assert!(!scope("config").allows_param_for(wing.driver.as_ref(), id));
    }
    for id in ["eq1_gain", "eq6_type", "eq8_freq", "eq_on", "eq_mode"] {
        assert!(scope("eq").allows_param_for(wing.driver.as_ref(), id));
        assert!(!scope("config").allows_param_for(wing.driver.as_ref(), id));
    }
    assert!(scope("sends_main").allows_param_for(wing.driver.as_ref(), "lr_assign"));
    assert!(!scope("config").allows_param_for(wing.driver.as_ref(), "lr_assign"));
    assert!(RecallScope::default().allows_param_for(wing.driver.as_ref(), "send28_on"));
    assert!(!RecallScope {
        groups: Some(vec![]),
        ..Default::default()
    }
    .allows_param_for(wing.driver.as_ref(), "send28_on"));
    // Other desks retain their historical scope meanings and fallbacks.
    for name in ["sim", "xair", "dl32s"] {
        let driver = &drivers.iter().find(|r| r.name == name).unwrap().driver;
        for id in [
            "send1_level",
            "send7_level",
            "lr_assign",
            "eq1_gain",
            "source_a",
            "other",
        ] {
            for group in ["sends_bus", "sends_fx", "config", "eq", "source"] {
                assert_eq!(
                    scope(group).allows_param_for(driver.as_ref(), id),
                    scope(group).allows_param(id)
                );
            }
        }
    }
}
