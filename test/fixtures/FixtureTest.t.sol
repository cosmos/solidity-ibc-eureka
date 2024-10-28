// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.28;

// solhint-disable custom-errors,max-line-length

import { Test } from "forge-std/Test.sol";
import { IICS26RouterMsgs } from "../../src/msgs/IICS26RouterMsgs.sol";
import { ICS02Client } from "../../src/ICS02Client.sol";
import { IICS02ClientMsgs } from "../../src/msgs/IICS02ClientMsgs.sol";
import { ICS26Router } from "../../src/ICS26Router.sol";
import { IICS26RouterMsgs } from "../../src/msgs/IICS26RouterMsgs.sol";
import { SP1ICS07Tendermint } from "@cosmos/sp1-ics07-tendermint/SP1ICS07Tendermint.sol";
import { SP1Verifier } from "@sp1-contracts/v3.0.0/SP1VerifierPlonk.sol";
import { ICS20Transfer } from "../../src/ICS20Transfer.sol";
import { IICS07TendermintMsgs } from "@cosmos/sp1-ics07-tendermint/msgs/IICS07TendermintMsgs.sol";
import { stdJson } from "forge-std/StdJson.sol";

abstract contract FixtureTest is Test {
    ICS02Client public ics02Client;
    ICS26Router public ics26Router;
    SP1ICS07Tendermint public sp1ICS07Tendermint;
    SP1Verifier public sp1Verifier;
    ICS20Transfer public ics20Transfer;

    string public counterpartyClient = "00-mock-0";
    bytes[] public merklePrefix = [bytes("ibc"), bytes("")];

    using stdJson for string;

    struct SP1ICS07GenesisFixtureJson {
        bytes trustedClientState;
        bytes trustedConsensusState;
        bytes32 updateClientVkey;
        bytes32 membershipVkey;
        bytes32 ucAndMembershipVkey;
        bytes32 misbehaviourVkey;
    }

    struct Fixture {
        SP1ICS07GenesisFixtureJson genesisFixture;
        bytes msg;
        address erc20Address;
        uint256 timestamp;
        IICS26RouterMsgs.Packet packet;
    }

    function setUp() public {
        ics02Client = new ICS02Client(address(this));
        ics26Router = new ICS26Router(address(ics02Client), address(this));
    }

    function loadInitialFixture(string memory fixtureFileName) internal returns (Fixture memory) {
        Fixture memory fixture = loadFixture(fixtureFileName);

        IICS07TendermintMsgs.ConsensusState memory trustedConsensusState =
            abi.decode(fixture.genesisFixture.trustedConsensusState, (IICS07TendermintMsgs.ConsensusState));
        bytes32 trustedConsensusHash = keccak256(abi.encode(trustedConsensusState));

        SP1Verifier verifier = new SP1Verifier();
        SP1ICS07Tendermint ics07Tendermint = new SP1ICS07Tendermint(
            fixture.genesisFixture.updateClientVkey,
            fixture.genesisFixture.membershipVkey,
            fixture.genesisFixture.ucAndMembershipVkey,
            fixture.genesisFixture.misbehaviourVkey,
            address(verifier),
            fixture.genesisFixture.trustedClientState,
            trustedConsensusHash
        );

        ics02Client.addClient(
            "07-tendermint",
            IICS02ClientMsgs.CounterpartyInfo(counterpartyClient, merklePrefix),
            address(ics07Tendermint)
        );

        ics20Transfer = new ICS20Transfer(address(ics26Router));
        ics26Router.addIBCApp("transfer", address(ics20Transfer));

        // Deploy ERC20 to the expected address from the fixture
        deployCodeTo("TestERC20.sol:TestERC20", fixture.erc20Address);

        return fixture;
    }

    function loadFixture(string memory fixtureFileName) internal returns (Fixture memory) {
        string memory root = vm.projectRoot();
        string memory path = string.concat(root, "/test/fixtures/", fixtureFileName);
        string memory json = vm.readFile(path);

        bytes memory sp1GenesisBz = json.readBytes(".sp1_genesis_fixture");
        string memory sp1GenesisJSON = string(sp1GenesisBz);
        SP1ICS07GenesisFixtureJson memory genesisFixture;
        genesisFixture.trustedClientState = sp1GenesisJSON.readBytes(".trustedClientState");
        genesisFixture.trustedConsensusState = sp1GenesisJSON.readBytes(".trustedConsensusState");
        genesisFixture.updateClientVkey = sp1GenesisJSON.readBytes32(".updateClientVkey");
        genesisFixture.membershipVkey = sp1GenesisJSON.readBytes32(".membershipVkey");
        genesisFixture.ucAndMembershipVkey = sp1GenesisJSON.readBytes32(".ucAndMembershipVkey");
        genesisFixture.misbehaviourVkey = sp1GenesisJSON.readBytes32(".misbehaviourVkey");

        bytes memory packetBz = json.readBytes(".packet");
        IICS26RouterMsgs.Packet memory packet = abi.decode(packetBz, (IICS26RouterMsgs.Packet));

        bytes memory msgBz = json.readBytes(".msg");
        address erc20Address = json.readAddress(".erc20_address");
        uint256 timestamp = json.readUint(".timestamp");

        vm.warp(timestamp);

        return Fixture({
            genesisFixture: genesisFixture,
            msg: msgBz,
            erc20Address: erc20Address,
            timestamp: timestamp,
            packet: packet
        });
    }
}
