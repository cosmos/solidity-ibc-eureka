// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

// solhint-disable custom-errors

import { Test } from "forge-std/Test.sol";

import { SolanaIFTSendCallConstructor } from "../../contracts/utils/SolanaIFTSendCallConstructor.sol";
import { IIFTSendCallConstructor } from "../../contracts/interfaces/IIFTSendCallConstructor.sol";
import { IERC165 } from "@openzeppelin-contracts/utils/introspection/IERC165.sol";

contract SolanaIFTSendCallConstructorTest is Test {
    SolanaIFTSendCallConstructor public constructor_;

    // Valid 32-byte Solana pubkey as hex
    string constant VALID_RECEIVER = "0x06ddf6e1d765a193d9cbe146ceeb79ac1cb485ed5f5b37913a8cf5857eff00a9";

    function setUp() public {
        constructor_ = new SolanaIFTSendCallConstructor();
    }

    function test_constructMintCall_validHex() public view {
        bytes memory result = constructor_.constructMintCall(VALID_RECEIVER, 1_000_000);
        (bytes32 receiver, uint256 amount) = abi.decode(result, (bytes32, uint256));
        assertEq(receiver, bytes32(hex"06ddf6e1d765a193d9cbe146ceeb79ac1cb485ed5f5b37913a8cf5857eff00a9"));
        assertEq(amount, 1_000_000);
    }

    function test_constructMintCall_invalidHex_reverts() public {
        vm.expectRevert();
        constructor_.constructMintCall("0xGGGGGGGGGGGGGGGGGGGGGGGGGGGGGGGGGGGGGGGGGGGGGGGGGGGGGGGGGGGGGGGG", 1000);
    }

    function test_constructMintCall_wrongLength_reverts() public {
        vm.expectRevert();
        constructor_.constructMintCall("0x1234", 1000);
    }

    function testFuzz_constructMintCall_anyAmount(uint256 amount) public view {
        bytes memory result = constructor_.constructMintCall(VALID_RECEIVER, amount);
        (, uint256 decoded) = abi.decode(result, (bytes32, uint256));
        assertEq(decoded, amount);
    }

    function test_supportsInterface() public view {
        assertTrue(constructor_.supportsInterface(type(IIFTSendCallConstructor).interfaceId));
        assertTrue(constructor_.supportsInterface(type(IERC165).interfaceId));
    }
}
