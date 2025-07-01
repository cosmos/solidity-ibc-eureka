use crate::adapter_client::Signable;

pub struct AccountState {
    pub(super) slot: u64,
    pub(super) data: Vec<u8>,
}

impl Signable for AccountState {}
