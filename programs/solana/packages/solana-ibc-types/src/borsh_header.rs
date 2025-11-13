//! Borsh-serializable wrapper types for Tendermint Header
//!
//! These types mirror ibc-client-tendermint::types::Header and related types,
//! but use Borsh serialization for efficient memory usage on Solana.
//!
//! Memory comparison:
//! - Protobuf: 38KB serialized → ~300KB deserialized
//! - Borsh: ~38KB serialized → ~60-90KB deserialized
//!
//! This allows fitting mainnet-sized headers within Solana's 256KB heap limit.
//!
//! Note: Conversion functions (From/TryFrom implementations) are implemented
//! in the packages that have access to ibc-rs types (relayer and ics07-tendermint).
//!
//! ## Direct Deserialization Optimization
//!
//! The HeaderWrapper type provides direct BorshDeserialize implementation for
//! ibc_client_tendermint::types::Header, eliminating the costly conversion step
//! from BorshHeader → Header (saves ~300k compute units).

use borsh::{BorshDeserialize, BorshSerialize};

#[cfg(feature = "direct-deser")]
use std::io::{self, Read};

/// Borsh-serializable wrapper for ibc_client_tendermint::types::Header
#[derive(Clone, Debug, BorshSerialize, BorshDeserialize)]
pub struct BorshHeader {
    pub signed_header: BorshSignedHeader,
    pub validator_set: BorshValidatorSet,
    pub trusted_height: BorshHeight,
    pub trusted_next_validator_set: BorshValidatorSet,
}

/// Borsh-serializable wrapper for tendermint::block::signed_header::SignedHeader
#[derive(Clone, Debug, BorshSerialize, BorshDeserialize)]
pub struct BorshSignedHeader {
    pub header: BorshBlockHeader,
    pub commit: BorshCommit,
}

/// Borsh-serializable wrapper for tendermint::block::Header
#[derive(Clone, Debug, BorshSerialize, BorshDeserialize)]
pub struct BorshBlockHeader {
    pub version: BorshConsensusVersion,
    pub chain_id: String,
    pub height: u64,
    pub time: BorshTimestamp,
    pub last_block_id: Option<BorshBlockId>,
    pub last_commit_hash: Option<Vec<u8>>,
    pub data_hash: Option<Vec<u8>>,
    pub validators_hash: Vec<u8>,
    pub next_validators_hash: Vec<u8>,
    pub consensus_hash: Vec<u8>,
    pub app_hash: Vec<u8>,
    pub last_results_hash: Option<Vec<u8>>,
    pub evidence_hash: Option<Vec<u8>>,
    pub proposer_address: Vec<u8>,
}

#[derive(Clone, Debug, BorshSerialize, BorshDeserialize)]
pub struct BorshConsensusVersion {
    pub block: u64,
    pub app: u64,
}

#[derive(Clone, Debug, BorshSerialize, BorshDeserialize)]
pub struct BorshTimestamp {
    pub secs: i64,
    pub nanos: i32,
}

#[derive(Clone, Debug, BorshSerialize, BorshDeserialize)]
pub struct BorshBlockId {
    pub hash: Vec<u8>,
    pub part_set_header: BorshPartSetHeader,
}

#[derive(Clone, Debug, BorshSerialize, BorshDeserialize)]
pub struct BorshPartSetHeader {
    pub total: u32,
    pub hash: Vec<u8>,
}

/// Borsh-serializable wrapper for tendermint::block::Commit
#[derive(Clone, Debug, BorshSerialize, BorshDeserialize)]
pub struct BorshCommit {
    pub height: u64,
    pub round: u16,
    pub block_id: BorshBlockId,
    pub signatures: Vec<BorshCommitSig>,
}

/// Borsh-serializable wrapper for tendermint::block::CommitSig
#[derive(Clone, Debug, BorshSerialize, BorshDeserialize)]
pub enum BorshCommitSig {
    BlockIdFlagAbsent,
    BlockIdFlagCommit {
        validator_address: [u8; 20],
        timestamp: BorshTimestamp,
        signature: [u8; 64],
    },
    BlockIdFlagNil {
        validator_address: [u8; 20],
        timestamp: BorshTimestamp,
        signature: [u8; 64],
    },
}

/// Borsh-serializable wrapper for tendermint::validator::Set
#[derive(Clone, Debug, BorshSerialize, BorshDeserialize)]
pub struct BorshValidatorSet {
    pub validators: Vec<BorshValidator>,
    pub proposer: Option<BorshValidator>,
    pub total_voting_power: u64,
}

/// Borsh-serializable wrapper for tendermint::validator::Info
#[derive(Clone, Debug, BorshSerialize, BorshDeserialize)]
pub struct BorshValidator {
    pub address: [u8; 20],
    pub pub_key: BorshPublicKey,
    pub voting_power: u64,
    pub proposer_priority: i64,
}

/// Borsh-serializable wrapper for tendermint::public_key::PublicKey
#[derive(Clone, Debug, BorshSerialize, BorshDeserialize)]
pub enum BorshPublicKey {
    Ed25519([u8; 32]),
    Secp256k1([u8; 33]),
}

/// Borsh-serializable wrapper for ibc_core_client_types::Height
#[derive(Clone, Debug, BorshSerialize, BorshDeserialize)]
pub struct BorshHeight {
    pub revision_number: u64,
    pub revision_height: u64,
}

// ============================================================================
// Direct Deserialization Wrapper
// ============================================================================

#[cfg(feature = "direct-deser")]
mod direct_deser {
    use super::*;
    use ibc_client_tendermint::types::Header;
    use ibc_core_client_types::Height;
    use tendermint::account::Id as AccountId;
    use tendermint::block::{Commit, CommitSig, Header as TmHeader};
    use tendermint::block::parts::Header as PartSetHeader;
    use tendermint::block::signed_header::SignedHeader;
    use tendermint::block::Id as BlockId;
    use tendermint::validator::{Info as ValidatorInfo, Set as ValidatorSet};
    use tendermint::{Hash, PublicKey, Time};

    /// Zero-copy wrapper for direct deserialization of Header
    ///
    /// This wrapper implements BorshDeserialize to convert bytes directly into
    /// ibc_client_tendermint::types::Header, bypassing the intermediate BorshHeader
    /// representation. This saves ~300k compute units by eliminating redundant
    /// type conversions and allocations.
    #[repr(transparent)]
    pub struct HeaderWrapper(pub Header);

    impl std::ops::Deref for HeaderWrapper {
        type Target = Header;
        fn deref(&self) -> &Self::Target {
            &self.0
        }
    }

    impl std::ops::DerefMut for HeaderWrapper {
        fn deref_mut(&mut self) -> &mut Self::Target {
            &mut self.0
        }
    }

    impl From<HeaderWrapper> for Header {
        fn from(wrapper: HeaderWrapper) -> Self {
            wrapper.0
        }
    }

    impl BorshDeserialize for HeaderWrapper {
        fn deserialize_reader<R: Read>(reader: &mut R) -> io::Result<Self> {
            let signed_header = deserialize_signed_header(reader)?;
            let validator_set = deserialize_validator_set(reader)?;
            let trusted_height = deserialize_height(reader)?;
            let trusted_next_validator_set = deserialize_validator_set(reader)?;

            Ok(HeaderWrapper(Header {
                signed_header,
                validator_set,
                trusted_height,
                trusted_next_validator_set,
            }))
        }
    }

    // Primitive deserializers

    fn deserialize_height<R: Read>(reader: &mut R) -> io::Result<Height> {
        let revision_number = u64::deserialize_reader(reader)?;
        let revision_height = u64::deserialize_reader(reader)?;
        Height::new(revision_number, revision_height)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))
    }

    fn deserialize_time<R: Read>(reader: &mut R) -> io::Result<Time> {
        let secs = i64::deserialize_reader(reader)?;
        let nanos = i32::deserialize_reader(reader)?;
        Time::from_unix_timestamp(secs, nanos as u32)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))
    }

    fn deserialize_required_hash<R: Read>(reader: &mut R) -> io::Result<Hash> {
        let hash_vec = Vec::<u8>::deserialize_reader(reader)?;
        let hash_bytes: [u8; 32] = hash_vec
            .try_into()
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "Invalid hash length"))?;
        Ok(Hash::Sha256(hash_bytes))
    }

    fn deserialize_optional_hash<R: Read>(reader: &mut R) -> io::Result<Option<Hash>> {
        let has_value = u8::deserialize_reader(reader)?;
        if has_value == 1 {
            Ok(Some(deserialize_required_hash(reader)?))
        } else {
            Ok(None)
        }
    }

    // PublicKey and Validator deserializers

    fn deserialize_public_key<R: Read>(reader: &mut R) -> io::Result<PublicKey> {
        let discriminant = u8::deserialize_reader(reader)?;
        match discriminant {
            0 => {
                // Ed25519
                let mut bytes = [0u8; 32];
                reader.read_exact(&mut bytes)?;
                PublicKey::from_raw_ed25519(&bytes)
                    .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "Invalid Ed25519 key"))
            }
            1 => {
                // Secp256k1 (not supported on Solana)
                Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "Secp256k1 not supported on Solana",
                ))
            }
            _ => Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Unknown public key type",
            )),
        }
    }

    fn deserialize_validator<R: Read>(reader: &mut R) -> io::Result<ValidatorInfo> {
        let mut address_bytes = [0u8; 20];
        reader.read_exact(&mut address_bytes)?;
        let address = AccountId::new(address_bytes);

        let pub_key = deserialize_public_key(reader)?;

        let voting_power = u64::deserialize_reader(reader)?;
        let power = tendermint::vote::Power::try_from(voting_power)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?;

        let proposer_priority_val = i64::deserialize_reader(reader)?;
        let proposer_priority = tendermint::validator::ProposerPriority::from(proposer_priority_val);

        Ok(ValidatorInfo {
            address,
            pub_key,
            power,
            proposer_priority,
            name: None,
        })
    }

    fn deserialize_validator_set<R: Read>(reader: &mut R) -> io::Result<ValidatorSet> {
        let validators_len = u32::deserialize_reader(reader)?;
        let mut validators = Vec::with_capacity(validators_len as usize);

        for _ in 0..validators_len {
            validators.push(deserialize_validator(reader)?);
        }

        let has_proposer = u8::deserialize_reader(reader)?;
        let proposer = if has_proposer == 1 {
            Some(deserialize_validator(reader)?)
        } else {
            None
        };

        // Read total_voting_power (but ignore it, ValidatorSet recalculates)
        let _total_voting_power = u64::deserialize_reader(reader)?;

        Ok(ValidatorSet::new(validators, proposer))
    }

    // Commit deserializers

    fn deserialize_commit_sig<R: Read>(reader: &mut R) -> io::Result<CommitSig> {
        let discriminant = u8::deserialize_reader(reader)?;
        match discriminant {
            0 => Ok(CommitSig::BlockIdFlagAbsent),
            1 => {
                // BlockIdFlagCommit
                let mut validator_address = [0u8; 20];
                reader.read_exact(&mut validator_address)?;

                let timestamp = deserialize_time(reader)?;

                let mut signature = [0u8; 64];
                reader.read_exact(&mut signature)?;
                let signature = tendermint::Signature::new(signature)
                    .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?;

                Ok(CommitSig::BlockIdFlagCommit {
                    validator_address: AccountId::new(validator_address),
                    timestamp,
                    signature,
                })
            }
            2 => {
                // BlockIdFlagNil
                let mut validator_address = [0u8; 20];
                reader.read_exact(&mut validator_address)?;

                let timestamp = deserialize_time(reader)?;

                let mut signature = [0u8; 64];
                reader.read_exact(&mut signature)?;
                let signature = tendermint::Signature::new(signature)
                    .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?;

                Ok(CommitSig::BlockIdFlagNil {
                    validator_address: AccountId::new(validator_address),
                    timestamp,
                    signature,
                })
            }
            _ => Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Unknown CommitSig variant",
            )),
        }
    }

    fn deserialize_part_set_header<R: Read>(reader: &mut R) -> io::Result<PartSetHeader> {
        let total = u32::deserialize_reader(reader)?;
        let hash = deserialize_required_hash(reader)?;
        PartSetHeader::new(total, hash)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))
    }

    fn deserialize_block_id<R: Read>(reader: &mut R) -> io::Result<BlockId> {
        let hash = deserialize_required_hash(reader)?;
        let part_set_header = deserialize_part_set_header(reader)?;
        Ok(BlockId {
            hash,
            part_set_header,
        })
    }

    fn deserialize_commit<R: Read>(reader: &mut R) -> io::Result<Commit> {
        let height_val = u64::deserialize_reader(reader)?;
        let height = tendermint::block::Height::try_from(height_val)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?;

        let round_val = u16::deserialize_reader(reader)?;
        let round = tendermint::block::Round::try_from(round_val)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?;

        let block_id = deserialize_block_id(reader)?;

        let sigs_len = u32::deserialize_reader(reader)?;
        let mut signatures = Vec::with_capacity(sigs_len as usize);
        for _ in 0..sigs_len {
            signatures.push(deserialize_commit_sig(reader)?);
        }

        Ok(Commit {
            height,
            round,
            block_id,
            signatures,
        })
    }

    // Block header deserializers

    fn deserialize_consensus_version<R: Read>(
        reader: &mut R,
    ) -> io::Result<tendermint::block::header::Version> {
        let block = u64::deserialize_reader(reader)?;
        let app = u64::deserialize_reader(reader)?;
        Ok(tendermint::block::header::Version { block, app })
    }

    fn deserialize_block_header<R: Read>(reader: &mut R) -> io::Result<TmHeader> {
        let version = deserialize_consensus_version(reader)?;

        let chain_id = String::deserialize_reader(reader)?;
        let chain_id = chain_id
            .try_into()
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "Invalid chain ID"))?;

        let height_val = u64::deserialize_reader(reader)?;
        let height = tendermint::block::Height::try_from(height_val)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?;

        let time = deserialize_time(reader)?;

        let has_last_block_id = u8::deserialize_reader(reader)?;
        let last_block_id = if has_last_block_id == 1 {
            Some(deserialize_block_id(reader)?)
        } else {
            None
        };

        let last_commit_hash = deserialize_optional_hash(reader)?;
        let data_hash = deserialize_optional_hash(reader)?;

        let validators_hash = deserialize_required_hash(reader)?;
        let next_validators_hash = deserialize_required_hash(reader)?;
        let consensus_hash = deserialize_required_hash(reader)?;

        let app_hash_vec = Vec::<u8>::deserialize_reader(reader)?;
        let app_hash = tendermint::AppHash::try_from(app_hash_vec)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?;

        let last_results_hash = deserialize_optional_hash(reader)?;
        let evidence_hash = deserialize_optional_hash(reader)?;

        let proposer_vec = Vec::<u8>::deserialize_reader(reader)?;
        let proposer_bytes: [u8; 20] = proposer_vec.try_into().map_err(|_| {
            io::Error::new(io::ErrorKind::InvalidData, "Invalid proposer address length")
        })?;
        let proposer_address = AccountId::new(proposer_bytes);

        Ok(TmHeader {
            version,
            chain_id,
            height,
            time,
            last_block_id,
            last_commit_hash,
            data_hash,
            validators_hash,
            next_validators_hash,
            consensus_hash,
            app_hash,
            last_results_hash,
            evidence_hash,
            proposer_address,
        })
    }

    fn deserialize_signed_header<R: Read>(reader: &mut R) -> io::Result<SignedHeader> {
        let header = deserialize_block_header(reader)?;
        let commit = deserialize_commit(reader)?;

        SignedHeader::new(header, commit)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))
    }
}

#[cfg(feature = "direct-deser")]
pub use direct_deser::HeaderWrapper;
