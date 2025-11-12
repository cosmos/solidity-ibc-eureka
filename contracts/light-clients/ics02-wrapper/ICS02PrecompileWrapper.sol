// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

import { ILightClientMsgs } from "../../msgs/ILightClientMsgs.sol";
import { IICS02ClientMsgs } from "../../msgs/IICS02ClientMsgs.sol";

import { IICS02PrecompileWrapper } from "./interfaces/IICS02PrecompileWrapper.sol";
import { IICS02PrecompileWrapperErrors } from "./errors/IICS02PrecompileWrapperErrors.sol";
import { ILightClient } from "../../interfaces/ILightClient.sol";
import { IICS02Precompile, ICS02_CONTRACT } from "./interfaces/IICS02Precompile.sol";

/// @title ICS02 Precompile Wrapper
/// @notice A wrapper around the ICS02 Precompile contract to implement the ILightClient interface
/// @dev This contract interacts with the ICS02 Precompile contract deployed at a fixed address in 'cosmos/evm'
contract ICS02PrecompileWrapper is ILightClient, IICS02PrecompileWrapper, IICS02PrecompileWrapperErrors {
    /// @inheritdoc IICS02PrecompileWrapper
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
            revert Unreachable();
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
    function misbehaviour(bytes calldata misbehaviourMsg) external {
        IICS02Precompile.UpdateResult result = ICS02_CONTRACT.updateClient(GO_CLIENT_ID, misbehaviourMsg);

        if (result != IICS02Precompile.UpdateResult.Misbehaviour) {
            revert NoMisbehaviourDetected();
        }
    }
}
