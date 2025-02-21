// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

// solhint-disable-next-line no-global-import
import "forge-std/console.sol";
import { stdJson } from "forge-std/StdJson.sol";
import { MembershipTest } from "./MembershipTest.sol";

contract SP1ICS07UpdateClientAndMembershipTest is MembershipTest {
    using stdJson for string;

    SP1MembershipAndUpdateClientProof public proof;

    function setUpUcAndMemTestWithFixtures(string memory fileName) public {
        setUpTestWithFixtures(fileName);

        proof = abi.decode(fixture.membershipProof.proof, (SP1MembershipAndUpdateClientProof));

        UcAndMembershipOutput memory output = abi.decode(proof.sp1Proof.publicValues, (UcAndMembershipOutput));

        ClientState memory clientState = abi.decode(mockIcs07Tendermint.getClientState(), (ClientState));
        assert(clientState.latestHeight.revisionHeight < output.updateClientOutput.newHeight.revisionHeight);
    }

    function fixtureTestCases() public pure returns (FixtureTestCase[] memory) {
        FixtureTestCase[] memory testCases = new FixtureTestCase[](2);
        testCases[0] = FixtureTestCase({ name: "groth16", fileName: "uc_and_memberships_fixture-groth16.json" });
        testCases[1] = FixtureTestCase({ name: "plonk", fileName: "uc_and_memberships_fixture-plonk.json" });

        return testCases;
    }

    // Confirm that submitting a real proof passes the verifier.
    function test_Valid_UpdateClientAndVerifyMembership() public {
        FixtureTestCase[] memory testCases = fixtureTestCases();

        for (uint256 i = 0; i < testCases.length; i++) {
            setUpUcAndMemTestWithFixtures(testCases[i].fileName);

            UcAndMembershipOutput memory output = abi.decode(proof.sp1Proof.publicValues, (UcAndMembershipOutput));
            // set a correct timestamp
            vm.warp(output.updateClientOutput.time + 300);

            MsgMembership memory membershipMsg = MsgMembership({
                proof: abi.encode(fixture.membershipProof),
                proofHeight: fixture.proofHeight,
                path: verifyMembershipPath,
                value: VERIFY_MEMBERSHIP_VALUE
            });

            // run verify
            ics07Tendermint.membership(membershipMsg);

            console.log(
                "UpdateClientAndVerifyMembership-", testCases[i].name, "gas used: ", vm.lastCallGas().gasTotalUsed
            );

            ClientState memory clientState = abi.decode(ics07Tendermint.getClientState(), (ClientState));
            assert(clientState.latestHeight.revisionHeight == output.updateClientOutput.newHeight.revisionHeight);
            assert(clientState.isFrozen == false);

            bytes32 consensusHash =
                ics07Tendermint.getConsensusStateHash(output.updateClientOutput.newHeight.revisionHeight);
            assert(consensusHash == keccak256(abi.encode(output.updateClientOutput.newConsensusState)));
        }
    }

    // Confirm that submitting a real proof passes the verifier.
    function test_Valid_UpdateClientAndVerifyNonMembership() public {
        FixtureTestCase[] memory testCases = fixtureTestCases();

        for (uint256 i = 0; i < testCases.length; i++) {
            setUpUcAndMemTestWithFixtures(testCases[i].fileName);

            UcAndMembershipOutput memory output = abi.decode(proof.sp1Proof.publicValues, (UcAndMembershipOutput));
            // set a correct timestamp
            vm.warp(output.updateClientOutput.time + 300);

            MsgMembership memory nonMembershipMsg = MsgMembership({
                proof: abi.encode(fixture.membershipProof),
                proofHeight: fixture.proofHeight,
                path: verifyNonMembershipPath,
                value: bytes("")
            });

            // run verify
            ics07Tendermint.membership(nonMembershipMsg);

            console.log(
                "UpdateClientAndVerifyNonMembership-", testCases[i].name, "gas used: ", vm.lastCallGas().gasTotalUsed
            );

            ClientState memory clientState = abi.decode(ics07Tendermint.getClientState(), (ClientState));
            assert(clientState.latestHeight.revisionHeight == output.updateClientOutput.newHeight.revisionHeight);
            assert(clientState.isFrozen == false);

            bytes32 consensusHash =
                ics07Tendermint.getConsensusStateHash(output.updateClientOutput.newHeight.revisionHeight);
            assert(consensusHash == keccak256(abi.encode(output.updateClientOutput.newConsensusState)));
        }
    }

    // Confirm that submitting a real proof passes the verifier.
    function test_Valid_CachedUpdateClientAndMembership() public {
        // It doesn't matter which fixture we use since this is a cached proof
        setUpUcAndMemTestWithFixtures("uc_and_memberships_fixture-groth16.json");

        UcAndMembershipOutput memory output = abi.decode(proof.sp1Proof.publicValues, (UcAndMembershipOutput));
        // set a correct timestamp
        vm.warp(output.updateClientOutput.time + 300);

        MsgMembership memory membershipMsg = MsgMembership({
            proof: abi.encode(fixture.membershipProof),
            proofHeight: fixture.proofHeight,
            path: verifyMembershipPath,
            value: VERIFY_MEMBERSHIP_VALUE
        });

        // run verify
        ics07Tendermint.membership(membershipMsg);

        ClientState memory clientState = abi.decode(ics07Tendermint.getClientState(), (ClientState));
        assert(clientState.latestHeight.revisionHeight == output.updateClientOutput.newHeight.revisionHeight);
        assert(clientState.isFrozen == false);

        bytes32 consensusHash =
            ics07Tendermint.getConsensusStateHash(output.updateClientOutput.newHeight.revisionHeight);
        assert(consensusHash == keccak256(abi.encode(output.updateClientOutput.newConsensusState)));

        // submit cached membership proof
        MsgMembership memory cachedMembershipMsg = MsgMembership({
            proof: bytes(""),
            proofHeight: fixture.proofHeight,
            path: verifyMembershipPath,
            value: VERIFY_MEMBERSHIP_VALUE
        });
        ics07Tendermint.membership(cachedMembershipMsg);

        console.log("Cached UpdateClientAndVerifyMembership gas used: ", vm.lastCallGas().gasTotalUsed);

        // submit cached non-membership proof
        MsgMembership memory nonMembershipMsg = MsgMembership({
            proof: bytes(""),
            proofHeight: fixture.proofHeight,
            path: verifyNonMembershipPath,
            value: bytes("")
        });

        // run verify
        ics07Tendermint.membership(nonMembershipMsg);

        console.log("Cached UpdateClientAndNonVerifyMembership gas used: ", vm.lastCallGas().gasTotalUsed);
    }

    function test_Invalid_UpdateClientAndMembership() public {
        // It doesn't matter which fixture we use since this is an invalid proof
        setUpUcAndMemTestWithFixtures("uc_and_memberships_fixture-groth16.json");

        UcAndMembershipOutput memory output = abi.decode(proof.sp1Proof.publicValues, (UcAndMembershipOutput));
        // set a correct timestamp
        vm.warp(output.updateClientOutput.time + 300);

        SP1MembershipAndUpdateClientProof memory ucAndMemProof = proof;
        ucAndMemProof.sp1Proof.proof = bytes("invalid");

        MembershipProof memory membershipProof = MembershipProof({
            proofType: MembershipProofType.SP1MembershipAndUpdateClientProof,
            proof: abi.encode(ucAndMemProof)
        });

        MsgMembership memory membershipMsg = MsgMembership({
            proof: abi.encode(membershipProof),
            proofHeight: fixture.proofHeight,
            path: verifyNonMembershipPath,
            value: bytes("")
        });

        vm.expectRevert();
        ics07Tendermint.membership(membershipMsg);
    }

    function test_MockMisbehavior_UpdateClientAndMembership() public {
        // It doesn't matter which fixture we use since this is a mock contract
        setUpUcAndMemTestWithFixtures("uc_and_memberships_fixture-groth16.json");

        UcAndMembershipOutput memory output = abi.decode(proof.sp1Proof.publicValues, (UcAndMembershipOutput));
        // set a correct timestamp
        vm.warp(output.updateClientOutput.time + 300);

        SP1MembershipAndUpdateClientProof memory ucAndMemProof = proof;
        ucAndMemProof.sp1Proof.proof = bytes("");

        MembershipProof memory membershipProof = MembershipProof({
            proofType: MembershipProofType.SP1MembershipAndUpdateClientProof,
            proof: abi.encode(ucAndMemProof)
        });

        MsgMembership memory membershipMsg = MsgMembership({
            proof: abi.encode(membershipProof),
            proofHeight: fixture.proofHeight,
            path: verifyNonMembershipPath,
            value: bytes("")
        });

        mockIcs07Tendermint.membership(membershipMsg);

        // change output so that it is a misbehaviour
        output.updateClientOutput.newConsensusState.timestamp = output.updateClientOutput.time + 1;
        // re-encode output
        ucAndMemProof.sp1Proof.publicValues = abi.encode(output);

        membershipProof.proof = abi.encode(ucAndMemProof);
        membershipMsg.proof = abi.encode(membershipProof);

        // run verify again
        vm.expectRevert(abi.encodeWithSelector(CannotHandleMisbehavior.selector));
        mockIcs07Tendermint.membership(membershipMsg);
    }
}
