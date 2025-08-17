// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

import { Test } from "forge-std/Test.sol";

import { ICS26Router } from "../../contracts/ICS26Router.sol";
import { IICS26RouterMsgs } from "../../contracts/msgs/IICS26RouterMsgs.sol";
import { IICS02ClientMsgs } from "../../contracts/msgs/IICS02ClientMsgs.sol";
import { IIBCAppCallbacks } from "../../contracts/msgs/IIBCAppCallbacks.sol";
import { ILightClientMsgs } from "../../contracts/msgs/ILightClientMsgs.sol";
import { AttestorLightClient } from "../../contracts/light-clients/attestor/AttestorLightClient.sol";
import { IAttestorMsgs } from "../../contracts/light-clients/attestor/IAttestorMsgs.sol";
import { ICS24Host } from "../../contracts/utils/ICS24Host.sol";
import { ICS20Lib } from "../../contracts/utils/ICS20Lib.sol";
import { IBCRolesLib } from "../../contracts/utils/IBCRolesLib.sol";

import { AccessManager } from "@openzeppelin-contracts/access/manager/AccessManager.sol";
import { ERC1967Proxy } from "@openzeppelin-contracts/proxy/ERC1967/ERC1967Proxy.sol";

import { TestHelper } from "./utils/TestHelper.sol";

contract MockOKApp is Test {
    event Recv(bytes ack);
    function onRecvPacket(IIBCAppCallbacks.OnRecvPacketCallback calldata) external returns (bytes memory) {
        bytes memory ack = bytes("ack OK");
        emit Recv(ack);
        return ack;
    }

    function onAcknowledgementPacket(IIBCAppCallbacks.OnAcknowledgementPacketCallback calldata) external {}
    function onTimeoutPacket(IIBCAppCallbacks.OnTimeoutPacketCallback calldata) external {}
}

contract AttestorRecvIntegration is Test {
    TestHelper helper = new TestHelper();
    ICS26Router router;
    AttestorLightClient lc;
    MockOKApp app;

    address relayer = address(0xBEEF);

    uint256 p1 = 0xA11CE;
    uint256 p2 = 0xB0B;
    address a1;
    address a2;

    function setUp() public {
        // Deploy router via proxy and set roles
        ICS26Router logic = new ICS26Router();
        AccessManager access = new AccessManager(address(this));
        ERC1967Proxy proxy = new ERC1967Proxy(address(logic), abi.encodeCall(ICS26Router.initialize, (address(access))));
        router = ICS26Router(address(proxy));

        // grant roles: relayer + id-customizer
        access.setTargetFunctionRole(address(router), IBCRolesLib.ics26RelayerSelectors(), IBCRolesLib.RELAYER_ROLE);
        access.setTargetFunctionRole(address(router), IBCRolesLib.ics26IdCustomizerSelectors(), IBCRolesLib.ID_CUSTOMIZER_ROLE);
        access.grantRole(IBCRolesLib.RELAYER_ROLE, relayer, 0);
        access.grantRole(IBCRolesLib.ID_CUSTOMIZER_ROLE, address(this), 0);

        // Deploy attestor LC with router as role manager
        a1 = vm.addr(p1);
        a2 = vm.addr(p2);
        IAttestorMsgs.ClientState memory cs;
        cs.attestors = new address[](2);
        cs.attestors[0] = a1;
        cs.attestors[1] = a2;
        cs.minRequiredSigs = 2;
        cs.latestHeight = IICS02ClientMsgs.Height({ revisionNumber: 0, revisionHeight: 100 });
        cs.isFrozen = false;
        lc = new AttestorLightClient(abi.encode(cs), 1_000, address(router));

        // Register client with counterparty mapping
        router.addClient(
            IICS02ClientMsgs.CounterpartyInfo({ clientId: "counterparty-01", merklePrefix: helper.COSMOS_MERKLE_PREFIX() }),
            address(lc)
        );

        // Add a mock OK app on default port
        app = new MockOKApp();
        router.addIBCApp(ICS20Lib.DEFAULT_PORT_ID, address(app));
    }

    function _sig(uint256 pk, bytes32 digest) internal view returns (bytes memory sig) {
        (uint8 v, bytes32 r, bytes32 s) = vm.sign(pk, digest);
        sig = abi.encodePacked(r, s, v);
    }

    function test_recvPacket_happy_flow_with_membership_proof() public {
        // Prepare a packet
        IICS26RouterMsgs.Payload[] memory payloads = new IICS26RouterMsgs.Payload[](1);
        payloads[0] = IICS26RouterMsgs.Payload({
            sourcePort: ICS20Lib.DEFAULT_PORT_ID,
            destPort: ICS20Lib.DEFAULT_PORT_ID,
            version: ICS20Lib.ICS20_VERSION,
            encoding: ICS20Lib.ICS20_ENCODING,
            value: bytes("some-payload")
        });
        IICS26RouterMsgs.Packet memory packet = IICS26RouterMsgs.Packet({
            sequence: 1,
            sourceClient: "counterparty-01",
            destClient: helper.FIRST_CLIENT_ID(),
            timeoutTimestamp: uint64(block.timestamp + 600),
            payloads: payloads
        });

        // Update LC to set consensus at height 101
        bytes32 commitment = ICS24Host.packetCommitmentBytes32(packet);
        bytes[] memory packets = new bytes[](1);
        packets[0] = abi.encodePacked(commitment);
        bytes32 digest = sha256(abi.encode(packets));
        bytes[] memory sigs = new bytes[](2);
        sigs[0] = _sig(p1, digest);
        sigs[1] = _sig(p2, digest);
        IAttestorMsgs.MsgUpdateClient memory umsg = IAttestorMsgs.MsgUpdateClient({
            newHeight: 101,
            timestamp: uint64(block.timestamp),
            packets: packets,
            signatures: sigs
        });
        vm.prank(address(router));
        lc.updateClient(abi.encode(umsg));

        // Encode membership proof for router->LC
        IAttestorMsgs.MembershipProof memory proof = IAttestorMsgs.MembershipProof({ packets: packets, signatures: sigs });

        // Build recv packet msg
        IICS26RouterMsgs.MsgRecvPacket memory msgRecv = IICS26RouterMsgs.MsgRecvPacket({
            packet: packet,
            proofCommitment: abi.encode(proof),
            proofHeight: IICS02ClientMsgs.Height({ revisionNumber: 0, revisionHeight: 101 })
        });

        // Call as relayer
        vm.prank(relayer);
        router.recvPacket(msgRecv);
    }
}


