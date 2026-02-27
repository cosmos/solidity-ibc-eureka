// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

// solhint-disable custom-errors,gas-small-strings,no-inline-assembly,gas-increment-by-one

import { Test } from "forge-std/Test.sol";

import { SolanaIFTSendCallConstructor } from "../../contracts/utils/SolanaIFTSendCallConstructor.sol";
import { IIFTSendCallConstructor } from "../../contracts/interfaces/IIFTSendCallConstructor.sol";
import { IERC165 } from "@openzeppelin-contracts/utils/introspection/IERC165.sol";

contract SolanaIFTSendCallConstructorTest is Test {
    SolanaIFTSendCallConstructor public constructor_;

    // Test PDA values (arbitrary bytes32 for testing)
    bytes32 internal constant APP_STATE = bytes32(uint256(1));
    bytes32 internal constant APP_MINT_STATE = bytes32(uint256(2));
    bytes32 internal constant IFT_BRIDGE = bytes32(uint256(3));
    bytes32 internal constant MINT = bytes32(uint256(4));
    bytes32 internal constant MINT_AUTHORITY = bytes32(uint256(5));
    bytes32 internal constant GMP_ACCOUNT = bytes32(uint256(6));

    // Receiver: "0x" + wallet(64 hex) + ata(64 hex) = 130 chars
    bytes32 internal constant WALLET = 0x06ddf6e1d765a193d9cbe146ceeb79ac1cb485ed5f5b37913a8cf5857eff00a9;
    bytes32 internal constant ATA = 0x8c97258f4e2489f1bb3d1029148e0d830b5a1399daff1084048e7bd8dbe9f859;

    string internal constant VALID_RECEIVER =
        "0x06ddf6e1d765a193d9cbe146ceeb79ac1cb485ed5f5b37913a8cf5857eff00a98c97258f4e2489f1bb3d1029148e0d830b5a1399daff1084048e7bd8dbe9f859";

    // Well-known Solana program IDs
    bytes32 internal constant TOKEN_PROGRAM = 0x06ddf6e1d765a193d9cbe146ceeb79ac1cb485ed5f5b37913a8cf5857eff00a9;
    bytes32 internal constant ASSOCIATED_TOKEN_PROGRAM =
        0x8c97258f4e2489f1bb3d1029148e0d830b5a1399daff1084048e7bd8dbe9f859;
    bytes32 internal constant SYSTEM_PROGRAM = bytes32(0);

    uint256 internal constant PACKED_ACCOUNT_SIZE = 34;

    function setUp() public {
        constructor_ =
            new SolanaIFTSendCallConstructor(APP_STATE, APP_MINT_STATE, IFT_BRIDGE, MINT, MINT_AUTHORITY, GMP_ACCOUNT);
    }

    function test_immutables() public view {
        assertEq(constructor_.APP_STATE(), APP_STATE);
        assertEq(constructor_.APP_MINT_STATE(), APP_MINT_STATE);
        assertEq(constructor_.IFT_BRIDGE(), IFT_BRIDGE);
        assertEq(constructor_.MINT(), MINT);
        assertEq(constructor_.MINT_AUTHORITY(), MINT_AUTHORITY);
        assertEq(constructor_.GMP_ACCOUNT(), GMP_ACCOUNT);
    }

    function test_constructMintCall_outputFormat() public view {
        bytes memory result = constructor_.constructMintCall(VALID_RECEIVER, 1_000_000);

        (bytes memory packedAccounts, bytes memory instructionData, uint32 payerPosition) =
            abi.decode(result, (bytes, bytes, uint32));

        // 11 accounts * 34 bytes each
        assertEq(packedAccounts.length, 11 * PACKED_ACCOUNT_SIZE);
        // discriminator(8) + wallet(32) + amount_le(8) = 48
        assertEq(instructionData.length, 48);
        // Payer injected at position 8
        assertEq(payerPosition, 8);
    }

    function test_constructMintCall_packedAccounts() public view {
        bytes memory result = constructor_.constructMintCall(VALID_RECEIVER, 1_000_000);
        (bytes memory packedAccounts,,) = abi.decode(result, (bytes, bytes, uint32));

        _assertAccount(packedAccounts, 0, APP_STATE, false, false);
        _assertAccount(packedAccounts, 1, APP_MINT_STATE, false, true);
        _assertAccount(packedAccounts, 2, IFT_BRIDGE, false, false);
        _assertAccount(packedAccounts, 3, MINT, false, true);
        _assertAccount(packedAccounts, 4, MINT_AUTHORITY, false, false);
        _assertAccount(packedAccounts, 5, ATA, false, true); // receiver token account
        _assertAccount(packedAccounts, 6, WALLET, false, false); // receiver owner
        _assertAccount(packedAccounts, 7, GMP_ACCOUNT, true, false); // signer
        _assertAccount(packedAccounts, 8, TOKEN_PROGRAM, false, false);
        _assertAccount(packedAccounts, 9, ASSOCIATED_TOKEN_PROGRAM, false, false);
        _assertAccount(packedAccounts, 10, SYSTEM_PROGRAM, false, false);
    }

    function test_constructMintCall_instructionData() public view {
        uint256 amount = 1_000_000;
        bytes memory result = constructor_.constructMintCall(VALID_RECEIVER, amount);
        (, bytes memory instructionData,) = abi.decode(result, (bytes, bytes, uint32));

        bytes8 discriminator;
        assembly {
            discriminator := mload(add(instructionData, 32))
        }
        assertEq(discriminator, bytes8(0x9cc9d99072afaa53));

        bytes32 walletInData;
        assembly {
            walletInData := mload(add(instructionData, 40))
        }
        assertEq(walletInData, WALLET);

        // Decode little-endian u64 from last 8 bytes
        uint64 decoded;
        for (uint256 i = 0; i < 8; i++) {
            decoded |= uint64(uint8(instructionData[40 + i])) << uint64(i * 8);
        }
        assertEq(decoded, amount);
    }

    function test_constructMintCall_wrongLength_reverts() public {
        vm.expectRevert();
        constructor_.constructMintCall("0x1234", 1000);
    }

    function test_constructMintCall_tooLong_reverts() public {
        vm.expectRevert();
        constructor_.constructMintCall(
            "0x06ddf6e1d765a193d9cbe146ceeb79ac1cb485ed5f5b37913a8cf5857eff00a98c97258f4e2489f1bb3d1029148e0d830b5a1399daff1084048e7bd8dbe9f859ab",
            1000
        );
    }

    function test_constructMintCall_invalidHex_reverts() public {
        vm.expectRevert();
        constructor_.constructMintCall(
            "0xGGddf6e1d765a193d9cbe146ceeb79ac1cb485ed5f5b37913a8cf5857eff00a98c97258f4e2489f1bb3d1029148e0d830b5a1399daff1084048e7bd8dbe9f859",
            1000
        );
    }

    function testFuzz_constructMintCall_amountEncoding(uint64 amount) public view {
        bytes memory result = constructor_.constructMintCall(VALID_RECEIVER, uint256(amount));
        (, bytes memory instructionData,) = abi.decode(result, (bytes, bytes, uint32));

        uint64 decoded;
        for (uint256 i = 0; i < 8; i++) {
            decoded |= uint64(uint8(instructionData[40 + i])) << uint64(i * 8);
        }
        assertEq(decoded, amount);
    }

    function test_supportsInterface() public view {
        assertTrue(constructor_.supportsInterface(type(IIFTSendCallConstructor).interfaceId));
        assertTrue(constructor_.supportsInterface(type(IERC165).interfaceId));
    }

    function _assertAccount(
        bytes memory packed,
        uint256 index,
        bytes32 expectedPubkey,
        bool expectedSigner,
        bool expectedWritable
    )
        private
        pure
    {
        uint256 offset = index * PACKED_ACCOUNT_SIZE;

        bytes32 pubkey;
        assembly {
            pubkey := mload(add(add(packed, 32), offset))
        }
        assertEq(pubkey, expectedPubkey);

        uint8 isSigner = uint8(packed[offset + 32]);
        uint8 isWritable = uint8(packed[offset + 33]);
        assertEq(isSigner, expectedSigner ? 1 : 0);
        assertEq(isWritable, expectedWritable ? 1 : 0);
    }
}
