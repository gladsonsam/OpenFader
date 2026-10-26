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
                    .filter(|f| f.kind == StripKind::In)
                    .flat_map(|f| (f.first..f.first + f.count).map(move |i| (f, i)))
                    .filter_map(|(f, i)| {
                        let key = openfader_core::strip_key_for_index(&topo, i)?;
                        Some(RoutingDestinationCaps {
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
                                })
                                .collect(),
                        })
                    })
                    .collect(),
            }
        })
        .collect()
}
