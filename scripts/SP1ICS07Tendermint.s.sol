// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.28;

// solhint-disable gas-custom-errors

import { IICS07TendermintMsgs } from "../contracts/light-clients/msgs/IICS07TendermintMsgs.sol";
import { SP1ICS07Tendermint } from "../contracts/light-clients/SP1ICS07Tendermint.sol";
import { Script } from "forge-std/Script.sol";
import { stdJson } from "forge-std/StdJson.sol";
import { Deployments } from "./helpers/Deployments.sol";

contract SP1TendermintScript is Script, IICS07TendermintMsgs, Deployments {
    using stdJson for string;

    address public verifier;
    SP1ICS07Tendermint public ics07Tendermint;

    string internal constant SP1_GENESIS_DIR = "/scripts/";

    // Deploy the SP1 Tendermint contract with the supplied initialization parameters.
    function run() public returns (address) {
        ConsensusState memory trustedConsensusState;
        ClientState memory trustedClientState;

        // Read the initialization parameters for the SP1 Tendermint contract.
        string memory root = vm.projectRoot();
        string memory path = string.concat(root, SP1_GENESIS_DIR, "genesis.json");
        string memory json = vm.readFile(path);

        SP1ICS07TendermintDeployment memory genesis = loadSP1ICS07TendermintDeployment(json, "");
        genesis.verifier = vm.envOr("VERIFIER", string(""));

        vm.startBroadcast();
        IICS07TendermintMsgs.ConsensusState memory trustedConsensusState =
                            abi.decode(genesis.trustedConsensusState, (IICS07TendermintMsgs.ConsensusState));
        IICS07TendermintMsgs.ClientState memory trustedClientState =
                            abi.decode(genesis.trustedClientState, (IICS07TendermintMsgs.ClientState));

        address verifier = address(0);

        if (keccak256(bytes(genesis.verifier)) == keccak256(bytes("mock"))) {
            verifier = address(new SP1MockVerifier());
        } else if (bytes(genesis.verifier).length > 0) {
            (bool success, address verifierAddr) = Strings.tryParseAddress(genesis.verifier);
            require(success, string.concat("Invalid verifier address: ", genesis.verifier));

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
            genesis.updateClientVkey,
            genesis.membershipVkey,
            genesis.ucAndMembershipVkey,
            genesis.misbehaviourVkey,
            verifier,
            genesis.trustedClientState,
            keccak256(abi.encode(trustedConsensusState))
        );


        vm.stopBroadcast();

        bytes memory clientStateBz = ics07Tendermint.getClientState();
        assert(keccak256(clientStateBz) == keccak256(genesis.trustedClientState));

        ClientState memory clientState = abi.decode(clientStateBz, (ClientState));
        bytes32 consensusHash = ics07Tendermint.getConsensusStateHash(clientState.latestHeight.revisionHeight);
        assert(consensusHash == keccak256(abi.encode(trustedConsensusState)));

        return address(ics07Tendermint);
    }
}
