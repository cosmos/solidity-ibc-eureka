// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

import { ILightClientMsgs } from "../../msgs/ILightClientMsgs.sol";
import { IICS02ClientMsgs } from "../../msgs/IICS02ClientMsgs.sol";

import { ILightClient } from "../../interfaces/ILightClient.sol";
import { IICS02Precompile } from "./interfaces/IICS02Precompile.sol";

/// @dev The ICS02I contract's address.
address constant ICS02_PRECOMPILE_ADDRESS = 0x0000000000000000000000000000000000000807;

/// @dev The ICS02 contract's instance.
IICS02Precompile constant ICS02_CONTRACT = IICS02Precompile(ICS02_PRECOMPILE_ADDRESS);

contract SP1ICS07Tendermint is ILightClient {
    /// @notice The client identifier of the IBC-Go Light Client
    /// @dev The client-id associated to this light client in solidity-ibc may be different.
    string public GO_CLIENT_ID;

    /// @notice The constructor sets the IBC-Go client identifier
    /// @param goClientId The IBC-Go client identifier
    constructor(string memory goClientId) {
        GO_CLIENT_ID = goClientId;
    }

    /// @inheritdoc ILightClient
    function getClientState() external view returns (bytes memory) {
        return ICS02_CONTRACT.getClientState(GO_CLIENT_ID);
    }

    /// @inheritdoc ILightClient
    function updateClient(bytes calldata updateMsg) external returns (ILightClientMsgs.UpdateResult) {
        IICS02Precompile.UpdateResult result = ICS02_CONTRACT.updateClient(GO_CLIENT_ID, updateMsg);

        if (result == IICS02Precompile.UpdateResult.Update) {
            return ILightClientMsgs.UpdateResult.Update;
        } else if (result == IICS02Precompile.UpdateResult.Misbehaviour) {
            return ILightClientMsgs.UpdateResult.Misbehaviour;
        } else {
            revert("Unknown update result");
        }
    }

    /// @inheritdoc ILightClient
    function verifyMembership(ILightClientMsgs.MsgVerifyMembership calldata msg_) external returns (uint256) {
        return ICS02_CONTRACT.verifyMembership(GO_CLIENT_ID, msg_.proof, msg_.proofHeight, msg_.path, msg_.value);
    }

    /// @inheritdoc ILightClient
    function verifyNonMembership(ILightClientMsgs.MsgVerifyNonMembership calldata msg_) external returns (uint256) {
        return ICS02_CONTRACT.verifyNonMembership(GO_CLIENT_ID, msg_.proof, msg_.proofHeight, msg_.path);
    }

    /// @inheritdoc ILightClient
    function misbehaviour(bytes calldata updateMsg) external {
        IICS02Precompile.UpdateResult result = ICS02_CONTRACT.updateClient(GO_CLIENT_ID, updateMsg);

        if (result != IICS02Precompile.UpdateResult.Misbehaviour) {
            revert("No misbehaviour detected");
        }
    }
}
