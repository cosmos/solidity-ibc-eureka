// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

import { Initializable } from "@openzeppelin/proxy/utils/Initializable.sol";

contract DummyInitializable is Initializable {
    string public value;

    function initialize(string calldata _value) public reinitializer(2) {
        value = _value;
    }
}

contract ErroneousInitializable is Initializable {
    function initialize(string calldata) public initializer {
        revert("initialize failed");
    }
}
