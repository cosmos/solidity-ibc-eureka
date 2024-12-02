// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.28;

import { Script } from "forge-std/Script.sol";
import { stdJson } from "forge-std/StdJson.sol";
import { SP1ICS07Tendermint } from "../contracts/light-clients/SP1ICS07Tendermint.sol";
import { IICS07TendermintMsgs } from "../contracts/light-clients/msgs/IICS07TendermintMsgs.sol";

struct SP1ICS07TendermintGenesisJson {
    bytes trustedClientState;
    bytes trustedConsensusState;
    bytes32 updateClientVkey;
    bytes32 membershipVkey;
    bytes32 ucAndMembershipVkey;
    bytes32 misbehaviourVkey;
}

contract SP1TendermintScript is Script, IICS07TendermintMsgs {
    using stdJson for string;

    SP1ICS07Tendermint public ics07Tendermint;

    string internal constant SP1_GENESIS_DIR = "/scripts/";

    // Deploy the SP1 Tendermint contract with the supplied initialization parameters.
    function run() public returns (address) {
        // Read the initialization parameters for the SP1 Tendermint contract.
        SP1ICS07TendermintGenesisJson memory genesis = loadGenesis("genesis.json");

        ConsensusState memory trustedConsensusState = abi.decode(genesis.trustedConsensusState, (ConsensusState));

        bytes32 trustedConsensusHash = keccak256(abi.encode(trustedConsensusState));

        vm.startBroadcast();

        ics07Tendermint = new SP1ICS07Tendermint(
            genesis.updateClientVkey,
            genesis.membershipVkey,
            genesis.ucAndMembershipVkey,
            genesis.misbehaviourVkey,
            genesis.trustedClientState,
            trustedConsensusHash
        );

        vm.stopBroadcast();

        ClientState memory clientState = ics07Tendermint.getClientState();
        assert(keccak256(abi.encode(clientState)) == keccak256(genesis.trustedClientState));

        bytes32 consensusHash = ics07Tendermint.getConsensusStateHash(clientState.latestHeight.revisionHeight);
        assert(consensusHash == keccak256(abi.encode(trustedConsensusState)));

        return address(ics07Tendermint);
    }

    function loadGenesis(string memory fileName) public view returns (SP1ICS07TendermintGenesisJson memory) {
        string memory root = vm.projectRoot();
        string memory path = string.concat(root, SP1_GENESIS_DIR, fileName);
        string memory json = vm.readFile(path);
        bytes memory trustedClientState = json.readBytes(".trustedClientState");
        bytes memory trustedConsensusState = json.readBytes(".trustedConsensusState");
        bytes32 updateClientVkey = json.readBytes32(".updateClientVkey");
        bytes32 membershipVkey = json.readBytes32(".membershipVkey");
        bytes32 ucAndMembershipVkey = json.readBytes32(".ucAndMembershipVkey");
        bytes32 misbehaviourVkey = json.readBytes32(".misbehaviourVkey");

        SP1ICS07TendermintGenesisJson memory fixture = SP1ICS07TendermintGenesisJson({
            trustedClientState: trustedClientState,
            trustedConsensusState: trustedConsensusState,
            updateClientVkey: updateClientVkey,
            membershipVkey: membershipVkey,
            ucAndMembershipVkey: ucAndMembershipVkey,
            misbehaviourVkey: misbehaviourVkey
        });

        return fixture;
    }
}
