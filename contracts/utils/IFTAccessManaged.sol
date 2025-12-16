// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

import { IFTBaseUpgradeable } from "./IFTBaseUpgradeable.sol";
import { Initializable } from "@openzeppelin-upgradeable/proxy/utils/Initializable.sol";
import { AccessManagedUpgradeable } from "@openzeppelin-upgradeable/access/manager/AccessManagedUpgradeable.sol";
import { UUPSUpgradeable } from "@openzeppelin-contracts/proxy/utils/UUPSUpgradeable.sol";

/// @title IFT Access Managed
/// @notice This is the access managed and upgradable implementation of IFT
/// @dev If you need a custom IFT implementation, then inherit from IFTBaseUpgradeable instead of deploying this contract directly
/// @dev WARNING: This contract is experimental
contract IFTAccessManaged is IFTBaseUpgradeable, AccessManagedUpgradeable, UUPSUpgradeable {
    /// @notice Initializes the IFTOwnable contract with the given access manager
    /// @param authority_ The address of the AccessManager contract
    // natlint-disable-next-line MissingInheritdoc
    function initialize(address authority_) external initializer {
	__AccessManaged_init(authority_);
    }

    /// @inheritdoc IFTBaseUpgradeable
    function _onlyAuthority() internal override(IFTBaseUpgradeable) restricted {}
    // solhint-disable-previous-line no-empty-blocks

    /// @inheritdoc UUPSUpgradeable
    function _authorizeUpgrade(address) internal override(UUPSUpgradeable) restricted {}
    // solhint-disable-previous-line no-empty-blocks
}

