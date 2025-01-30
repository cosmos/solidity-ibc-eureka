// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

// solhint-disable no-empty-blocks

import { Initializable } from "@openzeppelin-contracts/proxy/utils/Initializable.sol";
import { UUPSUpgradeable } from "@openzeppelin-contracts/proxy/utils/UUPSUpgradeable.sol";

contract DummyInitializable is Initializable, UUPSUpgradeable {
    function initializeV2() public reinitializer(2) { }

    function _authorizeUpgrade(address) internal override { }
}

contract ErroneousInitializable is Initializable, UUPSUpgradeable {
    error InitializeFailed();

    function initializeV2() public reinitializer(2) {
        revert InitializeFailed();
    }

    function _authorizeUpgrade(address) internal override { }
}
