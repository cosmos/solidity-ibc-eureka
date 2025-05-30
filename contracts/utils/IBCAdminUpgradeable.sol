// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

import { IIBCAdminErrors } from "../errors/IIBCAdminErrors.sol";
import { UUPSUpgradeable } from "@openzeppelin-contracts/proxy/utils/UUPSUpgradeable.sol";
import { IIBCAdmin } from "../interfaces/IIBCAdmin.sol";
import { AccessControlUpgradeable } from "@openzeppelin-upgradeable/access/AccessControlUpgradeable.sol";
import { ContextUpgradeable } from "@openzeppelin-upgradeable/utils/ContextUpgradeable.sol";
import { Initializable } from "@openzeppelin-upgradeable/proxy/utils/Initializable.sol";

/// @title IBC Admin Upgradeable
/// @notice This contract is an contract for tracking the admins of IBC contracts.
/// @dev This contract is developed with OpenZeppelin's UUPS upgradeable proxy pattern.
/// @dev This contract is meant to own AccessManager which is used to control access to IBC contracts.
/// @dev This contract manages two roles: the timelocked admin, and the governance admin. The timelocked admin
/// represents a timelocked security council, and the governance admin represents an interchain account from the
/// governance of a counterparty chain. The timelocked admin must be set during initialization, and the governance admin
/// should be set later by the timelocked admin.
/// @dev We recommend using `openzeppelin-contracts/contracts/governance/TimelockController.sol` for the timelocked
/// admin
contract IBCAdminUpgradeable is
    IIBCAdminErrors,
    IIBCAdmin,
    UUPSUpgradeable,
    Initializable,
    ContextUpgradeable
{
    /// @notice Storage of the IBCUUPSUpgradeable contract
    /// @dev It's implemented on a custom ERC-7201 namespace to reduce the risk of storage collisions when using with
    /// upgradeable contracts.
    /// @param timelockedAdmin The timelocked admin address, assumed to be timelocked
    /// @param govAdmin The governance admin address
    struct IBCAdminUpgradeableStorage {
        address timelockedAdmin;
        address govAdmin;
    }

    /// @notice ERC-7201 slot for the IBCUUPSUpgradeable storage
    /// @dev keccak256(abi.encode(uint256(keccak256("ibc.storage.IBCAdmin")) - 1)) & ~bytes32(uint256(0xff))
    bytes32 private constant IBCADMIN_STORAGE_SLOT =
        0xe6e017e6d032847d14d3fdd5f3faaa2f9e83c12c4889c3b8ac9728003f643a00;

    /// @notice This funtion initializes the timelockedAdmin, and the govAdmin should be set by the timelockedAdmin
    /// later
    /// @dev It makes sense to have the timelockedAdmin not be timelocked until the govAdmin is set
    /// @param timelockedAdmin The timelocked admin address, assumed to be timelocked
    function initialize(address timelockedAdmin) external initializer {
	__Context_init();
        _getIBCAdminUpgradeableStorage().timelockedAdmin = timelockedAdmin;
    }

    /// @inheritdoc IIBCAdmin
    function getTimelockedAdmin() external view returns (address) {
        return _getIBCAdminUpgradeableStorage().timelockedAdmin;
    }

    /// @inheritdoc IIBCAdmin
    function getGovAdmin() external view returns (address) {
        return _getIBCAdminUpgradeableStorage().govAdmin;
    }

    /// @inheritdoc IIBCAdmin
    function setTimelockedAdmin(address newTimelockedAdmin) external onlyAdmin {
        IBCAdminUpgradeableStorage storage $ = _getIBCAdminUpgradeableStorage();
        $.timelockedAdmin = newTimelockedAdmin;
    }

    /// @inheritdoc IIBCAdmin
    function setGovAdmin(address newGovAdmin) external onlyAdmin {
        IBCAdminUpgradeableStorage storage $ = _getIBCAdminUpgradeableStorage();
        $.govAdmin = newGovAdmin;
    }

    /// @inheritdoc UUPSUpgradeable
    function _authorizeUpgrade(address) internal view virtual override(UUPSUpgradeable) onlyAdmin { }
    // solhint-disable-previous-line no-empty-blocks

    /// @notice Returns the storage of the IBCUUPSUpgradeable contract
    /// @return $ The storage of the IBCUUPSUpgradeable contract
    function _getIBCAdminUpgradeableStorage() internal pure returns (IBCAdminUpgradeableStorage storage $) {
        // solhint-disable-next-line no-inline-assembly
        assembly {
            $.slot := IBCADMIN_STORAGE_SLOT
        }
    }

    /// @notice Modifier to check if the caller is an admin
    modifier onlyAdmin() {
        IBCAdminUpgradeableStorage storage $ = _getIBCAdminUpgradeableStorage();
        require(_msgSender() == $.timelockedAdmin || _msgSender() == $.govAdmin, Unauthorized());
        _;
    }
}
