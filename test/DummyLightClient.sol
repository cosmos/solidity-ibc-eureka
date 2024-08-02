// SPDX-License-Identifier: UNLICENSED
pragma solidity >=0.8.25 <0.9.0;

// solhint-disable no-empty-blocks

import { ILightClient } from "../src/interfaces/ILightClient.sol";

contract DummyLightClient is ILightClient {
    UpdateResult public updateResult;
    uint64 public membershipResult;
    bytes public latestUpdateMsg;

    constructor(UpdateResult updateResult_, uint64 membershipResult_) {
        updateResult = updateResult_;
        membershipResult = membershipResult_;
    }

    function updateClient(bytes calldata updateMsg) external returns (UpdateResult) {
        latestUpdateMsg = updateMsg;
        return updateResult;
    }

    function membership(MsgMembership calldata) external view returns (uint256) {
        return membershipResult;
    }

    function misbehaviour(bytes calldata misbehaviourMsg) external { }

    function upgradeClient(bytes calldata upgradeMsg) external { }

    // custom functions to return values we want
    function setUpdateResult(UpdateResult updateResult_) external {
        updateResult = updateResult_;
    }

    function setMembershipResult(uint64 membershipResult_) external {
        membershipResult = membershipResult_;
    }
}
