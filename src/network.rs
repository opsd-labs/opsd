use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PathKind {
    #[default]
    Public,
    Overlay,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct InterfaceAddress {
    pub interface: String,
    pub address: String,
    pub prefix: u8,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct NetworkFacts {
    pub hostname: String,
    pub interfaces: Vec<InterfaceAddress>,
    pub collected_at: i64,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OverlayMember {
    pub node_id: String,
    pub address: String,
    pub prefix: u8,
    pub interface: String,
    pub provider: String,
    pub machine_id: String,
    pub instance_id: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct Directory {
    pub revision: u64,
    pub controller_node_id: Option<String>,
    pub overlays: Vec<OverlayMember>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OverlayTrust {
    pub interface: String,
    pub network: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct Allowlist {
    pub revision: u64,
    pub public_addresses: Vec<String>,
    pub overlay: Option<OverlayTrust>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DesiredNetwork {
    #[serde(default)]
    pub revision: u64,
    pub allowlist: Allowlist,
    pub hub_overlay_address: Option<String>,
    pub probes: crate::protocol::ProbeTargetSet,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AppliedNetwork {
    pub desired_revision: u64,
    pub applied_revision: u64,
    pub backend: Option<String>,
    pub error: Option<String>,
    pub at: i64,
}

pub fn public_ip(ip: std::net::IpAddr) -> bool {
    crate::protocol::validate_public_address(&ip.to_string()).is_ok()
}
