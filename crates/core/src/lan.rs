//! Stored LAN preferences; these do not establish the connected address.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LanMode {
    Static,
    Dhcp,
    DhcpServer,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActiveNetworkMode {
    Ethernet,
    WirelessClient,
    AccessPoint,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct LanConfiguration {
    pub mode: LanMode,
    pub address: [u8; 4],
    pub mask: [u8; 4],
    pub gateway: [u8; 4],
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct LanSnapshot {
    pub generation: u64,
    pub stored: LanConfiguration,
    pub active_mode: ActiveNetworkMode,
}
#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case", tag = "status", content = "detail")]
pub enum LanApplyStatus {
    NotRequested,
    DispatchedUncertain,
    DispatchUncertain(String),
}
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct LanWriteResult {
    pub verified_stored: LanConfiguration,
    pub apply: LanApplyStatus,
}
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct LanWriteError {
    pub message: String,
    /// A dispatch was attempted; some preferences may have been staged.
    pub possibly_staged: bool,
}
