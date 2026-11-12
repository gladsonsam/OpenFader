// Lab adapter only: compiled outside OpenFader against the pinned Sidecar tree.
use livedaw_engine::{ConsoleConfig, Engine, Profile};
use livedaw_wing::{server::EngineHandle, WingState};
use std::{net::IpAddr, sync::Arc, time::Duration};

fn main() {
    let model = std::env::args().nth(1).expect("model");
    let profile = match model.as_str() {
        "wing" | "wing-compact" | "wing-rack" => Profile::Wing,
        other => Profile::from_id(other).expect("Sidecar profile"),
    };
    let engine = Arc::new(Engine::new(0));
    engine.configure(&ConsoleConfig {
        profile,
        counts: profile.spec().counts,
    });
    let handle: Arc<dyn EngineHandle> = Arc::new(engine.clone());
    let bind: IpAddr = "127.0.0.1".parse().unwrap();
    // Retain server objects until the process exits. No audio devices or plugins.
    let _server: Box<dyn std::any::Any> = match model.as_str() {
        "wing" | "wing-compact" | "wing-rack" => {
            let mut config = livedaw_wing::server::Config {
                bind,
                ..Default::default()
            };
            if model != "wing" {
                let variant = if model == "wing-rack" {
                    livedaw_session::ConsoleModel::Rack
                } else {
                    livedaw_session::ConsoleModel::Compact
                };
                config.identity = livedaw_wing::Identity::of_model(variant);
            }
            let server =
                livedaw_wing::WingServer::start(handle, Arc::new(WingState::default()), &config);
            assert!(
                server.status().errors.is_empty(),
                "{:?}",
                server.status().errors
            );
            Box::new(server)
        }
        "x32" => {
            let config = livedaw_x32::Config {
                bind,
                ..Default::default()
            };
            let server = livedaw_x32::X32Server::start(handle, &config);
            assert!(
                server.status().errors.is_empty(),
                "{:?}",
                server.status().errors
            );
            Box::new(server)
        }
        "sq5" | "dlive" => {
            let config = livedaw_ahmidi::Config {
                bind,
                surface: if model == "dlive" { Some(51328) } else { None },
                ..Default::default()
            };
            let server = livedaw_ahmidi::Server::start(handle, &config);
            assert!(
                server.status().errors.is_empty(),
                "{:?}",
                server.status().errors
            );
            Box::new(server)
        }
        _ => panic!("No network transport for this lab fixture; see matrix.json"),
    };
    println!("READY {model} 127.0.0.1");
    loop {
        engine.collect();
        std::thread::sleep(Duration::from_millis(100));
    }
}
