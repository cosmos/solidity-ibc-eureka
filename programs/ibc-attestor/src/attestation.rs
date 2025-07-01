#[derive(Clone)]
pub struct Attestation {
    pub data: Vec<u8>,
    pub signature: [u8; 64],
}
