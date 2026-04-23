// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

import { IICS02ClientMsgs } from "../../../msgs/IICS02ClientMsgs.sol";

interface IBesuLightClientMsgs {
    struct ClientState {
        address ibcRouter;
        IICS02ClientMsgs.Height latestHeight;
        uint64 trustingPeriod;
        uint64 maxClockDrift;
    }

    struct ConsensusState {
        uint64 timestamp;
        bytes32 storageRoot;
        address[] validators;
    }

    struct MsgUpdateClient {
        bytes headerRlp;
        IICS02ClientMsgs.Height trustedHeight;
        bytes accountProof;
    }
}
