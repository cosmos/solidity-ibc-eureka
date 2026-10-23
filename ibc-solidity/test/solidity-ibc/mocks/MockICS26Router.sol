// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.28;

import { AccessManaged } from "@openzeppelin-contracts/access/manager/AccessManaged.sol";

/// @notice Stand-in for the `ICS26Router` a light client is bound to. It only exposes the router's `authority()`.
contract MockICS26Router is AccessManaged {
    constructor(address authority) AccessManaged(authority) { }
}
