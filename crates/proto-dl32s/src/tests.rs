//! Existing driver mapping and loopback tests.

    use super::*;
    use crate::input::{decode_push, decode_push_value, send_table_key, send_slot_for_table_key};
    use openfader_core::{MixerDriver, ParamValue, StripId};

    #[tokio::test]
    async fn not_connected_errors() {
        let d = Dl32sDriver::new();
        assert_eq!(d.name(), "dl32s");
        assert!(d.get_fader(StripId(0)).await.is_err());
        assert!(d.get_mute(StripId(0)).await.is_err());
        assert!(d.get_name(StripId(0)).await.is_err());
        assert!(d.get_mute_group(1).await.is_err());
        assert!(d.set_fader(StripId(0), 0.5).await.is_err());
    }

    #[test]
    fn strip_index_is_zero_based() {
        assert_eq!(Dl32sDriver::channel(StripId(0)).unwrap(), 1);
        assert_eq!(Dl32sDriver::channel(StripId(31)).unwrap(), 32);
        assert!(Dl32sDriver::channel(StripId(32)).is_err());
    }

    #[tokio::test]
    async fn unknown_field_errors_without_network() {
        let d = Dl32sDriver::new();
        // Field resolution happens before the connection check, so mapping
        // mistakes surface even offline.
        let err = d
            .set_param(StripId(0), "nope", ParamValue::Float(1.0))
            .await
            .unwrap_err()
            .to_string();
        assert!(err.contains("unknown"), "{err}");
        let err = d.get_param(StripId(0), "nope").await.unwrap_err().to_string();
        assert!(err.contains("unknown"), "{err}");
    }

    #[tokio::test]
    async fn wrong_value_kind_errors() {
        let d = Dl32sDriver::new();
        let err = d
            .set_field(StripId(0), "gain", ParamValue::Bool(true))
            .await
            .unwrap_err()
            .to_string();
        assert!(err.contains("Db"), "{err}");
    }

    #[test]
    fn capabilities_are_wave1_verified_set() {
        let caps = Dl32sDriver::new().capabilities();
        for id in [
            "fader", "mute", "name", "pan", "gain", "phantom", "trim", "polarity",
            "hpf_on", "hpf_freq", "lr_assign", "link", "color", "icon",
            "gate_on", "gate_thr", "gate_att", "gate_mode", "gate_rel",
            "gate_range", "gate_hold",
            "comp_on", "comp_mode", "comp_thr", "comp_ratio", "comp_knee",
            "comp_att", "comp_rel", "comp_makeup", "eq_on", "eq_mode",
            "eq1_gain", "eq1_freq", "eq1_q", "eq1_type",
            "eq2_gain", "eq2_freq", "eq2_q", "eq2_type",
            "eq3_gain", "eq3_freq", "eq3_q", "eq3_type",
            "eq4_gain", "eq4_freq", "eq4_q", "eq4_type",
            "aux1_level", "aux1_mute", "aux8_level", "aux8_mute",
            "fx1_level", "fx1_mute", "fx4_level", "fx4_mute",
            "send1_level", "send1_pan", "send8_level", "send8_pan",
            "send9_level", "send12_level",
        ] {
            assert!(caps.contains(&id), "caps miss {id}");
        }
        for id in [
            "comp_auto",
            "comp_det",
            "comp_env",
            "comp_mix", // no leaf
            "gate_key_src",
            "gate_filter_on",
            "comp_key_src", // no leaf
            "eq_bands",
            "hpf_slope",
            "aux1_pan",
            "send1_tap",
            "send1_grpon",
            "send1_on",
            "send9_pan",
            "send13_level",
            "send13_pan",
            "solo",
        ] {
            assert!(!caps.contains(&id), "caps must not advertise {id}");
        }
        // Every advertised id resolves in the canonical catalogue or the
        // driver's provider rows, so the engine renders a params row for it.
        let defs = Dl32sDriver::new().param_defs();
        for id in caps {
            let known = openfader_core::params::find(id).is_some()
                || defs.iter().any(|d| d.id == *id);
            assert!(known, "caps id {id} resolves nowhere");
        }
    }

    #[test]
    fn send_slot_targets() {
        assert_eq!(send_table_key("send1_level").as_deref(), Some("aux1_level"));
        assert_eq!(send_table_key("send1_pan").as_deref(), Some("aux1_pan"));
        assert_eq!(send_table_key("send8_level").as_deref(), Some("aux8_level"));
        assert_eq!(send_table_key("send8_pan").as_deref(), Some("aux8_pan"));
        assert_eq!(send_table_key("send9_level").as_deref(), Some("fx1_level"));
        assert_eq!(send_table_key("send9_pan"), None);
        assert_eq!(send_table_key("send12_level").as_deref(), Some("fx4_level"));
        assert_eq!(send_table_key("send13_level"), None);
        assert_eq!(send_table_key("send0_level"), None);
        assert_eq!(send_table_key("send1_tap"), None);
        assert_eq!(send_table_key("send1_grpon"), None);
        assert_eq!(send_table_key("send1_on"), None);
        assert_eq!(send_table_key("aux1_level"), None);
        assert_eq!(send_table_key("pan"), None);
    }

    #[test]
    fn send_slot_reverse_targets() {
        assert_eq!(send_slot_for_table_key("aux1_level"), Some((1, "level")));
        assert_eq!(send_slot_for_table_key("aux1_pan"), Some((1, "pan")));
        assert_eq!(send_slot_for_table_key("aux8_level"), Some((8, "level")));
        assert_eq!(send_slot_for_table_key("aux8_pan"), Some((8, "pan")));
        assert_eq!(send_slot_for_table_key("fx1_level"), Some((9, "level")));
        assert_eq!(send_slot_for_table_key("fx4_level"), Some((12, "level")));
        assert_eq!(send_slot_for_table_key("fx1_mute"), None);
        assert_eq!(send_slot_for_table_key("aux1_mute"), None);
        assert_eq!(send_slot_for_table_key("aux9_level"), None);
        assert_eq!(send_slot_for_table_key("fx5_level"), None);
        assert_eq!(send_slot_for_table_key("pan"), None);
    }

    #[tokio::test]
    async fn unknown_send_slots_error_offline() {
        let d = Dl32sDriver::new();
        // Mapping/slot resolution happens before the connection check, so
        // these surface even offline.
        for id in ["send13_level", "send1_tap", "send9_pan", "nope"] {
            let err = d
                .set_param(StripId(0), id, ParamValue::Float(1.0))
                .await
                .unwrap_err()
                .to_string();
            assert!(err.contains("unknown"), "{id}: {err}");
            let err = d
                .get_param(StripId(0), id)
                .await
                .unwrap_err()
                .to_string();
            assert!(err.contains("unknown"), "{id}: {err}");
        }
    }

    #[tokio::test]
    async fn wrong_value_kind_errors_offline() {
        let d = Dl32sDriver::new();
        // Canonical pan takes a float; a bool must fail with an encoding
        // error, not a connection error.
        let err = d
            .set_param(StripId(0), "pan", ParamValue::Bool(true))
            .await
            .unwrap_err()
            .to_string();
        assert!(err.contains("Float"), "{err}");
    }

    #[tokio::test]
    async fn out_of_domain_modes_rejected_offline() {
        let d = Dl32sDriver::new();
        // Binary-confirmed modes: only 0/1 reach the wire.
        for id in ["gate_mode", "comp_mode", "comp_knee", "eq_mode"] {
            let err = d
                .set_param(StripId(0), id, ParamValue::Float(2.0))
                .await
                .unwrap_err()
                .to_string();
            assert!(err.contains("must be 0"), "{id}: {err}");
        }
        // EQ types: 0..=5 accepted (raw 2/3/4 meanings verified, 0/1/5 relay exact).
        let err = d
            .set_param(StripId(0), "eq2_type", ParamValue::Float(6.0))
            .await
            .unwrap_err()
            .to_string();
        assert!(err.contains("0..=5"), "{err}");
    }

    #[test]
    fn decode_push_value_mirrors_get_param() {
        // ch1 fader (addr 9) -15 dB -> canonical level 0.5.
        let got = decode_push_value(9, (-15.0f32).to_bits());
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].0, StripId(0));
        assert_eq!(got[0].1, "fader");
        assert!(matches!(got[0].2, ParamValue::Float(v) if (v - 0.5).abs() < 1e-6));
        // ch1 mute (addr 8) = 1.
        let got = decode_push_value(8, 1);
        assert_eq!(
            got,
            vec![(StripId(0), "mute".to_string(), ParamValue::Bool(true))]
        );
        // ch1 pan (addr 10) 0.5 wire -> +50 canonical.
        let got = decode_push_value(10, 0.5f32.to_bits());
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].1, "pan");
        assert!(matches!(got[0].2, ParamValue::Float(v) if (v - 50.0).abs() < 1e-6));
        // Renames ride canonical ids.
        // ch11 (strip 10) main_assign = 1+10*106+10 = 1071.
        let got = decode_push_value(1071, 1);
        assert_eq!(
            got,
            vec![(StripId(10), "lr_assign".to_string(), ParamValue::Bool(true))]
        );
        let got = decode_push_value(1 + 3 * 106 + 27, (-20.0f32).to_bits());
        assert_eq!(got[0].1, "comp_thr");
        // Aux send levels ride twice (native + generic slot).
        let got = decode_push_value(51, (-10.0f32).to_bits());
        assert_eq!(got.len(), 2);
        assert!(got.iter().any(|(_, p, _)| p == "aux1_level"));
        assert!(got.iter().any(|(_, p, _)| p == "send1_level"));
        let got = decode_push_value(75, (-10.0f32).to_bits());
        assert!(got.iter().any(|(_, p, _)| p == "fx1_level"));
        assert!(got.iter().any(|(_, p, _)| p == "send9_level"));
        // Aux pan mirrors to the generic send slot with x100 units.
        let got = decode_push_value(53, 0.5f32.to_bits());
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].1, "send1_pan");
        assert!(matches!(got[0].2, ParamValue::Float(v) if (v - 50.0).abs() < 1e-6));
        // Write-proven leaves decode straight through.
        let got = decode_push_value(21, 50.0f32.to_bits());
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].1, "gate_att");
        let got = decode_push_value(47, 2.0f32.to_bits());
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].1, "eq1_type");
        // Icon preserves the provider-owned numeric domain.
        assert_eq!(decode_push_value(1 + 4, 1005),
            vec![(StripId(0), "icon".to_string(), ParamValue::Float(1005.0))]);
        // Unadvertised + out-of-range leaves decode to nothing.
        assert!(decode_push_value(4041, 1).is_empty()); // subgroup master: no strip
        assert!(decode_push_value(4347, 0).is_empty()); // main: no strip
        assert!(decode_push_value(99999, 0).is_empty()); // unknown space
    }

    #[test]
    fn decode_push_names_and_snapshot() {
        use openfader_transport::mackie::PushEvent;
        let got = decode_push(PushEvent::Names {
            pairs: vec![(1, "Vox".to_string()), (33, "Aux".to_string())],
        });
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].strip, StripId(0));
        assert_eq!(got[0].param, "name");
        assert_eq!(got[0].value, ParamValue::Text("Vox".to_string()));
        // Snapshot relays decode to nothing (session cache records them;
        // the poller converges the wholesale change).
        assert!(decode_push(PushEvent::SnapshotRecall(7)).is_empty());
        assert!(decode_push(PushEvent::SnapshotRename {
            slot: 2,
            name: "Main".to_string()
        })
        .is_empty());
        // Non-param value types (meters, mute-group masters) are skipped.
        let got = decode_push(PushEvent::Values {
            value_type: 1,
            start: 114,
            raws: vec![1],
        });
        assert!(got.is_empty());
    }

    #[tokio::test]
    async fn broadcast_push_arrives_as_param_update_loopback() {
        use openfader_transport::mackie::{
            decode_one, word_swap, Command, Message, MessageType, RawValue,
        };
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        async fn read_msg(
            stream: &mut tokio::net::TcpStream,
        ) -> anyhow::Result<Message> {
            let mut prefix = [0u8; 8];
            stream.read_exact(&mut prefix).await?;
            let chunks = u16::from_be_bytes([prefix[2], prefix[3]]) as usize;
            let mut rest = vec![0u8; chunks * 4 + if chunks == 0 { 0 } else { 4 }];
            if !rest.is_empty() {
                stream.read_exact(&mut rest).await?;
            }
            let mut buf = prefix.to_vec();
            buf.extend_from_slice(&rest);
            let (msg, _) =
                decode_one(&buf)?.ok_or_else(|| anyhow::anyhow!("truncated"))?;
            Ok(msg)
        }

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let fake = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut seq: u8 = 40;
            let mut cic = 0;
            loop {
                let req = match read_msg(&mut stream).await {
                    Ok(m) => m,
                    Err(_) => break,
                };
                let resp = Message::new(req.seq, MessageType::Response, req.command, vec![])
                    .unwrap();
                if stream.write_all(&resp.encode()).await.is_err() {
                    break;
                }
                if req.command == Command::ChannelInfoControl {
                    cic += 1;
                }
                if cic < 2 {
                    continue;
                }
                // Repeating (not one-shot) keeps this race-free — pushes
                // sent before the test subscribes are missed by broadcast
                // semantics, so a later repetition must follow.
                // Another client moved ch1 fader to -15 dB (addr 9).
                let body = openfader_transport::mackie::build_single_set(
                    9,
                    RawValue::Float(-15.0),
                    5,
                );
                seq = seq.wrapping_add(1);
                let push = Message::new(seq, MessageType::Broadcast, Command::ChannelValues, body)
                    .unwrap();
                if stream.write_all(&push.encode()).await.is_err() {
                    break;
                }
                let mut padded = b"Vox\x00".to_vec();
                while !padded.len().is_multiple_of(4) {
                    padded.push(0);
                }
                let mut nbody = vec![];
                nbody.extend_from_slice(&1u32.to_be_bytes());
                nbody.extend_from_slice(&0x00010000u32.to_be_bytes());
                nbody.extend_from_slice(&word_swap(&padded));
                seq = seq.wrapping_add(1);
                let names = Message::new(seq, MessageType::Request, Command::ChannelNames, nbody)
                    .unwrap();
                if stream.write_all(&names.encode()).await.is_err() {
                    break;
                }
                // Readiness sentinels: the driver's connect gate waits for
                // the bulk-state words a real desk's whole-space push
                // delivers (CH1 mute at 8, CH32 mute/fader at 3294/3295,
                // main fader at 4347) before reporting ready.
                for (addr, raw) in [
                    (8, RawValue::Int(0)),
                    (3294, RawValue::Int(0)),
                    (3295, RawValue::Float(-120.0)),
                    (4347, RawValue::Float(-15.0)),
                ] {
                    let body = openfader_transport::mackie::build_single_set(addr, raw, 5);
                    seq = seq.wrapping_add(1);
                    let push =
                        Message::new(seq, MessageType::Broadcast, Command::ChannelValues, body)
                            .unwrap();
                    if stream.write_all(&push.encode()).await.is_err() {
                        break;
                    }
                }
                // Pause before the next relay round (post-init the client
                // only sends no-reply keepalives, so nothing needs an
                // answer here; the write error above still exits promptly
                // on a dropped client).
                tokio::time::sleep(std::time::Duration::from_millis(200)).await;
            }
        });

        let d = Dl32sDriver::new();
        d.connect(&addr.to_string()).await.unwrap();
        let mut rx = d.subscribe_updates().expect("driver has update channel");
        let mut got_fader = false;
        let mut got_name = false;
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(5);
        while !(got_fader && got_name) {
            assert!(
                tokio::time::Instant::now() < deadline,
                "push updates missing: fader={got_fader} name={got_name}"
            );
            let u = tokio::time::timeout(std::time::Duration::from_secs(5), rx.recv())
                .await
                .expect("update timeout")
                .expect("update channel closed");
            if u.strip == StripId(0) && u.param == "fader" {
                assert!(matches!(u.value, ParamValue::Float(v) if (v - 0.5).abs() < 1e-6));
                got_fader = true;
            }
            if u.strip == StripId(0) && u.param == "name" {
                assert_eq!(u.value, ParamValue::Text("Vox".to_string()));
                got_name = true;
            }
        }
        assert!(matches!(
            d.get_param(StripId(0), "fader").await.unwrap(),
            ParamValue::Float(v) if (v - 0.5).abs() < 1e-6
        ));
        assert_eq!(
            d.get_param(StripId(0), "name").await.unwrap(),
            ParamValue::Text("Vox".to_string())
        );
        fake.abort();
    }
