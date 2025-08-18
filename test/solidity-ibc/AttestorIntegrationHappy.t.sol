// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

import { Test } from "forge-std/Test.sol";

import { ICS26Router } from "../../contracts/ICS26Router.sol";
import { IICS26RouterMsgs } from "../../contracts/msgs/IICS26RouterMsgs.sol";
import { IICS02ClientMsgs } from "../../contracts/msgs/IICS02ClientMsgs.sol";
import { ILightClient } from "../../contracts/interfaces/ILightClient.sol";
import { ILightClientMsgs } from "../../contracts/msgs/ILightClientMsgs.sol";
import { AttestorLightClient } from "../../contracts/light-clients/attestor/AttestorLightClient.sol";
import { IAttestorMsgs } from "../../contracts/light-clients/attestor/IAttestorMsgs.sol";

contract AttestorIntegrationHappy is Test {
    ICS26Router router;
    AttestorLightClient lc;

    uint256 p1 = 0xA11CE;
    uint256 p2 = 0xB0B;
    address a1;
    address a2;

    function setUp() public {
        router = new ICS26Router();

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
    }

    function _sig(uint256 pk, bytes32 digest) internal view returns (bytes memory sig) {
        (uint8 v, bytes32 r, bytes32 s) = vm.sign(pk, digest);
        sig = abi.encodePacked(r, s, v);
    }

    function test_router_calls_membership_flow_happy() public {
        // Prepare an attested packets set
        bytes[] memory packets = new bytes[](1);
        packets[0] = bytes("packet-1");
        bytes32 digest = sha256(abi.encode(packets));
        bytes[] memory sigs = new bytes[](2);
        sigs[0] = _sig(p1, digest);
        sigs[1] = _sig(p2, digest);
        address[] memory signers = new address[](2);
        signers[0] = a1;
        signers[1] = a2;

        // role manager is router; call via router using standard IICS26 flow shape
        // First, update light client directly (router would typically send this)
        IAttestorMsgs.MsgUpdateClient memory umsg = IAttestorMsgs.MsgUpdateClient({
            newHeight: 101,
            timestamp: 1_234,
            packets: packets,
            signers: signers,
            signatures: sigs
        });
        vm.prank(address(router));
        ILightClientMsgs.UpdateResult res = lc.updateClient(abi.encode(umsg));
        assertEq(uint8(res), uint8(ILightClientMsgs.UpdateResult.Update));

        // Then, verify membership via router role (empty-proof path ok in same tx)
        ILightClientMsgs.MsgVerifyMembership memory vmsg = ILightClientMsgs.MsgVerifyMembership({
            proof: bytes(""),
            proofHeight: IICS02ClientMsgs.Height({ revisionNumber: 0, revisionHeight: 101 }),
            path: new bytes[](0),
            value: packets[0]
        });
        vm.prank(address(router));
        uint256 ts = lc.verifyMembership(vmsg);
        assertEq(ts, 1_234);
    }
}


