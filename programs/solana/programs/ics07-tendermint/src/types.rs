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
    UpdateSuccess,
    NoOp,
    Misbehaviour,
}

/// Parameters for uploading a header chunk
#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct UploadChunkParams {
    pub chain_id: String,
    pub target_height: u64,
    pub chunk_index: u8,
    pub chunk_data: Vec<u8>,
}

/// Parameters for uploading a misbehaviour chunk
#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct UploadMisbehaviourChunkParams {
    pub client_id: String,
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
    /// Access manager program ID for role-based access control
    pub access_manager: Pubkey,
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

#[cfg(test)]
mod compatibility_tests {
    use super::*;

    /// Ensures `IbcHeight` serialization remains compatible between program and solana-ibc-types
    /// This is critical because these types may be passed between programs
    #[test]
    fn test_ibc_height_serialization_compatibility() {
        let height = IbcHeight {
            revision_number: 1,
            revision_height: 1000,
        };

        let serialized = height.try_to_vec().unwrap();

        let types_height: solana_ibc_types::ics07::IbcHeight =
            AnchorDeserialize::deserialize(&mut &serialized[..]).unwrap();

        assert_eq!(height.revision_number, types_height.revision_number);
        assert_eq!(height.revision_height, types_height.revision_height);
    }

    /// Ensures `ConsensusState` serialization remains compatible between program and solana-ibc-types
    #[test]
    fn test_consensus_state_serialization_compatibility() {
        let consensus_state = ConsensusState {
            timestamp: 1_234_567_890,
            root: [1u8; 32],
            next_validators_hash: [2u8; 32],
        };

        let serialized = consensus_state.try_to_vec().unwrap();

        let types_consensus_state: solana_ibc_types::ics07::ConsensusState =
            AnchorDeserialize::deserialize(&mut &serialized[..]).unwrap();

        assert_eq!(consensus_state.timestamp, types_consensus_state.timestamp);
        assert_eq!(consensus_state.root, types_consensus_state.root);
        assert_eq!(
            consensus_state.next_validators_hash,
            types_consensus_state.next_validators_hash
        );
    }

    /// Ensures `ClientState` serialization remains compatible between program and solana-ibc-types
    #[test]
    fn test_client_state_serialization_compatibility() {
        let client_state = ClientState {
            chain_id: "test-chain".to_string(),
            trust_level_numerator: 1,
            trust_level_denominator: 3,
            trusting_period: 1_209_600,
            unbonding_period: 1_814_400,
            max_clock_drift: 10,
            frozen_height: IbcHeight {
                revision_number: 0,
                revision_height: 0,
            },
            latest_height: IbcHeight {
                revision_number: 1,
                revision_height: 1000,
            },
            access_manager: access_manager::ID,
        };

        let serialized = client_state.try_to_vec().unwrap();

        let types_client_state: solana_ibc_types::ics07::ClientState =
            AnchorDeserialize::deserialize(&mut &serialized[..]).unwrap();

        assert_eq!(client_state.chain_id, types_client_state.chain_id);
        assert_eq!(
            client_state.trust_level_numerator,
            types_client_state.trust_level_numerator
        );
        assert_eq!(
            client_state.trust_level_denominator,
            types_client_state.trust_level_denominator
        );
        assert_eq!(
            client_state.trusting_period,
            types_client_state.trusting_period
        );
        assert_eq!(
            client_state.unbonding_period,
            types_client_state.unbonding_period
        );
        assert_eq!(
            client_state.max_clock_drift,
            types_client_state.max_clock_drift
        );
        assert_eq!(
            client_state.frozen_height.revision_number,
            types_client_state.frozen_height.revision_number
        );
        assert_eq!(
            client_state.frozen_height.revision_height,
            types_client_state.frozen_height.revision_height
        );
        assert_eq!(
            client_state.latest_height.revision_number,
            types_client_state.latest_height.revision_number
        );
        assert_eq!(
            client_state.latest_height.revision_height,
            types_client_state.latest_height.revision_height
        );
    }

    /// Ensures `ClientState` SEED constant matches between program and solana-ibc-types
    #[test]
    fn test_client_state_seed_compatibility() {
        assert_eq!(
            ClientState::SEED,
            solana_ibc_types::ics07::ClientState::SEED
        );
    }
}
