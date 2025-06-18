// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

/// @title ICS26AdminsDeprecated
/// @notice This contract is a library for removing the deprecated and deleted IBCUUPSUpgradeable contract.
/// @dev This library will be deleted in the next release.
library ICS26AdminsDeprecated {
    /// @notice Storage of the IBCUUPSUpgradeable contract
    /// @dev It's implemented on a custom ERC-7201 namespace to reduce the risk of storage collisions when using with
    /// upgradeable contracts.
    /// @param timelockedAdmin The timelocked admin address, assumed to be timelocked
    /// @param govAdmin The governance admin address
    struct IBCUUPSUpgradeableStorage {
        address timelockedAdmin;
        address govAdmin;
    }

    /// @notice ERC-7201 slot for the IBCUUPSUpgradeable storage
    /// @dev keccak256(abi.encode(uint256(keccak256("ibc.storage.IBCUUPSUpgradeable")) - 1)) & ~bytes32(uint256(0xff))
    bytes32 private constant IBCUUPSUPGRADEABLE_STORAGE_SLOT =
        0xba83ed17c16070da0debaa680185af188d82c999a75962a12a40699ca48a2b00;

    /// @notice This function deinitializes the IBCUUPSUpgradeable contract as it is deprecated
    function __IBCUUPSUpgradeable_deinit() internal {
        IBCUUPSUpgradeableStorage storage $ = _getIBCUUPSUpgradeableStorage();
        delete $.timelockedAdmin;
        delete $.govAdmin;
    }

    /// @notice Checks if the given account is an admin
    /// @param account The address to check
    /// @return bool True if the account is an admin, false otherwise
    function isAdmin(address account) internal view returns (bool) {
        IBCUUPSUpgradeableStorage storage $ = _getIBCUUPSUpgradeableStorage();
        return account == $.timelockedAdmin || account == $.govAdmin;
    }

    /// @notice Returns the storage of the IBCUUPSUpgradeable contract
    /// @return $ The storage of the IBCUUPSUpgradeable contract
    function _getIBCUUPSUpgradeableStorage() internal pure returns (IBCUUPSUpgradeableStorage storage $) {
        // solhint-disable-next-line no-inline-assembly
        assembly {
            $.slot := IBCUUPSUPGRADEABLE_STORAGE_SLOT
        }
    }
}

/// @title IDeprecatedIBCUUPSUpgradeable
/// @notice This interface is used to interact with the deprecated IBCUUPSUpgradeable contract.
/// @dev This interface will be deleted in the next release.
interface IDeprecatedIBCUUPSUpgradeable {
    /// @notice Checks if the given account is an admin
    /// @param account The address to check
    /// @return bool True if the account is an admin, false otherwise
    function isAdmin(address account) external view returns (bool);
}
