// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

import { Initializable } from "@openzeppelin/proxy/utils/Initializable.sol";

contract DummyInitializable is Initializable {
    function initializeV2() public reinitializer(2) {
    }
}

contract ErroneousInitializable is Initializable {
    error InitializeFailed();

    function initializeV2() public reinitializer(2) {
        revert InitializeFailed();
    }
}
