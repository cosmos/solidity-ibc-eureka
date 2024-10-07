// SPDX-License-Identifier: MIT
pragma solidity >=0.8.25;

import { IERC20 } from "@openzeppelin/contracts/token/ERC20/IERC20.sol";
import { Ownable } from "@openzeppelin/contracts/access/Ownable.sol";
import { IEscrow } from "../interfaces/IEscrow.sol";

/// @title Escrow Contract
/// @notice This contract is used to escrow the funds for the ICS20 contract
contract Escrow is Ownable, IEscrow {
    /// @param owner_ The owner of the contract
    /// @dev Owner is to be the ICS20Transfer contract
    constructor(address owner_) Ownable(owner_) { }

    /// @inheritdoc IEscrow
    function approve(IERC20 token) external onlyOwner {
        token.approve(owner(), type(uint256).max);
    }
}
