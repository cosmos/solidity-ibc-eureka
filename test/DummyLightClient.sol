// SPDX-License-Identifier: UNLICENSED
pragma solidity >=0.8.25 <0.9.0;

import {ILightClient} from "../src/interfaces/ILightClient.sol";

contract DummyLightClient is ILightClient {
    UpdateResult public updateResult;
    uint32 public membershipResult;

    constructor(UpdateResult updateResult_, uint32 membershipResult_) {
        updateResult = updateResult_;
        membershipResult = membershipResult_;
    }

    function updateClient(bytes calldata updateMsg) external returns (UpdateResult) {
        return updateResult;
    }

    function membership(MsgMembership calldata msg_) external view returns (uint32) {
        return membershipResult;
    }

    function misbehaviour(bytes calldata misbehaviourMsg) external {}

    function upgradeClient(bytes calldata upgradeMsg) external {}
}