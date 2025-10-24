use anchor_lang::prelude::*;
use ibc_client_tendermint::types::ConsensusState as IbcConsensusState;
use ibc_core_client_types::Height;
use ibc_core_commitment_types::commitment::CommitmentRoot;
use std::fmt::Debug;
use tendermint::Time;
use tendermint_light_client_update_client::{ClientState as UpdateClientState, TrustThreshold};
use time::OffsetDateTime;

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub enum UpdateResult {
    Update,
    NoOp,
}

/// Parameters for uploading a header chunk
#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct UploadChunkParams {
    pub chain_id: String,
    pub target_height: u64,
    pub chunk_index: u8,
    pub chunk_data: Vec<u8>,
}

#[account]
#[derive(InitSpace)]
pub struct ClientState {
    #[max_len(64)]
    pub chain_id: String,
    pub trust_level_numerator: u64,
    pub trust_level_denominator: u64,
    pub trusting_period: u64,
    pub unbonding_period: u64,
    pub max_clock_drift: u64,
    pub frozen_height: IbcHeight,
    pub latest_height: IbcHeight,
}

impl ClientState {
    pub const SEED: &'static [u8] = b"client";

    pub const fn is_frozen(&self) -> bool {
        self.frozen_height.revision_height > 0
    }

    /// NOTE: supress clippy due to &mut self can't be const
    #[allow(clippy::missing_const_for_fn)]
    pub fn freeze(&mut self) {
        self.frozen_height = self.latest_height;
    }
}

#[derive(
    AnchorSerialize,
    AnchorDeserialize,
    Clone,
    Copy,
    Default,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    InitSpace,
)]
pub struct IbcHeight {
    pub revision_number: u64,
    pub revision_height: u64,
}

impl From<IbcHeight> for Height {
    fn from(h: IbcHeight) -> Self {
        Self::new(h.revision_number, h.revision_height).expect("valid height")
    }
}

impl From<Height> for IbcHeight {
    fn from(h: Height) -> Self {
        Self {
            revision_number: h.revision_number(),
            revision_height: h.revision_height(),
        }
    }
}

impl From<ClientState> for UpdateClientState {
    fn from(cs: ClientState) -> Self {
        Self {
            chain_id: cs.chain_id,
            trust_level: TrustThreshold {
                numerator: cs.trust_level_numerator,
                denominator: cs.trust_level_denominator,
            },
            trusting_period_seconds: cs.trusting_period,
            unbonding_period_seconds: cs.unbonding_period,
            max_clock_drift_seconds: cs.max_clock_drift,
            is_frozen: cs.frozen_height.revision_height > 0,
            latest_height: cs.latest_height.into(),
        }
    }
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, InitSpace, Eq, PartialEq)]
pub struct ConsensusState {
    pub timestamp: u64,
    pub root: [u8; 32],
    pub next_validators_hash: [u8; 32],
}

impl From<ConsensusState> for IbcConsensusState {
    fn from(cs: ConsensusState) -> Self {
        let time = OffsetDateTime::from_unix_timestamp_nanos(cs.timestamp.into())
            .expect("invalid timestamp");
        let seconds = time.unix_timestamp();
        let nanos = time.nanosecond();

        Self {
            timestamp: Time::from_unix_timestamp(seconds, nanos).expect("invalid time"),
            root: CommitmentRoot::from_bytes(&cs.root),
            next_validators_hash: tendermint::Hash::Sha256(cs.next_validators_hash),
        }
    }
}

impl TryFrom<IbcConsensusState> for ConsensusState {
    type Error = <[u8; 32] as TryFrom<Vec<u8>>>::Error;

    fn try_from(cs: IbcConsensusState) -> std::result::Result<Self, Self::Error> {
        Ok(Self {
            timestamp: cs.timestamp.unix_timestamp_nanos() as u64,
            root: cs.root.into_vec().try_into()?,
            next_validators_hash: cs.next_validators_hash.as_bytes().to_vec().try_into()?,
        })
    }
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct UpdateClientMsg {
    pub client_message: Vec<u8>,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct MisbehaviourMsg {
    pub client_id: String,
    pub misbehaviour: Vec<u8>, // Protobuf encoded Misbehaviour
}
