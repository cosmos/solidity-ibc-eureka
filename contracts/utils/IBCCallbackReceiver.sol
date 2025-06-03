// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

import { IIBCCallbacks } from "../interfaces/IIBCCallbacks.sol";

import { ERC165 } from "@openzeppelin-contracts/utils/introspection/ERC165.sol";

/// @title IBC Callback Receiver
/// @notice An abstract contract that implements the ERC165 interface for IIBCCallbacks.
abstract contract IBCCallbackReceiver is IIBCCallbacks, ERC165 {
    /// @inheritdoc ERC165
    function supportsInterface(bytes4 interfaceId) public view virtual override(ERC165) returns (bool) {
        return interfaceId == type(IIBCCallbacks).interfaceId || super.supportsInterface(interfaceId);
    }
}
