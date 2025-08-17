// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

import { Test } from "forge-std/Test.sol";

import { ILightClient } from "../../contracts/interfaces/ILightClient.sol";
import { ILightClientMsgs } from "../../contracts/msgs/ILightClientMsgs.sol";
import { IICS02ClientMsgs } from "../../contracts/msgs/IICS02ClientMsgs.sol";
import { AttestorLightClient } from "../../contracts/light-clients/attestor/AttestorLightClient.sol";
import { IAttestorMsgs } from "../../contracts/light-clients/attestor/IAttestorMsgs.sol";

contract AttestorLightClientTest is Test {
    AttestorLightClient lc;

    uint256 p1 = 0xA11CE;
    uint256 p2 = 0xB0B;
    uint256 p3 = 0xCAFE;
    address a1;
    address a2;
    address a3;

    function setUp() public {
        a1 = vm.addr(p1);
        a2 = vm.addr(p2);
        a3 = vm.addr(p3);

        IAttestorMsgs.ClientState memory cs;
        cs.attestors = new address[](3);
        cs.attestors[0] = a1;
        cs.attestors[1] = a2;
        cs.attestors[2] = a3;
        cs.minRequiredSigs = 2;
        cs.latestHeight = IICS02ClientMsgs.Height({ revisionNumber: 0, revisionHeight: 100 });
        cs.isFrozen = false;

        lc = new AttestorLightClient(abi.encode(cs), 1_000, address(0));
    }

    function _sig(uint256 pk, bytes32 digest) internal view returns (bytes memory sig) {
        (uint8 v, bytes32 r, bytes32 s) = vm.sign(pk, digest);
        sig = abi.encodePacked(r, s, v);
    }

    function _happyPackets() internal pure returns (bytes[] memory packets, bytes memory valueIn) {
        packets = new bytes[](2);
        packets[0] = bytes("hello");
        packets[1] = bytes("world");
        valueIn = packets[1];
    }

    function test_update_and_membership_with_proof_and_empty_proof() public {
        (bytes[] memory packets, bytes memory valueIn) = _happyPackets();

        bytes32 digest = sha256(abi.encode(packets));
        bytes[] memory sigs = new bytes[](2);
        sigs[0] = _sig(p1, digest);
        sigs[1] = _sig(p2, digest);

        // Update
        IAttestorMsgs.MsgUpdateClient memory umsg = IAttestorMsgs.MsgUpdateClient({
            newHeight: 101,
            timestamp: 1_100,
            packets: packets,
            signatures: sigs
        });
        ILightClientMsgs.UpdateResult res = lc.updateClient(abi.encode(umsg));
        assertEq(uint8(res), uint8(ILightClientMsgs.UpdateResult.Update));

        // Membership with full proof
        IAttestorMsgs.MembershipProof memory proof = IAttestorMsgs.MembershipProof({ packets: packets, signatures: sigs });
        uint256 ts = lc.verifyMembership(
            ILightClientMsgs.MsgVerifyMembership({
                proof: abi.encode(proof),
                proofHeight: IICS02ClientMsgs.Height({ revisionNumber: 0, revisionHeight: 101 }),
                path: new bytes[](0),
                value: valueIn
            })
        );
        assertEq(ts, 1_100);

        // Empty-proof membership in same tx (uses transient cache from update)
        uint256 ts2 = lc.verifyMembership(
            ILightClientMsgs.MsgVerifyMembership({
                proof: bytes(""),
                proofHeight: IICS02ClientMsgs.Height({ revisionNumber: 0, revisionHeight: 101 }),
                path: new bytes[](0),
                value: valueIn
            })
        );
        assertEq(ts2, 1_100);
    }

    function test_update_noop_on_same_height_same_timestamp() public {
        (bytes[] memory packets, ) = _happyPackets();

        bytes32 digest = sha256(abi.encode(packets));
        bytes[] memory sigs = new bytes[](2);
        sigs[0] = _sig(p1, digest);
        sigs[1] = _sig(p2, digest);

        // First update at 101
        IAttestorMsgs.MsgUpdateClient memory u1 = IAttestorMsgs.MsgUpdateClient({
            newHeight: 101,
            timestamp: 1_100,
            packets: packets,
            signatures: sigs
        });
        lc.updateClient(abi.encode(u1));

        // Second update with same height and same timestamp => NoOp
        ILightClientMsgs.UpdateResult res = lc.updateClient(abi.encode(u1));
        assertEq(uint8(res), uint8(ILightClientMsgs.UpdateResult.NoOp));
    }

    function test_freeze_blocks_updates() public {
        // freeze
        lc.misbehaviour(bytes("whatever"));

        (bytes[] memory packets, ) = _happyPackets();
        bytes32 digest = sha256(abi.encode(packets));
        bytes[] memory sigs = new bytes[](2);
        sigs[0] = _sig(p1, digest);
        sigs[1] = _sig(p2, digest);

        IAttestorMsgs.MsgUpdateClient memory umsg = IAttestorMsgs.MsgUpdateClient({
            newHeight: 101,
            timestamp: 1_100,
            packets: packets,
            signatures: sigs
        });

        vm.expectRevert();
        lc.updateClient(abi.encode(umsg));
    }
}


