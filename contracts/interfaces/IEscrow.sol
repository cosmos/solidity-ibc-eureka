// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

import { IERC20 } from "@openzeppelin-contracts/token/ERC20/IERC20.sol";

/// @title Escrow Contract Interface
/// @notice This interface is implemented by the Escrow contract
interface IEscrow {
    /// @notice Send tokens to the specified address
    /// @param token The token to send
    /// @param to The address to send the tokens to
    /// @param amount The amount of tokens to send
    function send(IERC20 token, address to, uint256 amount) external;
}
