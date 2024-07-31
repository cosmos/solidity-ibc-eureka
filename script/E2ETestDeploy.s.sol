// SPDX-License-Identifier: UNLICENSED
pragma solidity >=0.8.25 <0.9.0;

import { stdJson } from "forge-std/StdJson.sol";
import { Script } from "forge-std/Script.sol";
import { SP1ICS07Tendermint } from "@cosmos/sp1-ics07-tendermint/SP1ICS07Tendermint.sol";
import { SP1Verifier } from "@sp1-contracts/v1.0.1/SP1Verifier.sol";

struct SP1ICS07TendermintGenesisJson {
    bytes trustedClientState;
    bytes trustedConsensusState;
    bytes32 updateClientVkey;
    bytes32 membershipVkey;
    bytes32 ucAndMembershipVkey;
}

/// @dev See the Solidity Scripting tutorial: https://book.getfoundry.sh/tutorials/solidity-scripting
contract E2ETestDeploy is Script {
    using stdJson for string;

    // solhint-disable-next-line no-empty-blocks
    function run() public returns (address, address, address, address, address) {
        vm.startBroadcast();

        // TODO: Implement the deployment script here.
        SP1Verifier verifier = new SP1Verifier();
        SP1ICS07Tendermint ics07Tendermint = new SP1ICS07Tendermint(0, 0, 0, address(verifier), bytes(""), 0);

        vm.stopBroadcast();

        return (address(ics07Tendermint), address(0), address(0), address(0), address(0));
    }

    function loadGenesis(string memory fileName) public view returns (SP1ICS07TendermintGenesisJson memory) {
        string memory root = vm.projectRoot();
        string memory path = string.concat(root, "/contracts/script/", fileName);
        string memory json = vm.readFile(path);
        bytes memory trustedClientState = json.readBytes(".trustedClientState");
        bytes memory trustedConsensusState = json.readBytes(".trustedConsensusState");
        bytes32 updateClientVkey = json.readBytes32(".updateClientVkey");
        bytes32 membershipVkey = json.readBytes32(".membershipVkey");
        bytes32 ucAndMembershipVkey = json.readBytes32(".ucAndMembershipVkey");

        SP1ICS07TendermintGenesisJson memory fixture = SP1ICS07TendermintGenesisJson({
            trustedClientState: trustedClientState,
            trustedConsensusState: trustedConsensusState,
            updateClientVkey: updateClientVkey,
            membershipVkey: membershipVkey,
            ucAndMembershipVkey: ucAndMembershipVkey
        });

        return fixture;
    }
}
