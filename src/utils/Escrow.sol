// SPDX-License-Identifier: MIT
pragma solidity >=0.8.25;

import { SafeERC20 } from "@openzeppelin/contracts/token/ERC20/utils/SafeERC20.sol";
import { IERC20 } from "@openzeppelin/contracts/token/ERC20/IERC20.sol";
import { Ownable } from "@openzeppelin/contracts/access/Ownable.sol";
import { IEscrow } from "../interfaces/IEscrow.sol";

using SafeERC20 for IERC20;

/// @title Escrow Contract
/// @notice This contract is used to escrow the funds for the ICS20 contract
contract Escrow is Ownable, IEscrow {
    /// @param owner_ The owner of the contract
    /// @dev Owner is to be the ICS20Transfer contract
    constructor(address owner_) Ownable(owner_) { }

    /// @inheritdoc IEscrow
    function send(IERC20 token, address to, uint256 amount) external override onlyOwner {
        token.safeTransfer(to, amount);
    }
}
