//! Compatibility adapter for proven input patch providers. Providers may override
//! `routing_planes` with other executable planes; topology alone never advertises them.
use super::*;
use crate::protocol::{RoutingDestinationCaps, RoutingPlaneCaps};

pub fn input_planes<D: DynDriver + ?Sized>(drv: &D) -> Vec<RoutingPlaneCaps> {
    let groups = drv.routing_groups();
    if groups.is_empty() {
        return Vec::new();
    }
    let topo = drv.topology();
    drv.routing_targets()
        .into_iter()
        .map(|target| {
            let plane = if target.key.is_empty() {
                "input".to_string()
            } else {
                format!("input/{}", target.key)
            };
            RoutingPlaneCaps {
                key: plane.clone(),
                label: target.label.to_string(),
                destinations: topo
                    .iter()
                    .filter(|f| drv.routing_strip_kind(f.kind))
                    .flat_map(|f| (f.first..f.first + f.count).map(move |i| (f, i)))
                    .filter_map(|(f, i)| {
                        let key = openfader_core::strip_key_for_index(&topo, i)?;
                        Some(RoutingDestinationCaps {
                            output: false,
                            id: format!("{plane}/{key}"),
                            key,
                            strip: i as u16,
                            label: format!("{} {}", f.name_prefix, i - f.first + 1),
                            target: target.key.to_string(),
                            clearable: target.clearable,
                            source_groups: groups
                                .iter()
                                .map(|g| crate::protocol::RoutingGroupCaps {
                                    key: g.key.to_string(),
                                    label: g.label.to_string(),
                                    count: g.count,
                                    options: Vec::new(),
                                    mode: false,
                                })
                                .collect(),
                        })
                    })
                    .collect(),
            }
        })
        .collect()
}

/// Non-strip destination IDs are never resolved through strip topology.
pub fn output_planes(planes: Vec<openfader_core::routing::OutputPlane>) -> Vec<RoutingPlaneCaps> {
    planes
        .into_iter()
        .map(|p| RoutingPlaneCaps {
            key: p.key.into(),
            label: p.label.into(),
            destinations: (1..=p.count)
                .map(|slot| {
                    let id = format!("{}/{slot:02}", p.key);
                    RoutingDestinationCaps {
                        output: true,
                        id: id.clone(),
                        key: id,
                        strip: slot - 1,
                        label: format!("{} {slot}", p.label),
                        target: p.key.into(),
                        clearable: p.clearable,
                        source_groups: p
                            .sources
                            .iter()
                            .map(|(key, label, count)| crate::protocol::RoutingGroupCaps {
                                key: (*key).into(),
                                label: (*label).into(),
                                count: *count,
                                options: Vec::new(),
                                mode: false,
                            })
                            .collect(),
                    }
                })
                .collect(),
        })
        .collect()
}
