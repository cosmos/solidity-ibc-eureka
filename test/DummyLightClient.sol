// SPDX-License-Identifier: UNLICENSED
pragma solidity >=0.8.25 <0.9.0;

// solhint-disable no-empty-blocks

import { ILightClient } from "../src/interfaces/ILightClient.sol";

contract DummyLightClient is ILightClient {
    UpdateResult public updateResult;
    uint32 public membershipResult;

    constructor(UpdateResult updateResult_, uint32 membershipResult_) {
        updateResult = updateResult_;
        membershipResult = membershipResult_;
    }

    function updateClient(bytes calldata) external view returns (UpdateResult) {
        return updateResult;
    }

    function membership(MsgMembership calldata) external view returns (uint256) {
        return membershipResult;
    }

    function misbehaviour(bytes calldata misbehaviourMsg) external { }

    function upgradeClient(bytes calldata upgradeMsg) external { }
}
