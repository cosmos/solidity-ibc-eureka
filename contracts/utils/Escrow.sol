// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

import { SafeERC20 } from "@openzeppelin-contracts/token/ERC20/utils/SafeERC20.sol";
import { IERC20 } from "@openzeppelin-contracts/token/ERC20/IERC20.sol";
import { IEscrow } from "../interfaces/IEscrow.sol";
import { ContextUpgradeable } from "@openzeppelin-upgradeable/utils/ContextUpgradeable.sol";

using SafeERC20 for IERC20;

/// @title Escrow Contract
/// @notice This contract is used to escrow the funds for the ICS20 contract
contract Escrow is IEscrow, ContextUpgradeable {
    /// @notice Storage of the Escrow contract
    /// @dev It's implemented on a custom ERC-7201 namespace to reduce the risk of storage collisions when using with
    /// upgradeable contracts.
    /// @param _ics20 The ICS20 contract address, can send funds from the escrow
    struct EscrowStorage {
        address _ics20;
    }

    /// @notice ERC-7201 slot for the Escrow storage
    /// @dev keccak256(abi.encode(uint256(keccak256("ibc.storage.Escrow")) - 1)) & ~bytes32(uint256(0xff))
    bytes32 private constant ESCROW_STORAGE_SLOT = 0x537eb9d931756581e7ea6f7811162c646321946650ac0ac6bf83b24932e41600;

    /// @notice Unauthorized function call
    /// @param caller The caller of the function
    error EscrowUnauthorized(address caller);

    /// @dev This contract is meant to be deployed by a proxy, so the constructor is not used
    constructor() {
        _disableInitializers();
    }

    /// @inheritdoc IEscrow
    function initialize(address ics20_) external initializer {
        __Context_init();

        EscrowStorage storage $ = _getEscrowStorage();

        $._ics20 = ics20_;
    }

    /// @inheritdoc IEscrow
    function send(IERC20 token, address to, uint256 amount) external override onlyICS20 {
        token.safeTransfer(to, amount);
    }

    /// @inheritdoc IEscrow
    function ics20() external view override returns (address) {
        return _getEscrowStorage()._ics20;
    }

    /// @notice Returns the storage of the Escrow contract
    function _getEscrowStorage() private pure returns (EscrowStorage storage $) {
        // solhint-disable-next-line no-inline-assembly
        assembly {
            $.slot := ESCROW_STORAGE_SLOT
        }
    }

    /// @notice Modifier to check if the caller is the ICS20 contract
    modifier onlyICS20() {
        require(_msgSender() == _getEscrowStorage()._ics20, EscrowUnauthorized(_msgSender()));
        _;
    }
}
