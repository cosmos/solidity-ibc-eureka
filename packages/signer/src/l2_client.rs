use crate::attestor_error::AttestorError;

pub trait Header {
    fn chain_id(&self) -> u64;
    fn state_root(&self) -> Vec<u8>;
    fn timestamp(&self) -> u64;
}

pub trait L2Client {
    fn fetch_header(&self, height: u64) -> Result<impl Header, AttestorError>;
}
