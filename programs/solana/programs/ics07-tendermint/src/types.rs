use anchor_lang::prelude::*;
use ibc_client_tendermint::types::ConsensusState as IbcConsensusState;
use ibc_core_client_types::Height;
use ibc_core_commitment_types::commitment::CommitmentRoot;
use tendermint::Time;
use tendermint_light_client_update_client::{ClientState as UpdateClientState, TrustThreshold};
use time::OffsetDateTime;

#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct ClientState {
    pub chain_id: String,
    pub trust_level_numerator: u64,
    pub trust_level_denominator: u64,
    pub trusting_period: u64,
    pub unbonding_period: u64,
    pub max_clock_drift: u64,
    pub frozen_height: IbcHeight,
    pub latest_height: IbcHeight,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct IbcHeight {
    pub revision_number: u64,
    pub revision_height: u64,
}

impl From<IbcHeight> for Height {
    fn from(h: IbcHeight) -> Self {
        Height::new(h.revision_number, h.revision_height).expect("valid height")
    }
}

impl From<Height> for IbcHeight {
    fn from(h: Height) -> Self {
        IbcHeight {
            revision_number: h.revision_number(),
            revision_height: h.revision_height(),
        }
    }
}

impl From<ClientState> for UpdateClientState {
    fn from(cs: ClientState) -> Self {
        UpdateClientState {
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


#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
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

        IbcConsensusState {
            timestamp: Time::from_unix_timestamp(seconds, nanos).expect("invalid time"),
            root: CommitmentRoot::from_bytes(&cs.root),
            next_validators_hash: tendermint::Hash::Sha256(cs.next_validators_hash),
        }
    }
}

impl From<IbcConsensusState> for ConsensusState {
    fn from(cs: IbcConsensusState) -> Self {
        ConsensusState {
            timestamp: cs.timestamp.unix_timestamp_nanos() as u64,
            root: cs
                .root
                .into_vec()
                .try_into()
                .expect("root must be 32 bytes"),
            next_validators_hash: cs
                .next_validators_hash
                .as_bytes()
                .try_into()
                .expect("hash must be 32 bytes"),
        }
    }
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct UpdateClientMsg {
    pub client_message: Vec<u8>,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct MembershipMsg {
    pub height: u64,
    pub delay_time_period: u64,
    pub delay_block_period: u64,
    pub proof: Vec<u8>,
    pub path: Vec<u8>,
    pub value: Vec<u8>,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct MisbehaviourMsg {
    pub client_id: String,
    pub misbehaviour: Vec<u8>, // Protobuf encoded Misbehaviour
}