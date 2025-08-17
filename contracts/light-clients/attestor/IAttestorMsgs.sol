// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

import { IICS02ClientMsgs } from "../../msgs/IICS02ClientMsgs.sol";

/// @title Attestor Light Client Messages (EVM)
/// @notice ABI message types for the Ethereum attestor light client
interface IAttestorMsgs {
    /// @notice Client state with EVM-native attestor identities
    struct ClientState {
        address[] attestors;
        uint8 minRequiredSigs;
        IICS02ClientMsgs.Height latestHeight;
        bool isFrozen;
    }

    /// @notice Update message carrying attested packet list and signatures
    /// @dev Signatures must be 65-byte ECDSA (r,s,v) over sha256(abi.encode(packets))
    struct MsgUpdateClient {
        uint64 newHeight; // attested height
        uint64 timestamp; // unix seconds
        bytes[] packets;  // attested packets (opaque bytes)
        bytes[] signatures; // 65-byte ECDSA signatures (r,s,v)
    }

    /// @notice Membership proof used by verifyMembership
    /// @dev Signatures must be 65-byte ECDSA (r,s,v) over sha256(abi.encode(packets))
    struct MembershipProof {
        bytes[] packets;    // attested packets (opaque bytes)
        bytes[] signatures; // 65-byte ECDSA signatures (r,s,v)
    }
}


