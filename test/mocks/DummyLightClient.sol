// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.28;

// solhint-disable no-empty-blocks

import { ILightClient } from "../../contracts/interfaces/ILightClient.sol";

contract DummyLightClient is ILightClient {
    UpdateResult public updateResult;
    uint64 public membershipResult;
    bool public membershipShouldFail;
    bytes public latestUpdateMsg;

    error MembershipShouldFail(string reason);

    constructor(UpdateResult updateResult_, uint64 membershipResult_, bool membershipShouldFail_) {
        updateResult = updateResult_;
        membershipResult = membershipResult_;
        membershipShouldFail = membershipShouldFail_;
    }

    function updateClient(bytes calldata updateMsg) external returns (UpdateResult) {
        latestUpdateMsg = updateMsg;
        return updateResult;
    }

    function membership(MsgMembership calldata) external view returns (uint256) {
        if (membershipShouldFail) {
            revert MembershipShouldFail("membership should fail");
        }
        return membershipResult;
    }

    function misbehaviour(bytes calldata misbehaviourMsg) external { }

    function upgradeClient(bytes calldata upgradeMsg) external { }

    // custom functions to return values we want
    function setUpdateResult(UpdateResult updateResult_) external {
        updateResult = updateResult_;
    }

    function setMembershipResult(uint64 membershipResult_, bool shouldFail) external {
        membershipResult = membershipResult_;
        membershipShouldFail = shouldFail;
    }
}
