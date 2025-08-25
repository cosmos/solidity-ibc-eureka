// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

import { Test } from "forge-std/Test.sol";

import { AttestorLightClient } from "contracts/light-clients/AttestorLightClient.sol";
import { IAttestorMsgs } from "contracts/light-clients/msgs/IAttestorMsgs.sol";
import { ILightClientMsgs } from "contracts/msgs/ILightClientMsgs.sol";
import { IICS02ClientMsgs } from "contracts/msgs/IICS02ClientMsgs.sol";
import { IAttestorLightClientErrors } from "contracts/light-clients/errors/IAttestorLightClientErrors.sol";

contract AttestorLightClientTest is Test {
    AttestorLightClient internal client;

    uint64 internal constant INITIAL_HEIGHT = 100;
    uint64 internal constant INITIAL_TS = 1_700_000_000;

    uint256 internal attestorPrivKey1;
    uint256 internal attestorPrivKey2;
    uint256 internal attestorPrivKey3;
    address internal attestorAddr1;
    address internal attestorAddr2;
    address internal attestorAddr3;

    function setUp() public {
        attestorPrivKey1 = 0xA11CE;
        attestorPrivKey2 = 0xB0B;
        attestorPrivKey3 = 0xC0C;
        attestorAddr1 = vm.addr(attestorPrivKey1);
        attestorAddr2 = vm.addr(attestorPrivKey2);
        attestorAddr3 = vm.addr(attestorPrivKey3);

        address[] memory addrs = new address[](3);
        addrs[0] = attestorAddr1;
        addrs[1] = attestorAddr2;
        addrs[2] = attestorAddr3;

        client = new AttestorLightClient({
            attestorAddresses: addrs,
            minRequiredSigs: 2,
            initialHeight: INITIAL_HEIGHT,
            initialTimestampSeconds: INITIAL_TS,
            roleManager: address(0)
        });
    }

    function test_updateClient_success_updates_height_and_ts() public {
        uint64 newHeight = INITIAL_HEIGHT + 1;
        uint64 newTs = INITIAL_TS + 100;

        uint256[] memory signers = new uint256[](2);
        signers[0] = attestorPrivKey1;
        signers[1] = attestorPrivKey2;

        (bytes memory attestationData, bytes[] memory signatures) = _stateAttestation(newHeight, newTs, signers);
        IAttestorMsgs.AttestationProof memory proof =
            IAttestorMsgs.AttestationProof({ attestationData: attestationData, signatures: signatures });

        ILightClientMsgs.UpdateResult res = client.updateClient(abi.encode(proof));
        assertEq(uint8(res), uint8(ILightClientMsgs.UpdateResult.Update));
        assertEq(client.getConsensusTimestamp(newHeight), newTs);
    }

    function test_updateClient_noop_same_height_same_ts() public {
        uint256[] memory signers = new uint256[](2);
        signers[0] = attestorPrivKey1;
        signers[1] = attestorPrivKey2;
        (bytes memory attestationData, bytes[] memory signatures) =
            _stateAttestation(INITIAL_HEIGHT, INITIAL_TS, signers);

        IAttestorMsgs.AttestationProof memory proof =
            IAttestorMsgs.AttestationProof({ attestationData: attestationData, signatures: signatures });

        ILightClientMsgs.UpdateResult res = client.updateClient(abi.encode(proof));
        assertEq(uint8(res), uint8(ILightClientMsgs.UpdateResult.NoOp));
    }

    function test_updateClient_noop_same_height_different_ts_reverts() public {
        uint256[] memory signers = new uint256[](2);
        signers[0] = attestorPrivKey1;
        signers[1] = attestorPrivKey2;
        uint64 conflictingTs = INITIAL_TS + 1;
        (bytes memory attestationData, bytes[] memory signatures) =
            _stateAttestation(INITIAL_HEIGHT, conflictingTs, signers);

        vm.expectRevert(
            abi.encodeWithSelector(
                IAttestorLightClientErrors.ConflictingTimestamp.selector, INITIAL_HEIGHT, INITIAL_TS, conflictingTs
            )
        );
        IAttestorMsgs.AttestationProof memory proof =
            IAttestorMsgs.AttestationProof({ attestationData: attestationData, signatures: signatures });
        client.updateClient(abi.encode(proof));
    }

    function test_updateClient_out_of_order_update() public {
        uint64 lowerHeight = INITIAL_HEIGHT - 1;
        uint256[] memory signers = new uint256[](2);
        signers[0] = attestorPrivKey1;
        signers[1] = attestorPrivKey2;
        (bytes memory attestationData, bytes[] memory signatures) =
            _stateAttestation(lowerHeight, INITIAL_TS - 1, signers);

        IAttestorMsgs.AttestationProof memory proof =
            IAttestorMsgs.AttestationProof({ attestationData: attestationData, signatures: signatures });

        ILightClientMsgs.UpdateResult res = client.updateClient(abi.encode(proof));
        assertEq(uint8(res), uint8(ILightClientMsgs.UpdateResult.Update));
    }

    function test_updateClient_revert_threshold_not_met() public {
        uint64 newHeight = INITIAL_HEIGHT + 10;
        uint256[] memory signers = new uint256[](1);
        signers[0] = attestorPrivKey1;
        (bytes memory attestationData, bytes[] memory signatures) =
            _stateAttestation(newHeight, INITIAL_TS + 10, signers);

        vm.expectRevert(abi.encodeWithSelector(IAttestorLightClientErrors.ThresholdNotMet.selector, 1, 2));
        IAttestorMsgs.AttestationProof memory proof =
            IAttestorMsgs.AttestationProof({ attestationData: attestationData, signatures: signatures });
        client.updateClient(abi.encode(proof));
    }

    function test_updateClient_revert_duplicate_signer() public {
        bytes memory attestationData =
            abi.encode(IAttestorMsgs.StateAttestation({ height: INITIAL_HEIGHT + 1, timestamp: INITIAL_TS + 1 }));
        bytes32 digest = sha256(attestationData);

        bytes[] memory signatures = new bytes[](2);
        signatures[0] = _sig(attestorPrivKey1, digest);
        signatures[1] = _sig(attestorPrivKey1, digest);

        vm.expectRevert(abi.encodeWithSelector(IAttestorLightClientErrors.DuplicateSigner.selector, attestorAddr1));
        IAttestorMsgs.AttestationProof memory proof =
            IAttestorMsgs.AttestationProof({ attestationData: attestationData, signatures: signatures });
        client.updateClient(abi.encode(proof));
    }

    function test_updateClient_revert_unknown_signer() public {
        bytes memory attestationData =
            abi.encode(IAttestorMsgs.StateAttestation({ height: INITIAL_HEIGHT + 1, timestamp: INITIAL_TS + 1 }));
        bytes32 digest = sha256(attestationData);

        uint256 badPriv = 0xDEADBEEF;
        bytes[] memory signatures = new bytes[](2);
        signatures[0] = _sig(attestorPrivKey1, digest);
        signatures[1] = _sig(badPriv, digest);

        vm.expectRevert(abi.encodeWithSelector(IAttestorLightClientErrors.UnknownSigner.selector, vm.addr(badPriv)));
        IAttestorMsgs.AttestationProof memory proof =
            IAttestorMsgs.AttestationProof({ attestationData: attestationData, signatures: signatures });
        client.updateClient(abi.encode(proof));
    }

    function test_verifyMembership_success_returns_ts() public {
        bytes32[] memory commitments = new bytes32[](3);
        commitments[0] = bytes32("a");
        commitments[1] = bytes32("b");
        commitments[2] = bytes32("c");

        uint256[] memory signers = new uint256[](2);
        signers[0] = attestorPrivKey1;
        signers[1] = attestorPrivKey2;

        (bytes memory attestationData, bytes[] memory signatures) = _attestation(INITIAL_HEIGHT, commitments, signers);
        IAttestorMsgs.AttestationProof memory proof =
            IAttestorMsgs.AttestationProof({ attestationData: attestationData, signatures: signatures });

        ILightClientMsgs.MsgVerifyMembership memory msgVerify;
        msgVerify.proof = abi.encode(proof);
        msgVerify.proofHeight = IICS02ClientMsgs.Height({ revisionNumber: 0, revisionHeight: INITIAL_HEIGHT });
        msgVerify.path = new bytes[](0);
        msgVerify.value = abi.encode(bytes32("b"));

        uint256 ts = client.verifyMembership(msgVerify);
        assertEq(ts, INITIAL_TS);
    }

    function test_verifyMembership_revert_empty_value() public {
        IAttestorMsgs.PacketAttestation memory p =
            IAttestorMsgs.PacketAttestation({ height: INITIAL_HEIGHT, packets: new bytes32[](0) });
        IAttestorMsgs.AttestationProof memory proof =
            IAttestorMsgs.AttestationProof({ attestationData: abi.encode(p), signatures: new bytes[](0) });
        ILightClientMsgs.MsgVerifyMembership memory msgVerify;
        msgVerify.proof = abi.encode(proof);
        msgVerify.proofHeight = IICS02ClientMsgs.Height({ revisionNumber: 0, revisionHeight: INITIAL_HEIGHT });
        msgVerify.path = new bytes[](0);
        msgVerify.value = bytes("");

        vm.expectRevert(abi.encodeWithSelector(IAttestorLightClientErrors.EmptyValue.selector));
        client.verifyMembership(msgVerify);
    }

    function test_verifyMembership_revert_not_member() public {
        bytes32[] memory commitments = new bytes32[](2);
        commitments[0] = bytes32("x");
        commitments[1] = bytes32("y");

        uint256[] memory signers = new uint256[](2);
        signers[0] = attestorPrivKey2;
        signers[1] = attestorPrivKey3;
        (bytes memory attestationData, bytes[] memory signatures) = _attestation(INITIAL_HEIGHT, commitments, signers);

        IAttestorMsgs.AttestationProof memory proof =
            IAttestorMsgs.AttestationProof({ attestationData: attestationData, signatures: signatures });

        ILightClientMsgs.MsgVerifyMembership memory msgVerify;
        msgVerify.proof = abi.encode(proof);
        msgVerify.proofHeight = IICS02ClientMsgs.Height({ revisionNumber: 0, revisionHeight: INITIAL_HEIGHT });
        msgVerify.path = new bytes[](0);
        msgVerify.value = abi.encode(bytes32("z"));

        vm.expectRevert(abi.encodeWithSelector(IAttestorLightClientErrors.NotMember.selector));
        client.verifyMembership(msgVerify);
    }

    function test_verifyMembership_revert_missing_timestamp_for_height() public {
        uint64 unknownHeight = INITIAL_HEIGHT + 50;

        bytes32[] memory commitments = new bytes32[](1);
        commitments[0] = bytes32("a");
        uint256[] memory signers = new uint256[](2);
        signers[0] = attestorPrivKey1;
        signers[1] = attestorPrivKey2;
        (bytes memory attestationData, bytes[] memory signatures) = _attestation(unknownHeight, commitments, signers);
        IAttestorMsgs.AttestationProof memory proof =
            IAttestorMsgs.AttestationProof({ attestationData: attestationData, signatures: signatures });

        ILightClientMsgs.MsgVerifyMembership memory msgVerify;
        msgVerify.proof = abi.encode(proof);
        msgVerify.proofHeight = IICS02ClientMsgs.Height({ revisionNumber: 0, revisionHeight: unknownHeight });
        msgVerify.path = new bytes[](0);
        msgVerify.value = abi.encode(bytes32("a"));

        vm.expectRevert(
            abi.encodeWithSelector(IAttestorLightClientErrors.ConsensusTimestampNotFound.selector, unknownHeight)
        );
        client.verifyMembership(msgVerify);
    }

    function test_verifyMembership_revert_height_mismatch() public {
        // Build an attestation with a different height than the proofHeight
        uint64 attestedHeight = INITIAL_HEIGHT + 5;
        bytes32[] memory commitments = new bytes32[](2);
        commitments[0] = bytes32("x");
        commitments[1] = bytes32("y");

        uint256[] memory signers = new uint256[](2);
        signers[0] = attestorPrivKey1;
        signers[1] = attestorPrivKey2;

        (bytes memory attestationData, bytes[] memory signatures) = _attestation(attestedHeight, commitments, signers);
        IAttestorMsgs.AttestationProof memory proof =
            IAttestorMsgs.AttestationProof({ attestationData: attestationData, signatures: signatures });

        ILightClientMsgs.MsgVerifyMembership memory msgVerify;
        msgVerify.proof = abi.encode(proof);
        // Provide a different height here so the mismatch triggers
        msgVerify.proofHeight = IICS02ClientMsgs.Height({ revisionNumber: 0, revisionHeight: INITIAL_HEIGHT });
        msgVerify.path = new bytes[](0);
        msgVerify.value = abi.encode(bytes32("y"));

        vm.expectRevert(
            abi.encodeWithSelector(IAttestorLightClientErrors.HeightMismatch.selector, INITIAL_HEIGHT, attestedHeight)
        );
        client.verifyMembership(msgVerify);
    }

    function test_verifyNonMembership_reverts_feature_not_supported() public {
        ILightClientMsgs.MsgVerifyNonMembership memory m;
        m.proof = bytes("");
        m.proofHeight = IICS02ClientMsgs.Height({ revisionNumber: 0, revisionHeight: INITIAL_HEIGHT });
        m.path = new bytes[](0);
        vm.expectRevert(abi.encodeWithSelector(IAttestorLightClientErrors.FeatureNotSupported.selector));
        client.verifyNonMembership(m);
    }

    function test_misbehaviour_and_upgradeClient_revert_feature_not_supported() public {
        vm.expectRevert(abi.encodeWithSelector(IAttestorLightClientErrors.FeatureNotSupported.selector));
        client.misbehaviour(bytes(""));
        vm.expectRevert(abi.encodeWithSelector(IAttestorLightClientErrors.FeatureNotSupported.selector));
        client.upgradeClient(bytes(""));
    }

    function _sig(uint256 privKey, bytes32 digest) internal pure returns (bytes memory) {
        (uint8 v, bytes32 r, bytes32 s) = vm.sign(privKey, digest);
        return abi.encodePacked(r, s, v);
    }

    function _attestation(
        uint64 height,
        bytes32[] memory commitments,
        uint256[] memory signers
    )
        internal
        pure
        returns (bytes memory attestationData, bytes[] memory signatures)
    {
        IAttestorMsgs.PacketAttestation memory p =
            IAttestorMsgs.PacketAttestation({ height: height, packets: commitments });
        attestationData = abi.encode(p);
        bytes32 digest = sha256(attestationData);
        signatures = new bytes[](signers.length);
        for (uint256 i = 0; i < signers.length; ++i) {
            signatures[i] = _sig(signers[i], digest);
        }
    }

    function _stateAttestation(
        uint64 height,
        uint64 timestamp,
        uint256[] memory signers
    )
        internal
        pure
        returns (bytes memory attestationData, bytes[] memory signatures)
    {
        IAttestorMsgs.StateAttestation memory s =
            IAttestorMsgs.StateAttestation({ height: height, timestamp: timestamp });
        attestationData = abi.encode(s);
        bytes32 digest = sha256(attestationData);
        signatures = new bytes[](signers.length);
        for (uint256 i = 0; i < signers.length; ++i) {
            signatures[i] = _sig(signers[i], digest);
        }
    }
}
