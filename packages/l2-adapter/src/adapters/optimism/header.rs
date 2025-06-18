use alloy::primitives::FixedBytes;
use serde::Deserialize;

#[derive(Deserialize)]
pub(super) struct SequenceNumber(u64);

#[derive(Deserialize)]
pub(super) struct L1Origin {
    hash: FixedBytes<32>,
    #[serde(rename = "number")]
    height: u64,
}

#[derive(Deserialize)]
pub(super) struct SyncHeader {
    pub(super) hash: FixedBytes<32>,
    #[serde(rename = "number")]
    pub(super) height: u64,
    #[serde(rename = "parentHash")]
    parent_hash: FixedBytes<32>,
    pub(super) timestamp: u64,
    #[serde(rename = "l1origin")]
    l1_origin: Option<L1Origin>,
    #[serde(rename = "sequenceNumber")]
    sequence_number: Option<SequenceNumber>,
}
