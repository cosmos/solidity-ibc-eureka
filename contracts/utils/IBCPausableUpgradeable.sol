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
    /// @notice Storage of the IBCPausable contract
    /// @dev It's implemented on a custom ERC-7201 namespace to reduce the risk of storage collisions when using with
    /// upgradeable contracts.
    /// @param _pauser The address that can pause and unpause the contract
    struct IBCPausableStorage {
        address _pauser;
    }

    /// @notice ERC-7201 slot for the IBCPausable storage
    /// @dev keccak256(abi.encode(uint256(keccak256("ibc.storage.IBCPausable")) - 1)) &
    /// ~bytes32(uint256(0xff))
    bytes32 private constant IBCPAUSABLE_STORAGE_SLOT =
        0xf205aa58a40d121ba2119531ecfad12344b90ab99f75444bf95259654539d700;

    /**
     * @dev Initializes the contract in unpaused state.
     */
    function __IBCPausable_init(address pauser) internal onlyInitializing {
        __Pausable_init();

        _getIBCPausableStorage()._pauser = pauser;
    }

    /// @inheritdoc IIBCPausableUpgradeable
    function getPauser() public view returns (address) {
        return _getIBCPausableStorage()._pauser;
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
        _getIBCPausableStorage()._pauser = pauser;
    }

    /// @notice Authorizes the setting of a new pauser
    /// @param pauser The new address that can pause and unpause the contract
    /// @dev This function must be overridden to add authorization logic
    function _authorizeSetPauser(address pauser) internal virtual;

    /// @notice Returns the storage of the IBCPausableUpgradeable contract
    function _getIBCPausableStorage() internal pure returns (IBCPausableStorage storage $) {
        // solhint-disable-next-line no-inline-assembly
        assembly {
            $.slot := IBCPAUSABLE_STORAGE_SLOT
        }
    }

    /// @notice Modifier to make a function callable only by the pauser
    modifier onlyPauser() {
        require(_msgSender() == getPauser(), Unauthorized());
        _;
    }
}
