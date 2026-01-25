// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

import { IFTBaseUpgradeable } from "../../../contracts/utils/IFTBaseUpgradeable.sol";
import { OwnableUpgradeable } from "@openzeppelin-upgradeable/access/OwnableUpgradeable.sol";
import { UUPSUpgradeable } from "@openzeppelin-contracts/proxy/utils/UUPSUpgradeable.sol";

/// @title TestIFT - IFT implementation for e2e testing with mint capability
contract TestIFT is IFTBaseUpgradeable, OwnableUpgradeable, UUPSUpgradeable {
    constructor() {
        _disableInitializers();
    }

    function initialize(
        address owner_,
        string calldata erc20Name,
        string calldata erc20Symbol,
        address ics27Gmp
    )
        external
        initializer
    {
        __Ownable_init(owner_);
        __IFTBase_init(erc20Name, erc20Symbol, ics27Gmp);
    }

    function mint(address to, uint256 amount) external onlyOwner {
        _mint(to, amount);
    }

    // solhint-disable-next-line no-empty-blocks
    function _onlyAuthority() internal view override(IFTBaseUpgradeable) onlyOwner { }

    // solhint-disable-next-line no-empty-blocks
    function _authorizeUpgrade(address) internal view override(UUPSUpgradeable) onlyOwner { }
}
