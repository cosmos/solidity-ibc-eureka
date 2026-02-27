// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

// solhint-disable gas-strict-inequalities,no-inline-assembly

import { IIFTSendCallConstructor } from "../interfaces/IIFTSendCallConstructor.sol";

import { Strings } from "@openzeppelin-contracts/utils/Strings.sol";
import { ERC165 } from "@openzeppelin-contracts/utils/introspection/ERC165.sol";
import { IERC165 } from "@openzeppelin-contracts/utils/introspection/IERC165.sol";

/// @title Solana IFT Send Call Constructor
/// @notice Constructs ABI-encoded GmpSolanaPayload for minting IFT tokens on Solana.
/// @dev Stores 6 static PDAs as immutables (set at deployment, precomputed off-chain).
///      The `constructMintCall` builds the complete execution payload committed in the IBC packet,
///      eliminating the need for a relayer-controlled hint account.
contract SolanaIFTSendCallConstructor is IIFTSendCallConstructor, ERC165 {
    // -- Well-known Solana program IDs (constants) --

    /// @notice SPL Token program ID (`TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA`)
    bytes32 private constant TOKEN_PROGRAM = 0x06ddf6e1d765a193d9cbe146ceeb79ac1cb485ed5f5b37913a8cf5857eff00a9;
    /// @notice Associated Token Account program ID (`ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL`)
    bytes32 private constant ASSOCIATED_TOKEN_PROGRAM =
        0x8c97258f4e2489f1bb3d1029148e0d830b5a1399daff1084048e7bd8dbe9f859;
    /// @notice System program ID (`11111111111111111111111111111111`)
    bytes32 private constant SYSTEM_PROGRAM = bytes32(0);

    /// @notice Anchor discriminator for `ift_mint`: sha256("global:ift_mint")[0:8]
    bytes8 private constant IFT_MINT_DISCRIMINATOR = 0x9cc9d99072afaa53;

    /// @notice Position where the payer account is injected by GMP's `on_recv_packet` (0-indexed)
    uint32 private constant PAYER_POSITION = 8;

    /// @notice Expected length of receiver: "0x" + 64 hex (wallet) + 64 hex (ATA) = 130 chars
    uint256 private constant SOLANA_RECEIVER_HEX_LENGTH = 130;

    /// @notice Each packed account entry is 34 bytes: 32 (pubkey) + 1 (is_signer) + 1 (is_writable)
    uint256 private constant PACKED_ACCOUNT_SIZE = 34;

    /// @notice Number of accounts in the payload (excluding payer, which is injected)
    uint256 private constant NUM_ACCOUNTS = 11;

    // -- Static PDAs (set at deployment) --

    /// @notice IFT app state PDA
    // natlint-disable-next-line MissingInheritdoc
    bytes32 public immutable APP_STATE;
    /// @notice IFT app mint state PDA
    // natlint-disable-next-line MissingInheritdoc
    bytes32 public immutable APP_MINT_STATE;
    /// @notice IFT bridge PDA
    // natlint-disable-next-line MissingInheritdoc
    bytes32 public immutable IFT_BRIDGE;
    /// @notice SPL token mint address
    // natlint-disable-next-line MissingInheritdoc
    bytes32 public immutable MINT;
    /// @notice IFT mint authority PDA
    // natlint-disable-next-line MissingInheritdoc
    bytes32 public immutable MINT_AUTHORITY;
    /// @notice GMP account PDA
    // natlint-disable-next-line MissingInheritdoc
    bytes32 public immutable GMP_ACCOUNT;

    /// @notice Error thrown when the receiver address is invalid
    /// @param receiver The invalid receiver string
    error SolanaIFTInvalidReceiver(string receiver);

    /// @notice Initializes the constructor with 6 static Solana PDAs.
    /// @param _appState IFT app state PDA
    /// @param _appMintState IFT app mint state PDA
    /// @param _iftBridge IFT bridge PDA
    /// @param _mint SPL token mint address
    /// @param _mintAuthority IFT mint authority PDA
    /// @param _gmpAccount GMP account PDA
    constructor(
        bytes32 _appState,
        bytes32 _appMintState,
        bytes32 _iftBridge,
        bytes32 _mint,
        bytes32 _mintAuthority,
        bytes32 _gmpAccount
    ) {
        APP_STATE = _appState;
        APP_MINT_STATE = _appMintState;
        IFT_BRIDGE = _iftBridge;
        MINT = _mint;
        MINT_AUTHORITY = _mintAuthority;
        GMP_ACCOUNT = _gmpAccount;
    }

    /// @inheritdoc IIFTSendCallConstructor
    /// @dev Receiver format: "0x" + wallet_hex(64) + ata_hex(64) = 130 chars.
    ///      Returns `abi.encode(packedAccounts, instructionData, payerPosition)`.
    function constructMintCall(string calldata receiver, uint256 amount) external view returns (bytes memory) {
        require(bytes(receiver).length == SOLANA_RECEIVER_HEX_LENGTH, SolanaIFTInvalidReceiver(receiver));

        (bytes32 wallet, bytes32 ata) = _parseWalletAndAta(receiver);

        bytes memory packedAccounts = _buildPackedAccounts(wallet, ata);
        bytes memory instructionData = _buildInstructionData(wallet, amount);

        return abi.encode(packedAccounts, instructionData, PAYER_POSITION);
    }

    /// @inheritdoc ERC165
    function supportsInterface(bytes4 interfaceId) public view virtual override(ERC165, IERC165) returns (bool) {
        return interfaceId == type(IIFTSendCallConstructor).interfaceId || super.supportsInterface(interfaceId);
    }

    /// @notice Parse wallet and ATA from the receiver hex string.
    /// @param receiver The receiver string in format "0x" + wallet_hex(64) + ata_hex(64)
    /// @return wallet The wallet pubkey
    /// @return ata The associated token account pubkey
    function _parseWalletAndAta(string calldata receiver) private pure returns (bytes32 wallet, bytes32 ata) {
        string memory walletHex = string(abi.encodePacked("0x", bytes(receiver)[2:66]));
        string memory ataHex = string(abi.encodePacked("0x", bytes(receiver)[66:130]));

        bool successW;
        bool successA;
        uint256 walletParsed;
        uint256 ataParsed;

        (successW, walletParsed) = Strings.tryParseHexUint(walletHex);
        require(successW, SolanaIFTInvalidReceiver(receiver));

        (successA, ataParsed) = Strings.tryParseHexUint(ataHex);
        require(successA, SolanaIFTInvalidReceiver(receiver));

        wallet = bytes32(walletParsed);
        ata = bytes32(ataParsed);
    }

    /// @notice Build packed accounts for the GmpSolanaPayload.
    /// @param wallet The receiver wallet pubkey
    /// @param ata The receiver associated token account
    /// @return packed The packed account bytes (11 * 34 bytes)
    function _buildPackedAccounts(bytes32 wallet, bytes32 ata) private view returns (bytes memory) {
        bytes memory packed = new bytes(NUM_ACCOUNTS * PACKED_ACCOUNT_SIZE);
        uint256 offset = 0;

        offset = _packAccount(packed, offset, APP_STATE, false, false);
        offset = _packAccount(packed, offset, APP_MINT_STATE, false, true);
        offset = _packAccount(packed, offset, IFT_BRIDGE, false, false);
        offset = _packAccount(packed, offset, MINT, false, true);
        offset = _packAccount(packed, offset, MINT_AUTHORITY, false, false);
        offset = _packAccount(packed, offset, ata, false, true);
        offset = _packAccount(packed, offset, wallet, false, false);
        offset = _packAccount(packed, offset, GMP_ACCOUNT, true, false);
        offset = _packAccount(packed, offset, TOKEN_PROGRAM, false, false);
        offset = _packAccount(packed, offset, ASSOCIATED_TOKEN_PROGRAM, false, false);
        _packAccount(packed, offset, SYSTEM_PROGRAM, false, false);

        return packed;
    }

    /// @notice Pack a single account entry: pubkey(32) + is_signer(1) + is_writable(1)
    /// @param buf The target byte buffer
    /// @param offset The current write offset
    /// @param pubkey The account pubkey
    /// @param isSigner Whether the account is a signer
    /// @param isWritable Whether the account is writable
    /// @return The new offset after writing
    function _packAccount(
        bytes memory buf,
        uint256 offset,
        bytes32 pubkey,
        bool isSigner,
        bool isWritable
    )
        private
        pure
        returns (uint256)
    {
        assembly {
            let ptr := add(add(buf, 32), offset)
            mstore(ptr, pubkey)
            mstore8(add(ptr, 32), isSigner)
            mstore8(add(ptr, 33), isWritable)
        }
        return offset + PACKED_ACCOUNT_SIZE;
    }

    /// @notice Build instruction data: discriminator(8) + wallet(32) + amount_le(8) = 48 bytes.
    /// @param wallet The receiver wallet pubkey
    /// @param amount The amount to mint
    /// @return data The Borsh-encoded instruction data matching Anchor's IFTMintMsg
    function _buildInstructionData(bytes32 wallet, uint256 amount) private pure returns (bytes memory) {
        bytes memory data = new bytes(48);

        bytes8 disc = IFT_MINT_DISCRIMINATOR;
        assembly {
            let ptr := add(data, 32)
            mstore(ptr, disc)
        }

        assembly {
            let ptr := add(add(data, 32), 8)
            mstore(ptr, wallet)
        }

        uint64 amountU64 = uint64(amount);
        assembly {
            let ptr := add(add(data, 32), 40)
            mstore8(ptr, and(amountU64, 0xff))
            mstore8(add(ptr, 1), and(shr(8, amountU64), 0xff))
            mstore8(add(ptr, 2), and(shr(16, amountU64), 0xff))
            mstore8(add(ptr, 3), and(shr(24, amountU64), 0xff))
            mstore8(add(ptr, 4), and(shr(32, amountU64), 0xff))
            mstore8(add(ptr, 5), and(shr(40, amountU64), 0xff))
            mstore8(add(ptr, 6), and(shr(48, amountU64), 0xff))
            mstore8(add(ptr, 7), and(shr(56, amountU64), 0xff))
        }

        return data;
    }
}
