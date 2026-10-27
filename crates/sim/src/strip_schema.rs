//! Additional offline return/control-group coverage; existing family scopes stay intact.
use openfader_core::{
    params::{self, ParamDef},
    StripKind,
};
pub const RETURN_KIND: StripKind = StripKind::Custom("return");
pub fn supports(def: &ParamDef, kind: StripKind) -> bool {
    if kind == RETURN_KIND {
        params::supports_kind(def, StripKind::In)
            && !def.id.starts_with("gate_")
            && !def.id.starts_with("hpf_")
    } else {
        params::supports_kind(def, kind)
    }
}
pub fn scopes() -> Vec<(&'static str, &'static [StripKind])> {
    static S: std::sync::OnceLock<Vec<(&'static str, &'static [StripKind])>> =
        std::sync::OnceLock::new();
    S.get_or_init(|| {
        params::CATALOGUE
            .iter()
            .filter(|d| supports(d, RETURN_KIND))
            .map(|d| {
                let mut kinds = d.scopes.to_vec();
                kinds.push(RETURN_KIND);
                (d.id, &*Box::leak(kinds.into_boxed_slice()))
            })
            .collect()
    })
    .clone()
}
pub fn definition(id: &str) -> Option<&'static ParamDef> {
    params::find(id).or_else(|| extra().iter().find(|d| d.id == id))
}
pub fn extra() -> &'static [ParamDef] {
    static DEFS: std::sync::OnceLock<Vec<ParamDef>> = std::sync::OnceLock::new();
    DEFS.get_or_init(|| {
        ["dca5_assign", "dca6_assign"]
            .iter()
            .map(|&id| {
                let mut d = *params::find("dca4_assign").expect("canonical membership");
                d.id = id;
                d.label = if id == "dca5_assign" {
                    "VCA 5"
                } else {
                    "VCA 6"
                };
                d.scopes = &[StripKind::In, StripKind::Ret, StripKind::Bus, RETURN_KIND];
                d
            })
            .collect()
    })
}
