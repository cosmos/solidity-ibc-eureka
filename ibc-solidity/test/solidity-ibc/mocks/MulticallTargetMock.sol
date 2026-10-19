// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.28;

import { AccessManaged } from "@openzeppelin-contracts/access/manager/AccessManaged.sol";

/// @title MulticallTargetMock
/// @notice Restricted target that records keys, so tests can see which calls of a multicall took effect
contract MulticallTargetMock is AccessManaged {
    mapping(uint256 key => bool isSet) public isSet;

    error MockFailure(uint256 key);

    constructor(address authority) AccessManaged(authority) { }

    function set(uint256 key) external restricted returns (uint256) {
        isSet[key] = true;
        return key;
    }

    /// @dev Sets `key` and then reverts, so the write must be rolled back
    function setAndFail(uint256 key) external restricted {
        isSet[key] = true;
        revert MockFailure(key);
    }
}
