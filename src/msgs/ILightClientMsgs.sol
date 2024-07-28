// SPDX-License-Identifier: UNLICENSED
pragma solidity >=0.8.25;

import { IICS02ClientMsgs } from "./IICS02ClientMsgs.sol";

interface ILightClientMsgs {
    /// @notice Message for querying the (non)membership of the key-value pairs in the Merkle root at a given height.
    /// @dev If a value is empty, then we are querying for non-membership.
    /// @dev The proof may differ depending on the client implementation and whether
    /// `batchVerifyMembership` or `updateClientAndBatchVerifyMembership` is called.
    struct MsgMembership {
        /// The proof
        bytes proof;
        /// Height of the proof
        IICS02ClientMsgs.Height proofHeight;
        /// Path of the value in the Merkle tree
        bytes path;
        /// Value in the Merkle tree
        bytes value;
    }

    /// @notice The result of an update operation
    enum UpdateResult {
        /// The update was successful
        Update,
        /// A misbehaviour was detected
        Misbehaviour,
        /// Client is already up to date
        NoOp
    }
}
