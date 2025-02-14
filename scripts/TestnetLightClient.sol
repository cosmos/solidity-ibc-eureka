// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.28;

// solhint-disable no-empty-blocks

import { ILightClientMsgs } from "../contracts/msgs/ILightClientMsgs.sol";
import { ILightClient } from "../contracts/interfaces/ILightClient.sol";

contract TestnetLightClient is ILightClient, ILightClientMsgs {

    function updateClient(bytes calldata) external pure returns (UpdateResult) {
        return UpdateResult.Update;
    }

    function membership(MsgMembership calldata) external pure returns (uint256) {
        return 42;
    }

    function misbehaviour(bytes calldata) external { }

    function upgradeClient(bytes calldata) external { }

    function getClientState() external pure returns (bytes memory) {
        return bytes("");
    }
}
