// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

import { IERC20 } from "@openzeppelin-contracts/token/ERC20/IERC20.sol";
import { IICS20TransferMsgs } from "../msgs/IICS20TransferMsgs.sol";

interface IIBCERC20 is IERC20 {
    /// @notice Mint new tokens to the Escrow contract
    /// @param amount Amount of tokens to mint
    function mint(uint256 amount) external;

    /// @notice Burn tokens from the Escrow contract
    /// @param amount Amount of tokens to burn
    function burn(uint256 amount) external;

    /// @notice Get the full denom of the token
    /// @return the full token denom
    function fullDenom() external view returns (IICS20TransferMsgs.Denom memory);
}
