// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.28;

import { Vm } from "forge-std/Vm.sol";
import { stdJson } from "forge-std/StdJson.sol";

abstract contract Deployments {
    using stdJson for string;

    string internal constant DEPLOYMENT_DIR = "/deployments/";

    struct SP1ICS07TendermintDeployment {
        // The verifier address can be set in the environment variables.
        // If not set, then the verifier is set based on the zkAlgorithm.
        // If set to "mock", then the verifier is set to a mock verifier.
        address implementation;
        string counterpartyClientId;
        string verifier;
        bytes[] merklePrefix;
        bytes trustedClientState;
        bytes trustedConsensusState;
        bytes32 updateClientVkey;
        bytes32 membershipVkey;
        bytes32 ucAndMembershipVkey;
        bytes32 misbehaviourVkey;
    }

    function loadSP1ICS07TendermintDeployments(
        Vm vm,
        string memory json
    )
    public
    view
    returns (SP1ICS07TendermintDeployment[] memory)
    {
        string[] memory keys = vm.parseJsonKeys(json, "$.light_clients");
        SP1ICS07TendermintDeployment[] memory deployments = new SP1ICS07TendermintDeployment[](keys.length);

        for (uint256 i = 0; i < keys.length; i++) {
            string memory key = string.concat(".light_clients['", keys[i], "']");

            SP1ICS07TendermintDeployment memory fixture = SP1ICS07TendermintDeployment({
                merklePrefix: json.readBytesArray(string.concat(key, ".merklePrefix")),
                counterpartyClientId: json.readString(string.concat(key, ".counterpartyClientId")),
                implementation: json.readAddressOr(string.concat(key, ".implementation"), address(0)),
                trustedClientState: json.readBytes(string.concat(key, ".trustedClientState")),
                trustedConsensusState: json.readBytes(string.concat(key, ".trustedConsensusState")),
                updateClientVkey: json.readBytes32(string.concat(key, ".updateClientVkey")),
                membershipVkey: json.readBytes32(string.concat(key, ".membershipVkey")),
                ucAndMembershipVkey: json.readBytes32(string.concat(key, ".ucAndMembershipVkey")),
                misbehaviourVkey: json.readBytes32(string.concat(key, ".misbehaviourVkey")),
                verifier: json.readStringOr(string.concat(key, ".verifier"), "")
            });

            deployments[i] = fixture;
        }

        return deployments;
    }

    struct ProxiedICS26RouterDeployment {
        address implementation;
        address payable proxy;
        address timeLockAdmin;
    }

    function loadProxiedICS26RouterDeployment(
        Vm vm,
        string memory json
    )
    public
    pure
    returns (ProxiedICS26RouterDeployment memory)
    {
        ProxiedICS26RouterDeployment memory fixture = abi.decode(vm.parseJson(json, ".ics26Router"), (ProxiedICS26RouterDeployment));

        return fixture;
    }

    struct ProxiedICS20TransferDeployment {
        // transparent proxies
        address escrow;

        address ibcERC20;
        address ics26Router;
        address implementation;

        // admin control
        address pauser;
        address permit2;
        address payable proxy;
    }

    function loadProxiedICS20TransferDeployment(
        Vm vm,
        string memory json
    )
    public
    pure
    returns (ProxiedICS20TransferDeployment memory)
    {
        ProxiedICS20TransferDeployment memory fixture = abi.decode(vm.parseJson(json, ".ics20Transfer"), (ProxiedICS20TransferDeployment));

        return fixture;
    }
}
