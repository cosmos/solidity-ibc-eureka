// SPDX-License-Identifier: UNLICENSED
pragma solidity >=0.8.25;

import { ILightClientMsgs } from "../msgs/ILightClientMsgs.sol";

/// @title Light Client Interface
/// @notice ILightClient is the light client interface for the IBC Eureka light client
interface ILightClient is ILightClientMsgs {
    /// @notice Updating the client and consensus state
    /// @param updateMsg The update message e.g., an SP1 proof and public value pair.
    /// @return The result of the update operation
    function updateClient(bytes calldata updateMsg) external returns (UpdateResult);

    /// @notice Querying the (non)membership of the key-value pairs
    /// @return The unix timestamp of the verification height in the counterparty chain in seconds.
    function verifyMembership(MsgMembership calldata msg_) external view returns (uint32);

    /// @notice Misbehaviour handling, moves the light client to the frozen state if misbehaviour is detected
    /// @param misbehaviourMsg The misbehaviour message
    function misbehaviour(bytes calldata misbehaviourMsg) external;
}
