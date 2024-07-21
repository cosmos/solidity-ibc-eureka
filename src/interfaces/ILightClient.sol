// SPDX-License-Identifier: UNLICENSED
pragma solidity >=0.8.25;

import { IICS02Client } from "./IICS02Client.sol";

// @title Light Client Interface
// @notice ILightClient is the light client interface for the IBC Eureka light client
interface ILightClient {
    /// @notice The key-value pair.
    struct KVPair {
        bytes path;
        bytes value;
    }

    // @notice Initializes the light client with a trusted client state and consensus state
    // @dev Should be used in the constructor of the light client contract
    struct MsgInitialize {
        /// Initial client state
        bytes clientState;
        /// Initial consensus state
        bytes consensusState;
    }

    // @notice Message for querying the (non)membership of the key-value pairs in the Merkle root at a given height.
    // @dev If a value is empty, then we are querying for non-membership.
    // @dev The proof may differ depending on the client implementation and whether
    // `batchVerifyMembership` or `updateClientAndBatchVerifyMembership` is called.
    struct MsgBatchMembership {
        /// The proof
        bytes proof;
        /// Height of the proof
        IICS02Client.Height proofHeight;
        /// The key-value pairs
        KVPair[] keyValues;
    }

    /// The result of an update operation
    enum UpdateResult {
        /// The update was successful
        Update,
        /// A misbehaviour was detected
        Misbehaviour,
        /// Client is already up to date
        NoOp
    }

    // @notice Updating the client and consensus state
    // @param updateMsg The update message e.g., an SP1 proof and public value pair.
    // @return The result of the update operation
    function updateClient(bytes calldata updateMsg) external returns (UpdateResult);

    // @notice Querying the (non)membership of the key-value pairs
    // @returns The timestamp of the verification height in the counterparty chain, e.g., unix timestamp in seconds.
    function batchVerifyMembership(MsgBatchMembership calldata batchVerifyMsg) external view returns (uint32);

    // @notice Updating the client and querying the (non)membership of the key-value pairs on the updated consensus
    // state.
    // @returns The timestamp of the verification height in the counterparty chain
    // and the result of the update operation.
    function updateClientAndBatchVerifyMembership(MsgBatchMembership calldata batchVerifyMsg)
        external
        returns (uint32, UpdateResult);

    // @notice Misbehaviour handling, moves the light client to the frozen state if misbehaviour is detected
    // @param misbehaviourMsg The misbehaviour message
    function misbehaviour(bytes calldata misbehaviourMsg) external;
}
