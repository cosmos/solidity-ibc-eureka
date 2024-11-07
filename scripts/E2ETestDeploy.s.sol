// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.28;

/*
    This script is used for end-to-end testing with SP1_PROVER=network.
*/

// solhint-disable gas-custom-errors,custom-errors

import { stdJson } from "forge-std/StdJson.sol";
import { Script } from "forge-std/Script.sol";
import { SP1ICS07Tendermint } from "@cosmos/sp1-ics07-tendermint/SP1ICS07Tendermint.sol";
import { IICS07TendermintMsgs } from "@cosmos/sp1-ics07-tendermint/msgs/IICS07TendermintMsgs.sol";
import { ICS02Client } from "../src/ICS02Client.sol";
import { ICS26Router } from "../src/ICS26Router.sol";
import { ICS20Transfer } from "../src/ICS20Transfer.sol";
import { TestERC20 } from "../test/mocks/TestERC20.sol";
import { Strings } from "@openzeppelin/utils/Strings.sol";
import { ICS20Lib } from "../src/utils/ICS20Lib.sol";

struct SP1ICS07TendermintGenesisJson {
    bytes trustedClientState;
    bytes trustedConsensusState;
    bytes32 updateClientVkey;
    bytes32 membershipVkey;
    bytes32 ucAndMembershipVkey;
    bytes32 misbehaviourVkey;
}

/// @dev See the Solidity Scripting tutorial: https://book.getfoundry.sh/tutorials/solidity-scripting
contract E2ETestDeploy is Script {
    using stdJson for string;

    function run() public returns (string memory) {
        // Read the initialization parameters for the SP1 Tendermint contract.
        SP1ICS07TendermintGenesisJson memory genesis = loadGenesis("genesis.json");
        IICS07TendermintMsgs.ConsensusState memory trustedConsensusState =
            abi.decode(genesis.trustedConsensusState, (IICS07TendermintMsgs.ConsensusState));

        string memory e2eFaucet = vm.envString("E2E_FAUCET_ADDRESS");
        uint256 privateKey = vm.envUint("PRIVATE_KEY");

        vm.startBroadcast(privateKey);

        // Deploy the SP1 ICS07 Tendermint light client
        SP1ICS07Tendermint ics07Tendermint = new SP1ICS07Tendermint(
            genesis.updateClientVkey,
            genesis.membershipVkey,
            genesis.ucAndMembershipVkey,
            genesis.misbehaviourVkey,
            genesis.trustedClientState,
            keccak256(abi.encode(trustedConsensusState))
        );

        // Deploy IBC Eureka
        ICS26Router ics26Router = new ICS26Router(msg.sender);
        ICS20Transfer ics20Transfer = new ICS20Transfer(address(ics26Router));
        TestERC20 erc20 = new TestERC20();

        // Wire Transfer app
        ics26Router.addIBCApp("transfer", address(ics20Transfer));

        // Mint some tokens
        (address addr, bool ok) = ICS20Lib.hexStringToAddress(e2eFaucet);
        require(ok, "failed to parse faucet address");

        erc20.mint(addr, 1_000_000_000_000_000_000);

        vm.stopBroadcast();

        string memory json = "json";
        json.serialize("ics07Tendermint", Strings.toHexString(address(ics07Tendermint)));
        json.serialize("ics02Client", Strings.toHexString(address(ics26Router.ICS02_CLIENT())));
        json.serialize("ics26Router", Strings.toHexString(address(ics26Router)));
        json.serialize("ics20Transfer", Strings.toHexString(address(ics20Transfer)));
        json.serialize("escrow", Strings.toHexString(ics20Transfer.escrow()));
        json.serialize("ibcstore", Strings.toHexString(address(ics26Router.IBC_STORE())));
        string memory finalJson = json.serialize("erc20", Strings.toHexString(address(erc20)));

        return finalJson;
    }

    function loadGenesis(string memory fileName) public view returns (SP1ICS07TendermintGenesisJson memory) {
        string memory root = vm.projectRoot();
        string memory path = string.concat(root, "/e2e/", fileName);
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
