// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.28;

// solhint-disable custom-errors,max-line-length,gas-custom-errors

import { Test } from "forge-std/Test.sol";
import { IICS26RouterMsgs } from "../../contracts/msgs/IICS26RouterMsgs.sol";
import { IICS02ClientMsgs } from "../../contracts/msgs/IICS02ClientMsgs.sol";
import { ICS26Router } from "../../contracts/ICS26Router.sol";
import { IICS26RouterMsgs } from "../../contracts/msgs/IICS26RouterMsgs.sol";
import { SP1ICS07Tendermint } from "../../contracts/light-clients/SP1ICS07Tendermint.sol";
import { ICS20Transfer } from "../../contracts/ICS20Transfer.sol";
import { IICS07TendermintMsgs } from "../../contracts/light-clients/msgs/IICS07TendermintMsgs.sol";
import { ICS20Lib } from "../../contracts/utils/ICS20Lib.sol";
import { stdJson } from "forge-std/StdJson.sol";
import { ERC1967Proxy } from "@openzeppelin-contracts/proxy/ERC1967/ERC1967Proxy.sol";
import { SP1Verifier as SP1VerifierPlonk } from "@sp1-contracts/v4.0.0-rc.3/SP1VerifierPlonk.sol";
import { SP1Verifier as SP1VerifierGroth16 } from "@sp1-contracts/v4.0.0-rc.3/SP1VerifierGroth16.sol";
import { IBCERC20 } from "../../contracts/utils/IBCERC20.sol";
import { Escrow } from "../../contracts/utils/Escrow.sol";

abstract contract FixtureTest is Test, IICS07TendermintMsgs {
    ICS26Router public ics26Router;
    SP1ICS07Tendermint public sp1ICS07Tendermint;
    ICS20Transfer public ics20Transfer;

    string public counterpartyId = "08-wasm-0";
    bytes[] public merklePrefix = [bytes("ibc"), bytes("")];
    bytes[] public singleSuccessAck = [ICS20Lib.SUCCESSFUL_ACKNOWLEDGEMENT_JSON];

    string internal constant FIXTURE_DIR = "/test/solidity-ibc/fixtures/";

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
        // ============ Step 1: Deploy the logic contracts ==============
        address escrowLogic = address(new Escrow());
        address ibcERC20Logic = address(new IBCERC20());
        ICS26Router ics26RouterLogic = new ICS26Router();
        ICS20Transfer ics20TransferLogic = new ICS20Transfer();

        // ============== Step 2: Deploy ERC1967 Proxies ==============
        ERC1967Proxy routerProxy = new ERC1967Proxy(
            address(ics26RouterLogic), abi.encodeCall(ICS26Router.initialize, (address(this), address(this)))
        );

        ERC1967Proxy transferProxy = new ERC1967Proxy(
            address(ics20TransferLogic),
            abi.encodeCall(
                ICS20Transfer.initialize, (address(routerProxy), escrowLogic, ibcERC20Logic, address(0), address(0))
            )
        );

        // ============== Step 3: Wire up the contracts ==============
        ics26Router = ICS26Router(address(routerProxy));
        ics20Transfer = ICS20Transfer(address(transferProxy));
    }

    function loadInitialFixture(string memory fixtureFileName) internal returns (Fixture memory) {
        Fixture memory fixture = loadFixture(fixtureFileName);

        ConsensusState memory trustedConsensusState =
            abi.decode(fixture.genesisFixture.trustedConsensusState, (ConsensusState));
        bytes32 trustedConsensusHash = keccak256(abi.encode(trustedConsensusState));
        ClientState memory trustedClientState = abi.decode(fixture.genesisFixture.trustedClientState, (ClientState));

        address verifier;
        if (trustedClientState.zkAlgorithm == SupportedZkAlgorithm.Plonk) {
            verifier = address(new SP1VerifierPlonk());
        } else if (trustedClientState.zkAlgorithm == SupportedZkAlgorithm.Groth16) {
            verifier = address(new SP1VerifierGroth16());
        } else {
            revert("Unsupported zk algorithm");
        }

        SP1ICS07Tendermint ics07Tendermint = new SP1ICS07Tendermint(
            fixture.genesisFixture.updateClientVkey,
            fixture.genesisFixture.membershipVkey,
            fixture.genesisFixture.ucAndMembershipVkey,
            fixture.genesisFixture.misbehaviourVkey,
            verifier,
            fixture.genesisFixture.trustedClientState,
            trustedConsensusHash
        );

        ics26Router.addClient(IICS02ClientMsgs.CounterpartyInfo(counterpartyId, merklePrefix), address(ics07Tendermint));
        ics26Router.addIBCApp("transfer", address(ics20Transfer));

        // Deploy ERC20 to the expected address from the fixture
        deployCodeTo("TestERC20.sol:TestERC20", fixture.erc20Address);

        return fixture;
    }

    function loadFixture(string memory fixtureFileName) internal returns (Fixture memory) {
        string memory root = vm.projectRoot();
        string memory path = string.concat(root, FIXTURE_DIR, fixtureFileName);
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
