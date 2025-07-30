#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct AccountState {
    pub(super) slot: u64,
    pub(super) data: Vec<u8>,
}
