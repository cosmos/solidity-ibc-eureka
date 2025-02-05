// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

import { IIBCPausableUpgradeableErrors } from "../errors/IIBCPausableUpgradeableErrors.sol";
import { IIBCPausableUpgradeable } from "../interfaces/IIBCPausableUpgradeable.sol";
import { ContextUpgradeable } from "@openzeppelin-upgradeable/utils/ContextUpgradeable.sol";
import { PausableUpgradeable } from "@openzeppelin-upgradeable/utils/PausableUpgradeable.sol";

/// @title IBC Pausable Upgradeable contract
/// @notice This contract is an abstract contract for adding pausability to IBC contracts.
abstract contract IBCPausableUpgradeable is
    IIBCPausableUpgradeableErrors,
    IIBCPausableUpgradeable,
    ContextUpgradeable,
    PausableUpgradeable
{
    /// @notice Storage of the IBCPausableUpgradeable contract
    /// @dev It's implemented on a custom ERC-7201 namespace to reduce the risk of storage collisions when using with
    /// upgradeable contracts.
    /// @param _pauser The address that can pause and unpause the contract
    struct IBCPausableUpgradeableStorage {
        address _pauser;
    }

    /// @notice ERC-7201 slot for the IBCPausableUpgradeable storage
    /// @dev keccak256(abi.encode(uint256(keccak256("ibc.storage.IBCPausableUpgradeable")) - 1)) &
    /// ~bytes32(uint256(0xff))
    bytes32 private constant IBCPAUSABLEUPGRADEABLE_STORAGE_SLOT =
        0x3cb0d659d6ec9ab9509297c9cf14e29ed0165d10590ef43eb31ba393e648af00;

    /**
     * @dev Initializes the contract in unpaused state.
     */
    function __IBCPausable_init(address pauser) internal onlyInitializing {
        __Pausable_init();

        _getIBCPausableUpgradeableStorage()._pauser = pauser;
    }

    /// @inheritdoc IIBCPausableUpgradeable
    function getPauser() public view returns (address) {
        return _getIBCPausableUpgradeableStorage()._pauser;
    }

    /// @inheritdoc IIBCPausableUpgradeable
    function pause() external onlyPauser {
        _pause();
    }

    /// @inheritdoc IIBCPausableUpgradeable
    function unpause() external onlyPauser {
        _unpause();
    }

    /// @inheritdoc IIBCPausableUpgradeable
    function setPauser(address pauser) public {
        _authorizeSetPauser(pauser);
        _getIBCPausableUpgradeableStorage()._pauser = pauser;
    }

    /// @notice Authorizes the setting of a new pauser
    /// @param pauser The new address that can pause and unpause the contract
    /// @dev This function must be overridden to add authorization logic
    function _authorizeSetPauser(address pauser) internal virtual;

    /// @notice Returns the storage of the IBCPausableUpgradeable contract
    function _getIBCPausableUpgradeableStorage() internal pure returns (IBCPausableUpgradeableStorage storage $) {
        // solhint-disable-next-line no-inline-assembly
        assembly {
            $.slot := IBCPAUSABLEUPGRADEABLE_STORAGE_SLOT
        }
    }

    /// @notice Modifier to make a function callable only by the pauser
    modifier onlyPauser() {
        require(_msgSender() == getPauser(), Unauthorized());
        _;
    }
}
