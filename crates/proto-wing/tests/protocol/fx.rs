use super::*;

#[test]
fn fx_schema_is_complete_and_normalization_matches_engineering_values() {
    assert_eq!(fx::MODELS.len(), 65);
    for model in fx::MODELS {
        assert!(!model.params.is_empty());
        assert!(model.params.len() <= 64);
        for def in model.params {
            for normalized in [0., 0.5, 1.] {
                let value = def.encode(normalized).unwrap();
                let msg = rosc::OscMessage {
                    addr: format!("/fx/1/{}", def.name),
                    args: vec![value],
                };
                let back = def
                    .decode(&msg)
                    .unwrap_or_else(|e| panic!("{} {}: {e}", model.id, def.name));
                assert!(
                    back.is_finite() && (0.0..=1.0).contains(&back),
                    "{} {}",
                    model.id,
                    def.name
                );
            }
        }
    }
    let hall = fx::MODELS.iter().find(|m| m.id == "HALL").unwrap();
    let decay = hall.params.iter().find(|p| p.name == "dcy").unwrap();
    let O::Float(seconds) = decay.encode(0.5).unwrap() else {
        panic!()
    };
    assert!((seconds - 1.).abs() < 0.05);
    let m = Mapping {
        path: "/ch/1/mute".into(),
        kind: WireKind::Bool,
        model: None,
    };
    assert!(m
        .decode(&rosc::OscMessage {
            addr: m.path.clone(),
            args: vec![O::Int(2)]
        })
        .is_err());
}

#[tokio::test]
async fn fx_reads_use_loaded_model_and_named_wire_parameters() {
    let desk = Desk::new().await;
    let driver = WingDriver::new();
    driver.connect(&desk.addr).await.unwrap();
    let model = fx::MODELS.iter().position(|m| m.id == "HALL").unwrap();
    let decay = fx::MODELS[model]
        .params
        .iter()
        .position(|p| p.name == "dcy")
        .unwrap() as u8;
    let value = driver.read_fx(1, decay, Some(model as u8)).await.unwrap();
    driver.set_fx_param(1, decay, value).await.unwrap();
    driver.get_fx_param(1, decay).await.unwrap();
    assert_eq!(desk.writes.lock().unwrap().last().unwrap().0, "/fx/1/dcy");
    assert!(driver.read_fx(1, decay, Some(0)).await.is_err());
}
