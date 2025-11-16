//! Conversion functions between `BorshHeader` types and ibc-rs types
//!
//! These conversions are implemented here (instead of in solana-ibc-types)
//! because they require access to ibc-rs dependencies which are not available
//! in the lightweight solana-ibc-types package.

use ibc_client_tendermint::types::Header;
use ibc_core_client_types::Height;
use solana_ibc_types::borsh_header::*;
use tendermint::account::Id as AccountId;
use tendermint::block::parts::Header as PartSetHeader;
use tendermint::block::signed_header::SignedHeader;
use tendermint::block::Id as BlockId;
use tendermint::block::{Commit, CommitSig, Header as TmHeader};
use tendermint::validator::{Info as ValidatorInfo, Set as ValidatorSet};
use tendermint::{Hash, PublicKey, Time};

pub fn borsh_to_height(bh: BorshHeight) -> Result<Height, &'static str> {
    Height::new(bh.revision_number, bh.revision_height).map_err(|_| "Invalid height")
}

pub fn borsh_to_public_key(bpk: BorshPublicKey) -> Result<PublicKey, &'static str> {
    match bpk {
        BorshPublicKey::Ed25519(bytes) => {
            Ok(PublicKey::from_raw_ed25519(&bytes).ok_or("Invalid Ed25519 key")?)
        }
        BorshPublicKey::Secp256k1(_) => Err("Secp256k1 not supported on Solana"),
    }
}

pub fn borsh_to_validator(bv: BorshValidator) -> Result<ValidatorInfo, &'static str> {
    Ok(ValidatorInfo {
        address: AccountId::new(bv.address),
        pub_key: borsh_to_public_key(bv.pub_key)?,
        power: tendermint::vote::Power::try_from(bv.voting_power)
            .map_err(|_| "Invalid voting power")?,
        proposer_priority: tendermint::validator::ProposerPriority::from(bv.proposer_priority),
        name: None,
    })
}

pub fn borsh_to_validator_set(bvs: BorshValidatorSet) -> Result<ValidatorSet, &'static str> {
    let validators: Result<Vec<_>, _> =
        bvs.validators.into_iter().map(borsh_to_validator).collect();
    let validators = validators?;

    let proposer = if let Some(p) = bvs.proposer {
        Some(borsh_to_validator(p)?)
    } else {
        None
    };

    Ok(ValidatorSet::new(validators, proposer))
}

pub fn borsh_to_time(bt: BorshTimestamp) -> Result<Time, &'static str> {
    Time::from_unix_timestamp(bt.secs, bt.nanos as u32).map_err(|_| "Invalid timestamp")
}

pub fn borsh_to_part_set_header(bpsh: BorshPartSetHeader) -> Result<PartSetHeader, &'static str> {
    let hash_bytes: [u8; 32] = bpsh
        .hash
        .try_into()
        .map_err(|_| "Invalid part set header hash length")?;
    let header = PartSetHeader::new(bpsh.total, Hash::Sha256(hash_bytes))
        .map_err(|_| "Invalid part set header")?;

    Ok(header)
}

pub fn borsh_to_block_id(bbid: BorshBlockId) -> Result<BlockId, &'static str> {
    let hash_bytes: [u8; 32] = bbid
        .hash
        .try_into()
        .map_err(|_| "Invalid block ID hash length")?;
    Ok(BlockId {
        hash: Hash::Sha256(hash_bytes),
        part_set_header: borsh_to_part_set_header(bbid.part_set_header)?,
    })
}

pub fn borsh_to_commit_sig(bcs: BorshCommitSig) -> Result<CommitSig, &'static str> {
    match bcs {
        BorshCommitSig::BlockIdFlagAbsent => Ok(CommitSig::BlockIdFlagAbsent),
        BorshCommitSig::BlockIdFlagCommit {
            validator_address,
            timestamp,
            signature,
        } => {
            let sig = tendermint::Signature::new(signature).map_err(|_| "Invalid signature")?;

            Ok(CommitSig::BlockIdFlagCommit {
                validator_address: AccountId::new(validator_address),
                timestamp: borsh_to_time(timestamp)?,
                signature: sig,
            })
        }
        BorshCommitSig::BlockIdFlagNil {
            validator_address,
            timestamp,
            signature,
        } => {
            let sig = tendermint::Signature::new(signature).map_err(|_| "Invalid signature")?;

            Ok(CommitSig::BlockIdFlagNil {
                validator_address: AccountId::new(validator_address),
                timestamp: borsh_to_time(timestamp)?,
                signature: sig,
            })
        }
    }
}

pub fn borsh_to_commit(bc: BorshCommit) -> Result<Commit, &'static str> {
    let signatures: Result<Vec<_>, _> =
        bc.signatures.into_iter().map(borsh_to_commit_sig).collect();

    Ok(Commit {
        height: tendermint::block::Height::try_from(bc.height)
            .map_err(|_| "Invalid commit height")?,
        round: bc.round.into(),
        block_id: borsh_to_block_id(bc.block_id)?,
        signatures: signatures?,
    })
}

pub const fn borsh_to_consensus_version(
    bcv: BorshConsensusVersion,
) -> tendermint::block::header::Version {
    tendermint::block::header::Version {
        block: bcv.block,
        app: bcv.app,
    }
}

pub fn borsh_to_block_header(bbh: BorshBlockHeader) -> Result<TmHeader, &'static str> {
    let last_block_id = if let Some(lbid) = bbh.last_block_id {
        Some(borsh_to_block_id(lbid)?)
    } else {
        None
    };

    let last_commit_hash = if let Some(h) = bbh.last_commit_hash {
        let hash_bytes: [u8; 32] = h
            .try_into()
            .map_err(|_| "Invalid last commit hash length")?;
        Some(Hash::Sha256(hash_bytes))
    } else {
        None
    };

    let data_hash = if let Some(h) = bbh.data_hash {
        let hash_bytes: [u8; 32] = h.try_into().map_err(|_| "Invalid data hash length")?;
        Some(Hash::Sha256(hash_bytes))
    } else {
        None
    };

    let last_results_hash = if let Some(h) = bbh.last_results_hash {
        let hash_bytes: [u8; 32] = h
            .try_into()
            .map_err(|_| "Invalid last results hash length")?;
        Some(Hash::Sha256(hash_bytes))
    } else {
        None
    };

    let evidence_hash = if let Some(h) = bbh.evidence_hash {
        let hash_bytes: [u8; 32] = h.try_into().map_err(|_| "Invalid evidence hash length")?;
        Some(Hash::Sha256(hash_bytes))
    } else {
        None
    };

    let address_bytes: [u8; 20] = bbh
        .proposer_address
        .try_into()
        .map_err(|_| "Invalid proposer address length")?;

    Ok(TmHeader {
        version: borsh_to_consensus_version(bbh.version),
        chain_id: bbh.chain_id.try_into().map_err(|_| "Invalid chain ID")?,
        height: tendermint::block::Height::try_from(bbh.height)
            .map_err(|_| "Invalid header height")?,
        time: borsh_to_time(bbh.time)?,
        last_block_id,
        last_commit_hash,
        data_hash,
        validators_hash: Hash::Sha256(
            bbh.validators_hash
                .try_into()
                .map_err(|_| "Invalid validators hash length")?,
        ),
        next_validators_hash: Hash::Sha256(
            bbh.next_validators_hash
                .try_into()
                .map_err(|_| "Invalid next validators hash length")?,
        ),
        consensus_hash: Hash::Sha256(
            bbh.consensus_hash
                .try_into()
                .map_err(|_| "Invalid consensus hash length")?,
        ),
        app_hash: tendermint::AppHash::try_from(bbh.app_hash).map_err(|_| "Invalid app hash")?,
        last_results_hash,
        evidence_hash,
        proposer_address: AccountId::new(address_bytes),
    })
}

pub fn borsh_to_signed_header(bsh: BorshSignedHeader) -> Result<SignedHeader, &'static str> {
    let header = borsh_to_block_header(bsh.header)?;
    let commit = borsh_to_commit(bsh.commit)?;

    SignedHeader::new(header, commit).map_err(|_| "Failed to create signed header")
}

pub fn borsh_to_header(bh: BorshHeader) -> Result<Header, &'static str> {
    Ok(Header {
        signed_header: borsh_to_signed_header(bh.signed_header)?,
        validator_set: borsh_to_validator_set(bh.validator_set)?,
        trusted_height: borsh_to_height(bh.trusted_height)?,
        trusted_next_validator_set: borsh_to_validator_set(bh.trusted_next_validator_set)?,
    })
}
