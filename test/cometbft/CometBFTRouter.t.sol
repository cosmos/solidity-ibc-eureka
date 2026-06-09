// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

import { Test } from "forge-std/Test.sol";
import { stdJson } from "forge-std/StdJson.sol";

import { AccessManager } from "@openzeppelin-contracts/access/manager/AccessManager.sol";
import { ERC1967Proxy } from "@openzeppelin-contracts/proxy/ERC1967/ERC1967Proxy.sol";

import { ICS26Router } from "../../contracts/ICS26Router.sol";
import { IICS26Router } from "../../contracts/interfaces/IICS26Router.sol";
import { IIBCApp } from "../../contracts/interfaces/IIBCApp.sol";
import { IICS02ClientMsgs } from "../../contracts/msgs/IICS02ClientMsgs.sol";
import { IICS26RouterMsgs } from "../../contracts/msgs/IICS26RouterMsgs.sol";
import { IIBCAppCallbacks } from "../../contracts/msgs/IIBCAppCallbacks.sol";
import { IICS26RouterErrors } from "../../contracts/errors/IICS26RouterErrors.sol";
import { ICS24Host } from "../../contracts/utils/ICS24Host.sol";
import { IBCRolesLib } from "../../contracts/utils/IBCRolesLib.sol";
import { CometBFTClient } from "../../contracts/light-clients/cometbft/CometBFTClient.sol";
import { ICometBFTClientErrors } from "../../contracts/light-clients/cometbft/errors/ICometBFTClientErrors.sol";
import { ICometBFTMsgs } from "../../contracts/light-clients/cometbft/msgs/ICometBFTMsgs.sol";

contract CometBFTRouterTest is Test {
    using stdJson for string;

    bytes32 private constant IBC_STORE_SLOT = 0x1260944489272988d9df285149b5aa1b0f48f2136d6f416159f840a3e0747600;
    string private constant PORT_ID = "transfer";

    string private routerFixtureJson;
    address private relayer = makeAddr("relayer");
    address private idCustomizer = makeAddr("idCustomizer");

    function setUp() public {
        routerFixtureJson = vm.readFile(_fixturePath("native_ics23_router_fixture.json"));
    }

    function test_recvPacketVerifiesNativeCometBFTPacketCommitment() public {
        IICS26RouterMsgs.Packet memory packet = _packet(".packetCommitment.packet");
        (ICS26Router router, RouterApp app,) = _deployRouter(
            _cosmosMerklePrefix(),
            _consensusState(".packetCommitment"),
            _proofHeight(".packetCommitment"),
            packet.destClient,
            packet.sourceClient
        );

        assertEq(
            ICS24Host.prefixedPath(
                _cosmosMerklePrefix(), ICS24Host.packetCommitmentPathCalldata(packet.sourceClient, packet.sequence)
            ),
            routerFixtureJson.readBytesArray(".packetCommitment.path")
        );
        assertEq(
            abi.encodePacked(ICS24Host.packetCommitmentBytes32(packet)),
            routerFixtureJson.readBytes(".packetCommitment.value")
        );

        vm.expectEmit();
        bytes[] memory expectedAcks = new bytes[](1);
        expectedAcks[0] = routerFixtureJson.readBytes(".acknowledgement");
        emit IICS26Router.WriteAcknowledgement(packet.destClient, packet.sequence, packet, expectedAcks);

        vm.prank(relayer);
        router.recvPacket(
            IICS26RouterMsgs.MsgRecvPacket({
                packet: packet,
                proofCommitment: routerFixtureJson.readBytes(".packetCommitment.proof"),
                proofHeight: _proofHeight(".packetCommitment")
            })
        );

        assertEq(app.recvCount(), 1);
        assertEq(
            router.getCommitment(
                keccak256(ICS24Host.packetReceiptCommitmentPathCalldata(packet.destClient, packet.sequence))
            ),
            ICS24Host.packetReceiptCommitmentBytes32(packet)
        );
        assertEq(
            router.getCommitment(
                ICS24Host.packetAcknowledgementCommitmentKeyCalldata(packet.destClient, packet.sequence)
            ),
            ICS24Host.packetAcknowledgementCommitmentBytes32(expectedAcks)
        );
    }

    function test_ackPacketVerifiesNativeCometBFTAcknowledgementCommitment() public {
        IICS26RouterMsgs.Packet memory packet = _packet(".acknowledgementCommitment.packet");
        (ICS26Router router, RouterApp app,) = _deployRouter(
            _cosmosMerklePrefix(),
            _consensusState(".acknowledgementCommitment"),
            _proofHeight(".acknowledgementCommitment"),
            packet.sourceClient,
            packet.destClient
        );
        _cheatPacketCommitment(router, packet);

        assertEq(
            ICS24Host.prefixedPath(
                _cosmosMerklePrefix(),
                ICS24Host.packetAcknowledgementCommitmentPathCalldata(packet.destClient, packet.sequence)
            ),
            routerFixtureJson.readBytesArray(".acknowledgementCommitment.path")
        );
        bytes[] memory expectedAcks = new bytes[](1);
        expectedAcks[0] = routerFixtureJson.readBytes(".acknowledgement");
        assertEq(
            abi.encodePacked(ICS24Host.packetAcknowledgementCommitmentBytes32(expectedAcks)),
            routerFixtureJson.readBytes(".acknowledgementCommitment.value")
        );

        vm.prank(relayer);
        router.ackPacket(
            IICS26RouterMsgs.MsgAckPacket({
                packet: packet,
                acknowledgement: expectedAcks[0],
                proofAcked: routerFixtureJson.readBytes(".acknowledgementCommitment.proof"),
                proofHeight: _proofHeight(".acknowledgementCommitment")
            })
        );

        assertEq(app.ackCount(), 1);
        assertEq(router.getCommitment(ICS24Host.packetCommitmentKeyCalldata(packet.sourceClient, packet.sequence)), 0);
    }

    function test_ackPacketRejectsWrongNativeCometBFTAcknowledgementValue() public {
        IICS26RouterMsgs.Packet memory packet = _packet(".acknowledgementCommitment.packet");
        (ICS26Router router,,) = _deployRouter(
            _cosmosMerklePrefix(),
            _consensusState(".acknowledgementCommitment"),
            _proofHeight(".acknowledgementCommitment"),
            packet.sourceClient,
            packet.destClient
        );
        _cheatPacketCommitment(router, packet);

        vm.expectRevert(abi.encodeWithSelector(ICometBFTClientErrors.InvalidICS23Proof.selector));
        vm.prank(relayer);
        router.ackPacket(
            IICS26RouterMsgs.MsgAckPacket({
                packet: packet,
                acknowledgement: "wrong-ack",
                proofAcked: routerFixtureJson.readBytes(".acknowledgementCommitment.proof"),
                proofHeight: _proofHeight(".acknowledgementCommitment")
            })
        );
    }

    function test_timeoutPacketVerifiesNativeCometBFTPacketReceiptNonMembership() public {
        IICS26RouterMsgs.Packet memory packet = _packet(".packetReceipt.packet");
        (ICS26Router router, RouterApp app,) = _deployRouter(
            _cosmosMerklePrefix(),
            _consensusState(".packetReceipt"),
            _proofHeight(".packetReceipt"),
            packet.sourceClient,
            packet.destClient
        );
        _cheatPacketCommitment(router, packet);

        assertEq(
            ICS24Host.prefixedPath(
                _cosmosMerklePrefix(), ICS24Host.packetReceiptCommitmentPathCalldata(packet.destClient, packet.sequence)
            ),
            routerFixtureJson.readBytesArray(".packetReceipt.path")
        );

        vm.prank(relayer);
        router.timeoutPacket(
            IICS26RouterMsgs.MsgTimeoutPacket({
                packet: packet,
                proofTimeout: routerFixtureJson.readBytes(".packetReceipt.proof"),
                proofHeight: _proofHeight(".packetReceipt")
            })
        );

        assertEq(app.timeoutCount(), 1);
        assertEq(router.getCommitment(ICS24Host.packetCommitmentKeyCalldata(packet.sourceClient, packet.sequence)), 0);
    }

    function test_recvPacketRejectsWrongNativeCometBFTMerklePrefix() public {
        IICS26RouterMsgs.Packet memory packet = _packet(".packetCommitment.packet");
        (ICS26Router router,,) = _deployRouter(
            _wrongMerklePrefix(),
            _consensusState(".packetCommitment"),
            _proofHeight(".packetCommitment"),
            packet.destClient,
            packet.sourceClient
        );

        vm.expectRevert(abi.encodeWithSelector(ICometBFTClientErrors.InvalidICS23Proof.selector));
        vm.prank(relayer);
        router.recvPacket(
            IICS26RouterMsgs.MsgRecvPacket({
                packet: packet,
                proofCommitment: routerFixtureJson.readBytes(".packetCommitment.proof"),
                proofHeight: _proofHeight(".packetCommitment")
            })
        );
    }

    function test_recvPacketRejectsWrongNativeCometBFTValue() public {
        IICS26RouterMsgs.Packet memory packet = _packet(".packetCommitment.packet");
        (ICS26Router router,,) = _deployRouter(
            _cosmosMerklePrefix(),
            _consensusState(".packetCommitment"),
            _proofHeight(".packetCommitment"),
            packet.destClient,
            packet.sourceClient
        );
        packet.payloads[0].value = hex"04";

        vm.expectRevert(abi.encodeWithSelector(ICometBFTClientErrors.InvalidICS23Proof.selector));
        vm.prank(relayer);
        router.recvPacket(
            IICS26RouterMsgs.MsgRecvPacket({
                packet: packet,
                proofCommitment: routerFixtureJson.readBytes(".packetCommitment.proof"),
                proofHeight: _proofHeight(".packetCommitment")
            })
        );
    }

    function test_timeoutPacketRejectsStaleNativeCometBFTConsensusTimestamp() public {
        IICS26RouterMsgs.Packet memory packet = _packet(".packetReceipt.packet");
        ICometBFTMsgs.ConsensusState memory staleConsensusState = _consensusState(".packetReceipt");
        staleConsensusState.timestamp = (uint128(packet.timeoutTimestamp) - 1) * 1e9;
        (ICS26Router router,,) = _deployRouter(
            _cosmosMerklePrefix(),
            staleConsensusState,
            _proofHeight(".packetReceipt"),
            packet.sourceClient,
            packet.destClient
        );
        _cheatPacketCommitment(router, packet);

        vm.expectRevert(
            abi.encodeWithSelector(
                IICS26RouterErrors.IBCInvalidTimeoutTimestamp.selector,
                packet.timeoutTimestamp,
                staleConsensusState.timestamp / 1e9
            )
        );
        vm.prank(relayer);
        router.timeoutPacket(
            IICS26RouterMsgs.MsgTimeoutPacket({
                packet: packet,
                proofTimeout: routerFixtureJson.readBytes(".packetReceipt.proof"),
                proofHeight: _proofHeight(".packetReceipt")
            })
        );
    }

    function test_timeoutPacketRejectsWrongNativeCometBFTReceiptPath() public {
        IICS26RouterMsgs.Packet memory packet = _packet(".packetReceipt.packet");
        (ICS26Router router,,) = _deployRouter(
            _cosmosMerklePrefix(),
            _consensusState(".packetReceipt"),
            _proofHeight(".packetReceipt"),
            packet.sourceClient,
            packet.destClient
        );
        packet.sequence += 1;
        _cheatPacketCommitment(router, packet);

        vm.expectRevert(abi.encodeWithSelector(ICometBFTClientErrors.InvalidICS23Proof.selector));
        vm.prank(relayer);
        router.timeoutPacket(
            IICS26RouterMsgs.MsgTimeoutPacket({
                packet: packet,
                proofTimeout: routerFixtureJson.readBytes(".packetReceipt.proof"),
                proofHeight: _proofHeight(".packetReceipt")
            })
        );
    }

    function _deployRouter(
        bytes[] memory merklePrefix,
        ICometBFTMsgs.ConsensusState memory consensusState,
        IICS02ClientMsgs.Height memory trustedHeight,
        string memory localClientId,
        string memory counterpartyClientId
    )
        private
        returns (ICS26Router router, RouterApp app, CometBFTClient client)
    {
        ICS26Router routerLogic = new ICS26Router();
        AccessManager accessManager = new AccessManager(address(this));
        ERC1967Proxy routerProxy =
            new ERC1967Proxy(address(routerLogic), abi.encodeCall(ICS26Router.initialize, (address(accessManager))));
        router = ICS26Router(address(routerProxy));

        accessManager.setTargetFunctionRole(
            address(router), IBCRolesLib.ics26RelayerSelectors(), IBCRolesLib.RELAYER_ROLE
        );
        accessManager.setTargetFunctionRole(
            address(router), IBCRolesLib.ics26IdCustomizerSelectors(), IBCRolesLib.ID_CUSTOMIZER_ROLE
        );
        accessManager.grantRole(IBCRolesLib.RELAYER_ROLE, relayer, 0);
        accessManager.grantRole(IBCRolesLib.ID_CUSTOMIZER_ROLE, idCustomizer, 0);

        client = new CometBFTClient(_clientState(trustedHeight), consensusState, address(this));
        client.grantRole(client.PROOF_SUBMITTER_ROLE(), address(router));
        string memory clientId;
        IICS02ClientMsgs.CounterpartyInfo memory counterpartyInfo =
            IICS02ClientMsgs.CounterpartyInfo({ clientId: counterpartyClientId, merklePrefix: merklePrefix });
        if (keccak256(bytes(localClientId)) == keccak256(bytes("client-0"))) {
            clientId = router.addClient(counterpartyInfo, address(client));
        } else {
            vm.prank(idCustomizer);
            clientId = router.addClient(localClientId, counterpartyInfo, address(client));
        }
        assertEq(clientId, localClientId);

        app = new RouterApp(routerFixtureJson.readBytes(".acknowledgement"));
        vm.prank(idCustomizer);
        router.addIBCApp(PORT_ID, address(app));
    }

    function _clientState(IICS02ClientMsgs.Height memory latestHeight)
        private
        pure
        returns (ICometBFTMsgs.ClientState memory)
    {
        return ICometBFTMsgs.ClientState({
            chainId: "native-cometbft-router-1",
            trustLevel: ICometBFTMsgs.TrustThreshold({ numerator: 1, denominator: 3 }),
            latestHeight: latestHeight,
            trustingPeriod: 14 days,
            unbondingPeriod: 21 days,
            maxClockDrift: 30,
            isFrozen: false
        });
    }

    function _consensusState(string memory selector) private view returns (ICometBFTMsgs.ConsensusState memory) {
        return ICometBFTMsgs.ConsensusState({
            timestamp: uint128(routerFixtureJson.readUint(string.concat(selector, ".timestamp"))),
            root: routerFixtureJson.readBytes32(string.concat(selector, ".root")),
            nextValidatorsHash: routerFixtureJson.readBytes32(string.concat(selector, ".nextValidatorsHash"))
        });
    }

    function _proofHeight(string memory selector) private view returns (IICS02ClientMsgs.Height memory) {
        return IICS02ClientMsgs.Height({
            revisionNumber: 0,
            revisionHeight: uint64(routerFixtureJson.readUint(string.concat(selector, ".proofHeight")))
        });
    }

    function _packet(string memory selector) private view returns (IICS26RouterMsgs.Packet memory packet) {
        packet.sequence = uint64(routerFixtureJson.readUint(string.concat(selector, ".sequence")));
        packet.sourceClient = routerFixtureJson.readString(string.concat(selector, ".sourceClient"));
        packet.destClient = routerFixtureJson.readString(string.concat(selector, ".destClient"));
        packet.timeoutTimestamp = uint64(routerFixtureJson.readUint(string.concat(selector, ".timeoutTimestamp")));
        packet.payloads = new IICS26RouterMsgs.Payload[](1);
        packet.payloads[0] = IICS26RouterMsgs.Payload({
            sourcePort: routerFixtureJson.readString(string.concat(selector, ".payload.sourcePort")),
            destPort: routerFixtureJson.readString(string.concat(selector, ".payload.destPort")),
            version: routerFixtureJson.readString(string.concat(selector, ".payload.version")),
            encoding: routerFixtureJson.readString(string.concat(selector, ".payload.encoding")),
            value: routerFixtureJson.readBytes(string.concat(selector, ".payload.value"))
        });
    }

    function _cosmosMerklePrefix() private pure returns (bytes[] memory prefix) {
        prefix = new bytes[](2);
        prefix[0] = "ibc";
        prefix[1] = "";
    }

    function _wrongMerklePrefix() private pure returns (bytes[] memory prefix) {
        prefix = new bytes[](2);
        prefix[0] = "wrong";
        prefix[1] = "";
    }

    function _cheatPacketCommitment(ICS26Router router, IICS26RouterMsgs.Packet memory packet) private {
        bytes32 path = ICS24Host.packetCommitmentKeyCalldata(packet.sourceClient, packet.sequence);
        bytes32 value = ICS24Host.packetCommitmentBytes32(packet);
        bytes32 commitmentSlot = keccak256(abi.encodePacked(path, IBC_STORE_SLOT));
        vm.store(address(router), commitmentSlot, value);
    }

    function _fixturePath(string memory fileName) private view returns (string memory) {
        string memory projectRootPath = vm.projectRoot();
        string memory path = string.concat(projectRootPath, "/test/cometbft/fixtures/", fileName);
        if (vm.exists(path)) {
            return path;
        }
        return string.concat(projectRootPath, "/../test/cometbft/fixtures/", fileName);
    }
}

contract RouterApp is IIBCApp {
    bytes private acknowledgement;
    uint256 public recvCount;
    uint256 public ackCount;
    uint256 public timeoutCount;

    constructor(bytes memory acknowledgement_) {
        acknowledgement = acknowledgement_;
    }

    function onRecvPacket(IIBCAppCallbacks.OnRecvPacketCallback calldata) external override returns (bytes memory) {
        ++recvCount;
        return acknowledgement;
    }

    function onAcknowledgementPacket(IIBCAppCallbacks.OnAcknowledgementPacketCallback calldata) external override {
        ++ackCount;
    }

    function onTimeoutPacket(IIBCAppCallbacks.OnTimeoutPacketCallback calldata) external override {
        ++timeoutCount;
    }
}
