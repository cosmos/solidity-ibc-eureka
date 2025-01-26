// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

import { ICS20Lib } from "../utils/ICS20Lib.sol";
import { IERC20 } from "@openzeppelin-contracts/token/ERC20/IERC20.sol";

interface IIBCERC20 is IERC20 {
    /// @notice Mint new tokens to the Escrow contract
    /// @param amount Amount of tokens to mint
    function mint(uint256 amount) external;

    /// @notice Burn tokens from the Escrow contract
    /// @param amount Amount of tokens to burn
    function burn(uint256 amount) external;

    /// @notice Get the full denom path of the token
    /// @return the full path of the token's denom
    function fullDenom() external view returns (ICS20Lib.Denom memory);
}
