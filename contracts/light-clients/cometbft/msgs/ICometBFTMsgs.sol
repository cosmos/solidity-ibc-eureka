// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

import { IICS02ClientMsgs } from "../../../msgs/IICS02ClientMsgs.sol";

/// @title Native CometBFT Light Client Messages
/// @notice ABI message types for the native CometBFT light client.
interface ICometBFTMsgs {
    struct TrustThreshold {
        uint8 numerator;
        uint8 denominator;
    }

    struct ClientState {
        string chainId;
        TrustThreshold trustLevel;
        IICS02ClientMsgs.Height latestHeight;
        uint32 trustingPeriod;
        uint32 unbondingPeriod;
        uint32 maxClockDrift;
        bool isFrozen;
    }

    struct ConsensusState {
        uint128 timestamp;
        bytes32 root;
        bytes32 nextValidatorsHash;
    }

    struct PartSetHeader {
        uint32 total;
        bytes32 hash;
    }

    struct BlockID {
        bytes32 hash;
        PartSetHeader partSetHeader;
    }

    struct Header {
        uint64 versionBlock;
        uint64 versionApp;
        string chainId;
        uint64 height;
        uint64 timeSeconds;
        uint32 timeNanos;
        BlockID lastBlockId;
        bytes32 lastCommitHash;
        bytes32 dataHash;
        bytes32 validatorsHash;
        bytes32 nextValidatorsHash;
        bytes32 consensusHash;
        bytes32 appHash;
        bytes32 lastResultsHash;
        bytes32 evidenceHash;
        address proposerAddress;
    }

    struct CommitSig {
        uint8 blockIdFlag;
        address validatorAddress;
        uint64 timestampSeconds;
        uint32 timestampNanos;
        bytes signature;
    }

    struct Commit {
        uint64 height;
        uint32 round;
        BlockID blockId;
        CommitSig[] signatures;
    }

    struct Validator {
        bytes pubKey;
        bytes32 y;
        uint64 votingPower;
    }

    struct MsgUpdateClient {
        IICS02ClientMsgs.Height trustedHeight;
        ConsensusState trustedConsensusState;
        Header header;
        Commit commit;
        Validator[] validators;
    }
}
