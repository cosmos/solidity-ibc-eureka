use crate::adapter_client::Signable;

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct AccountState {
    pub(super) slot: u64,
    pub(super) data: Vec<u8>,
}

impl Signable for AccountState {
    fn height(&self) -> u64 {
        self.slot
    }
}
