// SPDX-License-Identifier: UNLICENSED
pragma solidity >=0.8.25 <0.9.0;

/*
    This script is used for local testing with SP1_PROVER=mock.
*/

// solhint-disable gas-custom-errors,custom-errors

import { stdJson } from "forge-std/StdJson.sol";
import { Script } from "forge-std/Script.sol";
import { SP1ICS07Tendermint } from "@cosmos/sp1-ics07-tendermint/SP1ICS07Tendermint.sol";
import { IICS07TendermintMsgs } from "@cosmos/sp1-ics07-tendermint/msgs/IICS07TendermintMsgs.sol";
import { ICS02Client } from "../src/ICS02Client.sol";
import { ICS26Router } from "../src/ICS26Router.sol";
import { ICS20Transfer } from "../src/ICS20Transfer.sol";
import { TestERC20 } from "../test/TestERC20.sol";
import { AcceptAllSP1Verifier } from "../test/AcceptAllSP1Verifier.sol";
import { Strings } from "@openzeppelin/contracts/utils/Strings.sol";
import { ICS20Lib } from "../src/utils/ICS20Lib.sol";

struct SP1ICS07TendermintGenesisJson {
    bytes trustedClientState;
    bytes trustedConsensusState;
    bytes32 updateClientVkey;
    bytes32 membershipVkey;
    bytes32 ucAndMembershipVkey;
}

/// @dev See the Solidity Scripting tutorial: https://book.getfoundry.sh/tutorials/solidity-scripting
contract MockE2ETestDeploy is Script {
    using stdJson for string;

    string public constant E2E_FAUCET = "0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266";

    function run() public returns (string memory) {
        // Read the initialization parameters for the SP1 Tendermint contract.
        SP1ICS07TendermintGenesisJson memory genesis = loadGenesis("genesis.json");
        IICS07TendermintMsgs.ConsensusState memory trustedConsensusState =
            abi.decode(genesis.trustedConsensusState, (IICS07TendermintMsgs.ConsensusState));
        bytes32 trustedConsensusHash = keccak256(abi.encode(trustedConsensusState));

        vm.startBroadcast();
        address deployerAddress = msg.sender; // This is being set in the e2e test

        // Deploy the SP1 ICS07 Tendermint light client
        AcceptAllSP1Verifier verifier = new AcceptAllSP1Verifier();
        SP1ICS07Tendermint ics07Tendermint = new SP1ICS07Tendermint(
            genesis.updateClientVkey,
            genesis.membershipVkey,
            genesis.ucAndMembershipVkey,
            address(verifier),
            genesis.trustedClientState,
            trustedConsensusHash
        );

        // Deploy IBC Eureka
        ICS02Client ics02Client = new ICS02Client(deployerAddress);
        ICS26Router ics26Router = new ICS26Router(address(ics02Client), deployerAddress);
        ICS20Transfer ics20Transfer = new ICS20Transfer(address(ics26Router));
        TestERC20 erc20 = new TestERC20();

        // Wire Transfer app
        ics26Router.addIBCApp("transfer", address(ics20Transfer));

        // Mint some tokens
        (address addr, bool ok) = ICS20Lib.hexStringToAddress(E2E_FAUCET);
        require(ok, "invalid address");
        erc20.mint(addr, 100_000_000_000);

        vm.stopBroadcast();

        string memory json = "json";
        json.serialize("ics07Tendermint", Strings.toHexString(address(ics07Tendermint)));
        json.serialize("ics02Client", Strings.toHexString(address(ics02Client)));
        json.serialize("ics26Router", Strings.toHexString(address(ics26Router)));
        json.serialize("ics20Transfer", Strings.toHexString(address(ics20Transfer)));
        string memory finalJson = json.serialize("erc20", Strings.toHexString(address(erc20)));

        return finalJson;
    }

    function loadGenesis(string memory fileName) public view returns (SP1ICS07TendermintGenesisJson memory) {
        string memory root = vm.projectRoot();
        string memory path = string.concat(root, "/e2e/artifacts/", fileName);
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
