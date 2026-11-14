//! Portable, provider-owned processor state inside an application-local scene.
//! Leaf IDs are provider-defined identifiers, not arbitrary native paths.
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NativeProcessorDomain {
    Gate,
    Dynamics,
    Eq,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "type",
    content = "value",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum NativeProcessorValue {
    Bool(bool),
    Integer(i64),
    Number(f64),
    Text(String),
    Enum(String),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeProcessorLeaf {
    pub id: String,
    pub value: NativeProcessorValue,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeProcessorBlock {
    pub strip_key: String,
    pub domain: NativeProcessorDomain,
    pub model: String,
    pub leaves: Vec<NativeProcessorLeaf>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeProcessorSnapshot {
    pub provider: String,
    pub version: u32,
    pub blocks: Vec<NativeProcessorBlock>,
}

/// Universal size, uniqueness and scalar checks; provider validation must
/// additionally prove the model-specific leaf roster and native value domains.
pub fn validate_structure(snapshot: &NativeProcessorSnapshot) -> anyhow::Result<()> {
    fn identifier(s: &str, max: usize, slash: bool) -> bool {
        !s.is_empty()
            && s.len() <= max
            && s.bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"_.-".contains(&b) || (slash && b == b'/'))
            && !s.contains("..")
            && !s.starts_with('/')
            && !s.ends_with('/')
    }
    anyhow::ensure!(
        identifier(&snapshot.provider, 32, false) && snapshot.version > 0,
        "invalid native processor provider or version"
    );
    anyhow::ensure!(
        snapshot.blocks.len() <= 384,
        "too many native processor blocks"
    );
    let mut blocks = BTreeSet::new();
    for block in &snapshot.blocks {
        anyhow::ensure!(
            identifier(&block.strip_key, 64, true) && identifier(&block.model, 64, false),
            "invalid native processor identity"
        );
        anyhow::ensure!(
            blocks.insert((&block.strip_key, block.domain)),
            "duplicate native processor block"
        );
        anyhow::ensure!(
            !block.leaves.is_empty() && block.leaves.len() <= 128,
            "invalid native processor leaf count"
        );
        let mut leaves = BTreeSet::new();
        for leaf in &block.leaves {
            anyhow::ensure!(
                identifier(&leaf.id, 64, false) && leaves.insert(&leaf.id),
                "invalid or duplicate native processor leaf"
            );
            match &leaf.value {
                NativeProcessorValue::Number(value) => {
                    anyhow::ensure!(value.is_finite(), "non-finite native processor value")
                }
                NativeProcessorValue::Text(value) | NativeProcessorValue::Enum(value) => {
                    anyhow::ensure!(
                        value.len() <= 256 && !value.chars().any(char::is_control),
                        "invalid native processor text"
                    )
                }
                _ => {}
            }
        }
    }
    Ok(())
}
