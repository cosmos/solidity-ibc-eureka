// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

// solhint-disable gas-custom-errors

// solhint-disable-next-line no-global-import
import "forge-std/console.sol";
import { MembershipTest } from "./MembershipTest.sol";
import { ILightClient } from "../../contracts/interfaces/ILightClient.sol";

contract SP1ICS07LargeMembershipTest is MembershipTest {
    SP1MembershipProof public proof;

    function setUpLargeMembershipTestWithFixture(string memory fileName) public {
        setUpTestWithFixtures(fileName);

        proof = abi.decode(fixture.membershipProof.proof, (SP1MembershipProof));
    }

    function getOutput() public view returns (MembershipOutput memory) {
        return abi.decode(proof.sp1Proof.publicValues, (MembershipOutput));
    }

    function test_ValidLargeCachedVerifyMembership_25_plonk() public {
        ValidCachedMulticallMembershipTest("membership_25-plonk_fixture.json", 25, "25 key-value pairs with plonk");
    }

    function test_ValidLargeCachedVerifyMembership_100_groth16() public {
        ValidCachedMulticallMembershipTest(
            "membership_100-groth16_fixture.json", 100, "100 key-value pairs with groth16"
        );
    }

    function ValidCachedMulticallMembershipTest(string memory fileName, uint32 n, string memory metadata) public {
        require(n > 0, "n must be greater than 0");

        setUpLargeMembershipTestWithFixture(fileName);

        bytes[] memory multicallData = new bytes[](n);

        multicallData[0] = abi.encodeCall(
            ILightClient.membership,
            MsgMembership({
                proof: abi.encode(fixture.membershipProof),
                proofHeight: fixture.proofHeight,
                path: getOutput().kvPairs[0].path,
                value: getOutput().kvPairs[0].value
            })
        );

        for (uint32 i = 1; i < n; i++) {
            multicallData[i] = abi.encodeCall(
                ILightClient.membership,
                MsgMembership({
                    proof: bytes(""), // cached kv pairs
                    proofHeight: fixture.proofHeight,
                    path: getOutput().kvPairs[i].path,
                    value: getOutput().kvPairs[i].value
                })
            );
        }

        ics07Tendermint.multicall(multicallData);
        console.log("Proved", metadata, ", gas used: ", vm.lastCallGas().gasTotalUsed);
    }
}
