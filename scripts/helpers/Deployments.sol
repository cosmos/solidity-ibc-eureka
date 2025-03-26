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
        string clientId;
        string counterpartyClientId;
        string verifier;
        string[] merklePrefix;
        bytes trustedClientState;
        bytes trustedConsensusState;
        bytes32 updateClientVkey;
        bytes32 membershipVkey;
        bytes32 ucAndMembershipVkey;
        bytes32 misbehaviourVkey;
        address proofSubmitter;
    }

    function loadSP1ICS07TendermintDeployment(
        string memory json,
        string memory key,
        address defaultProofSubmitter
    )
    public
    view
    returns (SP1ICS07TendermintDeployment memory) {
        return SP1ICS07TendermintDeployment({
            clientId: json.readStringOr(string.concat(key, ".clientId"), ""),
            verifier: json.readStringOr(string.concat(key, ".verifier"), ""),
            merklePrefix: json.readStringArrayOr(string.concat(key, ".merklePrefix"), new string[](0)),
            counterpartyClientId: json.readStringOr(string.concat(key, ".counterpartyClientId"), ""),
            implementation: json.readAddressOr(string.concat(key, ".implementation"), address(0)),
            trustedClientState: json.readBytes(string.concat(key, ".trustedClientState")),
            trustedConsensusState: json.readBytes(string.concat(key, ".trustedConsensusState")),
            updateClientVkey: json.readBytes32(string.concat(key, ".updateClientVkey")),
            membershipVkey: json.readBytes32(string.concat(key, ".membershipVkey")),
            ucAndMembershipVkey: json.readBytes32(string.concat(key, ".ucAndMembershipVkey")),
            misbehaviourVkey: json.readBytes32(string.concat(key, ".misbehaviourVkey")),
            proofSubmitter: json.readAddressOr(string.concat(key, ".proofSubmitter"), defaultProofSubmitter)
        });
    }

    // TODO: Move these to ops repo
    function loadSP1ICS07TendermintDeployments(
        Vm vm,
        string memory json,
        address defaultProofSubmitter
    )
    public
    view
    returns (SP1ICS07TendermintDeployment[] memory)
    {
        string[] memory keys = vm.parseJsonKeys(json, "$.light_clients");
        SP1ICS07TendermintDeployment[] memory deployments = new SP1ICS07TendermintDeployment[](keys.length);

        for (uint256 i = 0; i < keys.length; i++) {
            string memory key = string.concat(".light_clients['", keys[i], "']");
            deployments[i] = loadSP1ICS07TendermintDeployment(json, key, defaultProofSubmitter);
        }

        return deployments;
    }

    struct ProxiedICS26RouterDeployment {
        address implementation;
        address proxy;
        address timeLockAdmin;
        address portCustomizer;
        address clientIdCustomizer;
        address[] relayers;
    }

    function loadProxiedICS26RouterDeployment(
        Vm vm,
        string memory json
    )
    public
    pure
    returns (ProxiedICS26RouterDeployment memory)
    {
        ProxiedICS26RouterDeployment memory fixture = ProxiedICS26RouterDeployment({
            implementation: vm.parseJsonAddress(json, ".ics26Router.implementation"),
            proxy: vm.parseJsonAddress(json, ".ics26Router.proxy"),
            timeLockAdmin: vm.parseJsonAddress(json, ".ics26Router.timeLockAdmin"),
            portCustomizer: vm.parseJsonAddress(json, ".ics26Router.portCustomizer"),
            clientIdCustomizer: vm.parseJsonAddress(json, ".ics26Router.clientIdCustomizer"),
            relayers: vm.parseJsonAddressArray(json, ".ics26Router.relayers")
        });

        return fixture;
    }

    struct ProxiedICS20TransferDeployment {
        // transparant proxies
        address ics26Router;

        // implementation addresses
        address implementation;
        address escrowImplementation;
        address ibcERC20Implementation;

        // admin control
        address[] pausers;
        address[] unpausers;
        address tokenOperator;
        address permit2;
        address proxy;
    }

    // TODO: Move these to ops repo
    function loadProxiedICS20TransferDeployment(
        Vm vm,
        string memory json
    )
    public
    pure
    returns (ProxiedICS20TransferDeployment memory)
    {
        // abi.decode(vm.parseJson(json, ".ics20Transfer"), (ProxiedICS20TransferDeployment));
        ProxiedICS20TransferDeployment memory fixture = ProxiedICS20TransferDeployment({
            escrowImplementation: vm.parseJsonAddress(json, ".ics20Transfer.escrowImplementation"),
            ibcERC20Implementation: vm.parseJsonAddress(json, ".ics20Transfer.ibcERC20Implementation"),
            ics26Router: vm.parseJsonAddress(json, ".ics20Transfer.ics26Router"),
            implementation: vm.parseJsonAddress(json, ".ics20Transfer.implementation"),
            pausers: vm.parseJsonAddressArray(json, ".ics20Transfer.pausers"),
            unpausers: vm.parseJsonAddressArray(json, ".ics20Transfer.unpausers"),
            tokenOperator: vm.parseJsonAddress(json, ".ics20Transfer.tokenOperator"),
            permit2: vm.parseJsonAddress(json, ".ics20Transfer.permit2"),
            proxy: vm.parseJsonAddress(json, ".ics20Transfer.proxy")
        });

        return fixture;
    }
}
