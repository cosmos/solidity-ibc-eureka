// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.28;

import {Vm} from "forge-std/Vm.sol";
import { stdJson } from "forge-std/StdJson.sol";

library Deployments {
    using stdJson for string;

    struct SP1ICS07TendermintDeployment {
        // The verifier address can be set in the environment variables.
        // If not set, then the verifier is set based on the zkAlgorithm.
        // If set to "mock", then the verifier is set to a mock verifier.
        string verifier;

        bytes trustedClientState;
        bytes trustedConsensusState;
        bytes32 updateClientVkey;
        bytes32 membershipVkey;
        bytes32 ucAndMembershipVkey;
        bytes32 misbehaviourVkey;
    }

    struct ProxiedICS26RouterDeployment {
        address implementation;
        address timeLockAdmin;
    }

    struct ProxiedICS26RouterUpgrade {
        address proxy;
        address newImplementation;

        // we either expect 0 or a ITimeLockController contract
        address payable timeLockAdmin;
    }

    struct ProxiedICS20TransferDeployment {
        address implementation;

        // transparent proxies
        address ics26Router;
        address escrow;
        address ibcERC20;

        // admin control
        address pauserAddress;
        address permit2Address;
    }

    function loadSP1ICS07TendermintDeployment(Vm vm, string memory fileName) public view returns (SP1ICS07TendermintDeployment memory) {
        string memory json = vm.readFile(fileName);
        string memory verifier = json.readStringOr(".verifier", "");
        bytes memory trustedClientState = json.readBytes(".trustedClientState");
        bytes memory trustedConsensusState = json.readBytes(".trustedConsensusState");
        bytes32 updateClientVkey = json.readBytes32(".updateClientVkey");
        bytes32 membershipVkey = json.readBytes32(".membershipVkey");
        bytes32 ucAndMembershipVkey = json.readBytes32(".ucAndMembershipVkey");
        bytes32 misbehaviourVkey = json.readBytes32(".misbehaviourVkey");

        SP1ICS07TendermintDeployment memory fixture = SP1ICS07TendermintDeployment({
            trustedClientState: trustedClientState,
            trustedConsensusState: trustedConsensusState,
            updateClientVkey: updateClientVkey,
            membershipVkey: membershipVkey,
            ucAndMembershipVkey: ucAndMembershipVkey,
            misbehaviourVkey: misbehaviourVkey,
            verifier: verifier
        });

        return fixture;
    }
}

