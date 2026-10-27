//! Provider-owned native libraries. Separate from app-local scenes and presets.
//! JSON descriptors are session-local guards, never portable backup formats.
use serde_json::Value;
pub type Check<'a> = &'a (dyn Fn() -> bool + Send + Sync);

pub fn receipt(status: &str, message: impl Into<String>, inventory: Value) -> Value {
    serde_json::json!({"status":status,"message":message.into(),"inventory":inventory})
}

/// Conservative wire envelope. Native UI character limits are not inferred
/// from this byte ceiling. Reject rather than clip UTF-8 or embedded NUL.
pub fn validate_name(name: &str) -> anyhow::Result<()> {
    anyhow::ensure!(
        !name.trim().is_empty() && name.trim() == name && name.len() <= 63 && !name.chars().any(char::is_control),
        "Name needs 1..63 UTF-8 bytes, no control characters or surrounding whitespace"
    );
    Ok(())
}
