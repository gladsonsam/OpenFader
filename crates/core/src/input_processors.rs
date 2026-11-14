//! Provider-owned input processor recall. Observed revisions are not desk CAS.
use crate::ParamValue;
use serde_json::{json, Value};

/// Strip keys on which a provider supports a guarded processor operation.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GuardedProcessorTargets {
    pub processor: String,
    pub strip_keys: Vec<String>,
}

/// Complete processor membership, including excluded bypass witnesses.
pub fn member(processor: &str, id: &str) -> bool {
    match processor {
        "channel_processing" => ["eq", "gate", "compressor"].iter().any(|p| member(p, id)),
        "eq" => id.starts_with("eq") || id.starts_with("hpf_"),
        "gate" => id.starts_with("gate_"),
        "compressor" => id.starts_with("comp_"),
        _ => false,
    }
}
pub fn writable(processor: &str, id: &str) -> bool {
    member(processor, id) && (processor == "channel_processing" || !id.ends_with("_on"))
}
pub fn value(value: &ParamValue) -> Value {
    match value {
        ParamValue::Float(v) => json!(v),
        ParamValue::Bool(v) => json!(v),
        ParamValue::Text(v) => json!(v),
    }
}
/// Static descriptor domain; providers append native domains/mappings as needed.
pub fn domain(ids: &[&str]) -> Value {
    json!(ids.iter().filter_map(|id| crate::params::find(id)).map(|d|
        json!({"id":d.id,"kind":format!("{:?}",d.kind),"min":d.min,"max":d.max,"scopes":format!("{:?}",d.scopes)})
    ).collect::<Vec<_>>())
}

/// Re-evaluate engine access, locks, descriptor and epoch under the provider's
/// serialized write lane, immediately before dispatch and after readback.
pub type RecallCheck<'a> = &'a (dyn Fn() -> anyhow::Result<()> + Send + Sync);
