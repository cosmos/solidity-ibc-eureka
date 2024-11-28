// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.28;

// solhint-disable-next-line no-global-import
import "forge-std/console.sol";
import { stdJson } from "forge-std/StdJson.sol";
import { SP1ICS07TendermintTest } from "./SP1ICS07TendermintTest.sol";

abstract contract MembershipTest is SP1ICS07TendermintTest {
    bytes[] public verifyMembershipPath = [bytes("ibc"), bytes("clients/07-tendermint-0/clientState")];

    bytes[] public verifyNonMembershipPath = [bytes("ibc"), bytes("clients/07-tendermint-001/clientState")];

    bytes public constant VERIFY_MEMBERSHIP_VALUE =
        hex"0a2b2f6962632e6c69676874636c69656e74732e74656e6465726d696e742e76312e436c69656e7453746174651287010a1174686574612d746573746e65742d3030311204080110031a040880840722040880c60a2a02082832003a0510b7e3c60842190a090801180120012a0100120c0a02000110211804200c300142190a090801180120012a0100120c0a02000110201801200130014a07757067726164654a107570677261646564494243537461746550015801";

    struct SP1ICS07MembershipFixtureJson {
        Height proofHeight;
        MembershipProof membershipProof;
    }

    using stdJson for string;

    SP1ICS07MembershipFixtureJson public fixture;

    function setUpTestWithFixtures(string memory fileName) public {
        fixture = loadFixture(fileName);

        setUpTest(fileName);
    }

    function loadFixture(string memory fileName) public view returns (SP1ICS07MembershipFixtureJson memory) {
        string memory root = vm.projectRoot();
        string memory path = string.concat(root, FIXTURE_DIR, fileName);
        string memory json = vm.readFile(path);
        bytes memory proofHeightBz = json.readBytes(".proofHeight");
        bytes memory membershipProofBz = json.readBytes(".membershipProof");

        SP1ICS07MembershipFixtureJson memory fix = SP1ICS07MembershipFixtureJson({
            proofHeight: abi.decode(proofHeightBz, (Height)),
            membershipProof: abi.decode(membershipProofBz, (MembershipProof))
        });

        return fix;
    }
}
