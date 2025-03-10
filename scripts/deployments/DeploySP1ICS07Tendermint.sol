// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.28;

// solhint-disable custom-errors,gas-custom-errors

import { Deployments } from "../helpers/Deployments.sol";
import { SP1ICS07Tendermint } from "../../contracts/light-clients/SP1ICS07Tendermint.sol";
import { ISP1ICS07Tendermint } from "../../contracts/light-clients/ISP1ICS07Tendermint.sol";
import { stdJson } from "forge-std/StdJson.sol";
import { SP1Verifier as SP1VerifierPlonk } from "@sp1-contracts/v4.0.0-rc.3/SP1VerifierPlonk.sol";
import { SP1Verifier as SP1VerifierGroth16 } from "@sp1-contracts/v4.0.0-rc.3/SP1VerifierGroth16.sol";
import { SP1MockVerifier } from "@sp1-contracts/SP1MockVerifier.sol";
import { IICS02ClientMsgs } from "../../contracts/msgs/IICS02ClientMsgs.sol";
import { IICS02Client } from "../../contracts/interfaces/IICS02Client.sol";
import { IICS07TendermintMsgs } from "../../contracts/light-clients/msgs/IICS07TendermintMsgs.sol";
import { Strings } from "@openzeppelin-contracts/utils/Strings.sol";
import { Script } from "forge-std/Script.sol";

abstract contract DeploySP1ICS07Tendermint is Deployments {
    using stdJson for string;

    function deploySP1ICS07Tendermint(SP1ICS07TendermintDeployment memory deployment)
        public
        returns (
            SP1ICS07Tendermint,
            IICS07TendermintMsgs.ConsensusState memory,
            IICS07TendermintMsgs.ClientState memory
        )
    {
        IICS07TendermintMsgs.ConsensusState memory trustedConsensusState =
            abi.decode(deployment.trustedConsensusState, (IICS07TendermintMsgs.ConsensusState));
        IICS07TendermintMsgs.ClientState memory trustedClientState =
            abi.decode(deployment.trustedClientState, (IICS07TendermintMsgs.ClientState));

        address verifier = address(0);

        if (keccak256(bytes(deployment.verifier)) == keccak256(bytes("mock"))) {
            verifier = address(new SP1MockVerifier());
        } else if (bytes(deployment.verifier).length > 0) {
            (bool success, address verifierAddr) = Strings.tryParseAddress(deployment.verifier);
            require(success, string.concat("Invalid verifier address: ", deployment.verifier));

            if (verifierAddr == address(0)) {
                revert("Verifier address is zero");
            }

            verifier = verifierAddr;
        } else if (trustedClientState.zkAlgorithm == IICS07TendermintMsgs.SupportedZkAlgorithm.Plonk) {
            verifier = address(new SP1VerifierPlonk());
        } else if (trustedClientState.zkAlgorithm == IICS07TendermintMsgs.SupportedZkAlgorithm.Groth16) {
            verifier = address(new SP1VerifierGroth16());
        } else {
            revert("Unsupported zk algorithm");
        }

        // Deploy the SP1 ICS07 Tendermint light client
        SP1ICS07Tendermint ics07Tendermint = new SP1ICS07Tendermint(
            deployment.updateClientVkey,
            deployment.membershipVkey,
            deployment.ucAndMembershipVkey,
            deployment.misbehaviourVkey,
            verifier,
            deployment.trustedClientState,
            keccak256(abi.encode(trustedConsensusState))
        );

        return (ics07Tendermint, trustedConsensusState, trustedClientState);
    }
}

contract DeploySP1ICS07TendermintScript is DeploySP1ICS07Tendermint, Script {
    string internal constant DEPLOYMENT_NAME = "SP1ICS07Tendermint.json";

    function verify(
        SP1ICS07TendermintDeployment memory deployment,
        ProxiedICS26RouterDeployment memory ics26RouterDeployment
    ) internal view {
        vm.assertNotEq(deployment.implementation, address(0), "implementation address is zero");

        ISP1ICS07Tendermint ics07Tendermint = ISP1ICS07Tendermint(deployment.implementation);
        address actualVerifierAddress = address(ics07Tendermint.VERIFIER());

        (bool success, address verifierAddr) = Strings.tryParseAddress(deployment.verifier);

        vm.assertTrue(
            success,
            string.concat(
                "Invalid verifier address: ",
                deployment.verifier,
                " (actual address: ",
                vm.toString(actualVerifierAddress),
                ")"
            )
        );

        vm.assertEq(
            address(ics07Tendermint.VERIFIER()),
            verifierAddr,
            "verifier address doesn't match"
        );

        vm.assertEq(
            ics07Tendermint.MEMBERSHIP_PROGRAM_VKEY(),
            deployment.membershipVkey,
            "membershipVkey doesn't match"
        );

        vm.assertEq(
            ics07Tendermint.MISBEHAVIOUR_PROGRAM_VKEY(),
            deployment.misbehaviourVkey,
            "misbehaviourVkey doesn't match"
        );

        vm.assertEq(
            ics07Tendermint.UPDATE_CLIENT_PROGRAM_VKEY(),
            deployment.updateClientVkey,
            "updateClientVkey doesn't match"
        );
        vm.assertEq(
            ics07Tendermint.UPDATE_CLIENT_AND_MEMBERSHIP_PROGRAM_VKEY(),
            deployment.ucAndMembershipVkey,
            "ucAndMembershipVkey doesn't match"
        );

        IICS02Client router = IICS02Client(ics26RouterDeployment.proxy);

        vm.assertEq(
            address(router.getClient(deployment.clientId)),
            deployment.implementation,
            "address of clientId in ics26Router doesn't match implementation address"
        );

        IICS02ClientMsgs.CounterpartyInfo memory counterparty = router.getCounterparty(deployment.clientId);

        for (uint256 i = 0; i < counterparty.merklePrefix.length; i++) {
            vm.assertEq(
                counterparty.merklePrefix[i],
                bytes(deployment.merklePrefix[i]),
                "merklePrefix doesn't match"
            );
        }

        vm.assertEq(
            counterparty.clientId,
            deployment.counterpartyClientId,
            "counterpartyClientId doesn't match"
        );
    }

    function run() public {
        string memory root = vm.projectRoot();
        string memory deployEnv = vm.envString("DEPLOYMENT_ENV");
        string memory path = string.concat(root, DEPLOYMENT_DIR, "/", deployEnv, "/", Strings.toString(block.chainid), ".json");
        string memory json = vm.readFile(path);

        bool verifyOnly = vm.envOr("VERIFY_ONLY", false);

        SP1ICS07TendermintDeployment[] memory deployments = loadSP1ICS07TendermintDeployments(vm, json);
        ProxiedICS26RouterDeployment memory ics26RouterDeployment = loadProxiedICS26RouterDeployment(vm, json);

        IICS02Client ics26Router = IICS02Client(ics26RouterDeployment.proxy);

        for (uint256 i = 0; i < deployments.length; i++) {
            if (deployments[i].implementation != address(0) || verifyOnly) {
                verify(deployments[i], ics26RouterDeployment);
                continue;
            }

            vm.startBroadcast();

            (SP1ICS07Tendermint ics07Tendermint, ,) = deploySP1ICS07Tendermint(deployments[i]);

            deployments[i].implementation = address(ics07Tendermint);
            deployments[i].verifier = vm.toString(address(ics07Tendermint.VERIFIER()));

            bytes[] memory merklePrefix = new bytes[](deployments[i].merklePrefix.length);
            for (uint256 j = 0; j < deployments[i].merklePrefix.length; j++) {
                merklePrefix[j] = bytes(deployments[i].merklePrefix[j]);
            }
            IICS02ClientMsgs.CounterpartyInfo memory counterPartyInfo = IICS02ClientMsgs.CounterpartyInfo(deployments[i].counterpartyClientId, merklePrefix);
            if (bytes(deployments[i].clientId).length == 0) {
                deployments[i].clientId = ics26Router.addClient(counterPartyInfo, address(ics07Tendermint));
            } else {
                ics26Router.addClient(deployments[i].clientId, counterPartyInfo, address(ics07Tendermint));
            }

            vm.stopBroadcast();
        }

        for (uint256 i = 0; i < deployments.length; ++i) {
            verify(deployments[i], ics26RouterDeployment);
        }

        for (uint256 i = 0; i < deployments.length; ++i) {
            string memory idx = Strings.toString(i);
            string memory key = string.concat(".light_clients['", idx, "']");

            vm.writeJson(deployments[i].clientId, path, string.concat(key, ".clientId"));
            vm.writeJson(vm.toString(deployments[i].implementation), path, string.concat(key, ".implementation"));
            vm.writeJson(deployments[i].verifier, path, string.concat(key, ".verifier"));
        }
    }
}
