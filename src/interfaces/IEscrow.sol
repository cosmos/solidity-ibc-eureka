// SPDX-License-Identifier: MIT
pragma solidity >=0.8.25;

import { IERC20 } from "@openzeppelin/contracts/token/ERC20/IERC20.sol";

/// @title Escrow Contract Interface
/// @notice This interface is implemented by the Escrow contract
interface IEscrow {
    /// @notice Approve the owner to spend the token on behalf of the contract
    /// @param token The token to approve
    function approve(IERC20 token) external;
}
