// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

import { IFTBaseUpgradeable } from "./IFTBaseUpgradeable.sol";
import { OwnableUpgradeable } from "@openzeppelin-upgradeable/access/OwnableUpgradeable.sol";
import { Initializable } from "@openzeppelin-upgradeable/proxy/utils/Initializable.sol";
import { UUPSUpgradeable } from "@openzeppelin-contracts/proxy/utils/UUPSUpgradeable.sol";

/// @title IFT Ownable
/// @notice This is the ownable and upgradable implementation of IFT
/// @dev If you need a custom IFT implementation, then inherit from IFTBaseUpgradeable instead of deploying this
/// contract directly @dev WARNING: This contract is experimental
contract IFTOwnable is IFTBaseUpgradeable, OwnableUpgradeable, UUPSUpgradeable {
    /// @notice Initializes the IFTOwnable contract with the given owner
    /// @param owner_ The owner of the contract
    // natlint-disable-next-line MissingInheritdoc
    function initialize(address owner_) external initializer {
        __Ownable_init(owner_);
    }

    /// @inheritdoc IFTBaseUpgradeable
    function _onlyAuthority() internal view override(IFTBaseUpgradeable) onlyOwner { }
    // solhint-disable-previous-line no-empty-blocks

    /// @inheritdoc UUPSUpgradeable
    function _authorizeUpgrade(address) internal view override(UUPSUpgradeable) onlyOwner { }
    // solhint-disable-previous-line no-empty-blocks
}
