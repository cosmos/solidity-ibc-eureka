// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

import { IIBCAdminErrors } from "../errors/IIBCAdminErrors.sol";
import { IIBCAdmin } from "../interfaces/IIBCAdmin.sol";
import { IAccessManager } from "@openzeppelin-contracts/access/manager/IAccessManager.sol";

import { UUPSUpgradeable } from "@openzeppelin-contracts/proxy/utils/UUPSUpgradeable.sol";
import { AccessControlUpgradeable } from "@openzeppelin-upgradeable/access/AccessControlUpgradeable.sol";
import { ContextUpgradeable } from "@openzeppelin-upgradeable/utils/ContextUpgradeable.sol";
import { Initializable } from "@openzeppelin-upgradeable/proxy/utils/Initializable.sol";
import { IBCRolesLib } from "./IBCRolesLib.sol";

/// @title IBC Admin
/// @notice This contract is a contract for tracking the admins of IBC contracts.
/// @dev This contract is developed with OpenZeppelin's UUPS upgradeable proxy pattern.
/// @dev This contract is meant to own AccessManager which is used to control access to IBC contracts.
/// @dev This contract manages two roles: the timelocked admin, and the governance admin. The timelocked admin
/// represents a timelocked security council, and the governance admin represents an interchain account from the
/// governance of a counterparty chain. The timelocked admin must be set during initialization, and the governance admin
/// should be set later by the timelocked admin.
/// @dev We recommend using `openzeppelin-contracts/contracts/governance/TimelockController.sol` for the timelocked
/// admin
contract IBCAdmin is IIBCAdminErrors, IIBCAdmin, UUPSUpgradeable, Initializable, ContextUpgradeable {
    /// @notice Storage of the IBCUUPSUpgradeable contract
    /// @dev It's implemented on a custom ERC-7201 namespace to reduce the risk of storage collisions when using with
    /// upgradeable contracts.
    /// @param timelockedAdmin The timelocked admin address, assumed to be timelocked
    /// @param govAdmin The governance admin address
    /// @param accessManager The address of the AccessManager contract, which this contract is an admin of
    struct IBCAdminStorage {
        address _timelockedAdmin;
        address _govAdmin;
        IAccessManager _accessManager;
    }

    /// @notice ERC-7201 slot for the IBCUUPSUpgradeable storage
    /// @dev keccak256(abi.encode(uint256(keccak256("ibc.storage.IBCAdmin")) - 1)) & ~bytes32(uint256(0xff))
    bytes32 private constant IBCADMIN_STORAGE_SLOT = 0xe6e017e6d032847d14d3fdd5f3faaa2f9e83c12c4889c3b8ac9728003f643a00;

    /// @notice This funtion initializes the timelockedAdmin, and the govAdmin should be set by the timelockedAdmin
    /// later
    /// @dev It makes sense to have the timelockedAdmin not be timelocked until the govAdmin is set
    /// @param timelockedAdmin_ The timelocked admin address, assumed to be timelocked
    /// @param accessManager_ The address of the AccessManager contract, which this contract is an admin of
    function initialize(address timelockedAdmin_, address accessManager_) external initializer {
        __Context_init();

        IBCAdminStorage storage $ = _getIBCAdminStorage();
        $._timelockedAdmin = timelockedAdmin_;
        $._accessManager = IAccessManager(accessManager_);
    }

    /// @inheritdoc IIBCAdmin
    function timelockedAdmin() external view returns (address) {
        return _getIBCAdminStorage()._timelockedAdmin;
    }

    /// @inheritdoc IIBCAdmin
    function govAdmin() external view returns (address) {
        return _getIBCAdminStorage()._govAdmin;
    }

    /// @inheritdoc IIBCAdmin
    function accessManager() external view returns (address) {
        return address(_getIBCAdminStorage()._accessManager);
    }

    /// @inheritdoc IIBCAdmin
    function setTimelockedAdmin(address newTimelockedAdmin) external onlyAdmin {
        IBCAdminStorage storage $ = _getIBCAdminStorage();
        $._accessManager.revokeRole(IBCRolesLib.ADMIN_ROLE, $._timelockedAdmin);
        $._timelockedAdmin = newTimelockedAdmin;
        $._accessManager.grantRole(IBCRolesLib.ADMIN_ROLE, newTimelockedAdmin, 0);
    }

    /// @inheritdoc IIBCAdmin
    function setGovAdmin(address newGovAdmin) external onlyAdmin {
        IBCAdminStorage storage $ = _getIBCAdminStorage();
        if ($._govAdmin != address(0)) {
            $._accessManager.revokeRole(IBCRolesLib.ADMIN_ROLE, $._govAdmin);
        }
        $._govAdmin = newGovAdmin;
        $._accessManager.grantRole(IBCRolesLib.ADMIN_ROLE, newGovAdmin, 0);
    }

    /// @inheritdoc IIBCAdmin
    function setAccessManager(address newAccessManager) external onlyAdmin {
        IBCAdminStorage storage $ = _getIBCAdminStorage();
        $._accessManager = IAccessManager(newAccessManager);
    }

    /// @inheritdoc UUPSUpgradeable
    function _authorizeUpgrade(address) internal view virtual override(UUPSUpgradeable) onlyAdmin { }
    // solhint-disable-previous-line no-empty-blocks

    /// @notice Returns the storage of the IBCUUPSUpgradeable contract
    /// @return $ The storage of the IBCUUPSUpgradeable contract
    function _getIBCAdminStorage() internal pure returns (IBCAdminStorage storage $) {
        // solhint-disable-next-line no-inline-assembly
        assembly {
            $.slot := IBCADMIN_STORAGE_SLOT
        }
    }

    /// @notice Modifier to check if the caller is an admin
    modifier onlyAdmin() {
        IBCAdminStorage storage $ = _getIBCAdminStorage();
        require(_msgSender() == $._timelockedAdmin || _msgSender() == $._govAdmin, Unauthorized());
        _;
    }
}
