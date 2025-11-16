//! Conversion functions from ibc-rs types to BorshHeader types
//!
//! These conversions are used by the relayer to convert Header to BorshHeader
//! for efficient serialization before uploading to Solana.
//!
//! Note: We use helper functions instead of From implementations to maintain
//! consistency with the Solana program's approach and avoid orphan rule issues.

use ibc_client_tendermint::types::Header;
use ibc_core_client_types::Height;
use solana_ibc_types::borsh_header::*;
use tendermint::block::parts::Header as PartSetHeader;
use tendermint::block::Id as BlockId;
use tendermint::block::{signed_header::SignedHeader, Commit, CommitSig, Header as TmHeader};
use tendermint::validator::{Info as ValidatorInfo, Set as ValidatorSet};
use tendermint::{PublicKey, Time};

pub fn height_to_borsh(height: Height) -> BorshHeight {
    BorshHeight {
        revision_number: height.revision_number(),
        revision_height: height.revision_height(),
    }
}

pub fn public_key_to_borsh(pk: PublicKey) -> BorshPublicKey {
    match pk {
        PublicKey::Ed25519(bytes) => {
            let bytes_array: [u8; 32] = bytes
                .as_bytes()
                .try_into()
                .expect("Ed25519 pubkey must be 32 bytes");
            BorshPublicKey::Ed25519(bytes_array)
        }
        _ => panic!("Only Ed25519 public keys are supported on Solana"),
    }
}

pub fn validator_to_borsh(v: ValidatorInfo) -> BorshValidator {
    let address_array: [u8; 20] = v
        .address
        .as_bytes()
        .try_into()
        .expect("Validator address must be 20 bytes");

    BorshValidator {
        address: address_array,
        pub_key: public_key_to_borsh(v.pub_key),
        voting_power: v.power.value(),
        proposer_priority: v.proposer_priority.value(),
    }
}

pub fn validator_set_to_borsh(vs: ValidatorSet) -> BorshValidatorSet {
    BorshValidatorSet {
        validators: vs
            .validators()
            .iter()
            .cloned()
            .map(validator_to_borsh)
            .collect(),
        proposer: vs.proposer().clone().map(|p| validator_to_borsh(p)),
        total_voting_power: vs.total_voting_power().value(),
    }
}

pub fn time_to_borsh(t: Time) -> BorshTimestamp {
    BorshTimestamp {
        secs: t.unix_timestamp(),
        nanos: (t.unix_timestamp_nanos() % 1_000_000_000) as i32,
    }
}

pub fn part_set_header_to_borsh(psh: PartSetHeader) -> BorshPartSetHeader {
    BorshPartSetHeader {
        total: psh.total,
        hash: psh.hash.as_bytes().to_vec(),
    }
}

pub fn block_id_to_borsh(bid: BlockId) -> BorshBlockId {
    BorshBlockId {
        hash: bid.hash.as_bytes().to_vec(),
        part_set_header: part_set_header_to_borsh(bid.part_set_header),
    }
}

pub fn commit_sig_to_borsh(cs: CommitSig) -> BorshCommitSig {
    match cs {
        CommitSig::BlockIdFlagAbsent => BorshCommitSig::BlockIdFlagAbsent,
        CommitSig::BlockIdFlagCommit {
            validator_address,
            timestamp,
            signature,
        } => {
            let address_array: [u8; 20] = validator_address
                .as_bytes()
                .try_into()
                .expect("Validator address must be 20 bytes");
            let sig_array: [u8; 64] = signature
                .map(|s| s.as_bytes().try_into().expect("Signature must be 64 bytes"))
                .unwrap_or([0u8; 64]);

            BorshCommitSig::BlockIdFlagCommit {
                validator_address: address_array,
                timestamp: time_to_borsh(timestamp),
                signature: sig_array,
            }
        }
        CommitSig::BlockIdFlagNil {
            validator_address,
            timestamp,
            signature,
        } => {
            let address_array: [u8; 20] = validator_address
                .as_bytes()
                .try_into()
                .expect("Validator address must be 20 bytes");
            let sig_array: [u8; 64] = signature
                .map(|s| s.as_bytes().try_into().expect("Signature must be 64 bytes"))
                .unwrap_or([0u8; 64]);

            BorshCommitSig::BlockIdFlagNil {
                validator_address: address_array,
                timestamp: time_to_borsh(timestamp),
                signature: sig_array,
            }
        }
    }
}

pub fn commit_to_borsh(c: Commit) -> BorshCommit {
    // Convert and sort signatures by validator address
    // This pre-sorting saves ~60-80k CUs during on-chain deserialization
    // The sort must match the order expected by the verifier's binary search
    let mut signatures: Vec<BorshCommitSig> =
        c.signatures.into_iter().map(commit_sig_to_borsh).collect();

    signatures.sort_unstable_by(|a, b| match (a, b) {
        (
            BorshCommitSig::BlockIdFlagCommit {
                validator_address: addr_a,
                ..
            },
            BorshCommitSig::BlockIdFlagCommit {
                validator_address: addr_b,
                ..
            },
        ) => addr_a.cmp(addr_b),
        (
            BorshCommitSig::BlockIdFlagNil {
                validator_address: addr_a,
                ..
            },
            BorshCommitSig::BlockIdFlagNil {
                validator_address: addr_b,
                ..
            },
        ) => addr_a.cmp(addr_b),
        (
            BorshCommitSig::BlockIdFlagCommit {
                validator_address: addr_a,
                ..
            },
            BorshCommitSig::BlockIdFlagNil {
                validator_address: addr_b,
                ..
            },
        ) => addr_a.cmp(addr_b),
        (
            BorshCommitSig::BlockIdFlagNil {
                validator_address: addr_a,
                ..
            },
            BorshCommitSig::BlockIdFlagCommit {
                validator_address: addr_b,
                ..
            },
        ) => addr_a.cmp(addr_b),
        (BorshCommitSig::BlockIdFlagAbsent, BorshCommitSig::BlockIdFlagAbsent) => {
            std::cmp::Ordering::Equal
        }
        (BorshCommitSig::BlockIdFlagAbsent, _) => std::cmp::Ordering::Less,
        (_, BorshCommitSig::BlockIdFlagAbsent) => std::cmp::Ordering::Greater,
    });

    BorshCommit {
        height: c.height.value(),
        round: c.round.value() as u16,
        block_id: block_id_to_borsh(c.block_id),
        signatures,
    }
}

pub fn consensus_version_to_borsh(v: tendermint::block::header::Version) -> BorshConsensusVersion {
    BorshConsensusVersion {
        block: v.block,
        app: v.app,
    }
}

pub fn block_header_to_borsh(h: TmHeader) -> BorshBlockHeader {
    BorshBlockHeader {
        version: consensus_version_to_borsh(h.version),
        chain_id: h.chain_id.to_string(),
        height: h.height.value(),
        time: time_to_borsh(h.time),
        last_block_id: h.last_block_id.map(block_id_to_borsh),
        last_commit_hash: h.last_commit_hash.map(|h| h.as_bytes().to_vec()),
        data_hash: h.data_hash.map(|h| h.as_bytes().to_vec()),
        validators_hash: h.validators_hash.as_bytes().to_vec(),
        next_validators_hash: h.next_validators_hash.as_bytes().to_vec(),
        consensus_hash: h.consensus_hash.as_bytes().to_vec(),
        app_hash: h.app_hash.as_bytes().to_vec(),
        last_results_hash: h.last_results_hash.map(|h| h.as_bytes().to_vec()),
        evidence_hash: h.evidence_hash.map(|h| h.as_bytes().to_vec()),
        proposer_address: h.proposer_address.as_bytes().to_vec(),
    }
}

pub fn signed_header_to_borsh(sh: SignedHeader) -> BorshSignedHeader {
    BorshSignedHeader {
        header: block_header_to_borsh(sh.header),
        commit: commit_to_borsh(sh.commit),
    }
}

pub fn header_to_borsh(h: Header) -> BorshHeader {
    BorshHeader {
        signed_header: signed_header_to_borsh(h.signed_header),
        validator_set: validator_set_to_borsh(h.validator_set),
        trusted_height: height_to_borsh(h.trusted_height),
        trusted_next_validator_set: validator_set_to_borsh(h.trusted_next_validator_set),
    }
}
