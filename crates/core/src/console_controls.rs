//! Provider-owned native controls and guarded write receipts.
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ConsoleControlsCapability {
    pub available: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unavailable_reason: Option<String>,
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ConsoleControlKind {
    #[default]
    Branch,
    Integer,
    Number,
    Boolean,
    Text,
    Enumeration,
    Bitset,
    Action,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ConsoleControlChoice {
    pub label: String,
    pub value: Value,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct ConsoleControl {
    pub path: String,
    pub label: String,
    pub kind: ConsoleControlKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<Value>,
    pub read_only: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unavailable_reason: Option<String>,
    pub unit: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub step: Option<f64>,
    pub choices: Vec<ConsoleControlChoice>,
    /// Provider-owned proof of the observed value, domain, model and native
    /// session. Clients return it unchanged; providers revalidate it live.
    pub revision: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<Value>,
}

impl Default for ConsoleControl {
    fn default() -> Self {
        Self {
            path: String::new(),
            label: String::new(),
            kind: ConsoleControlKind::Branch,
            value: None,
            read_only: true,
            unavailable_reason: None,
            unit: String::new(),
            min: None,
            max: None,
            step: None,
            choices: Vec::new(),
            revision: Value::Null,
            model: None,
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ConsoleControlNode {
    pub path: String,
    pub label: String,
    pub items: Vec<ConsoleControl>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unavailable_reason: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ConsoleControlWrite {
    pub path: String,
    pub value: Value,
    pub revision: Value,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ConsoleControlStatus {
    Verified,
    Accepted,
    Rejected,
    Uncertain,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ConsoleControlResult {
    pub status: ConsoleControlStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub control: Option<ConsoleControl>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}
