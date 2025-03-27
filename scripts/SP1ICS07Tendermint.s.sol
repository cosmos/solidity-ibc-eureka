// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.28;

// solhint-disable gas-custom-errors

import { IICS07TendermintMsgs } from "../contracts/light-clients/msgs/IICS07TendermintMsgs.sol";
import { SP1ICS07Tendermint } from "../contracts/light-clients/SP1ICS07Tendermint.sol";
import { Script } from "forge-std/Script.sol";
import { stdJson } from "forge-std/StdJson.sol";
import { DeploySP1ICS07Tendermint } from "./deployments/DeploySP1ICS07Tendermint.sol";

contract SP1TendermintScript is Script, IICS07TendermintMsgs, DeploySP1ICS07Tendermint {
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
        SP1ICS07TendermintDeployment memory genesis = loadSP1ICS07TendermintDeployment(json, "", address(0));
        genesis.verifier = vm.envOr("VERIFIER", string(""));

        vm.startBroadcast();
        (ics07Tendermint, trustedConsensusState, trustedClientState) = deploySP1ICS07Tendermint(genesis);
        vm.stopBroadcast();

        bytes memory clientStateBz = ics07Tendermint.getClientState();
        assert(keccak256(clientStateBz) == keccak256(genesis.trustedClientState));

        ClientState memory clientState = abi.decode(clientStateBz, (ClientState));
        bytes32 consensusHash = ics07Tendermint.getConsensusStateHash(clientState.latestHeight.revisionHeight);
        assert(consensusHash == keccak256(abi.encode(trustedConsensusState)));

        return address(ics07Tendermint);
    }
}
