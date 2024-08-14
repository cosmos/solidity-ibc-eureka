// SPDX-License-Identifier: MIT
pragma solidity >=0.8.25;

import { ERC20 } from "@openzeppelin/contracts/token/ERC20/ERC20.sol";
import { IICS20Transfer } from "../interfaces/IICS20Transfer.sol";
import { Ownable } from "@openzeppelin/contracts/access/Ownable.sol";
import { IIBCERC20 } from "../interfaces/IIBCERC20.sol";

contract IBCERC20 is IIBCERC20, ERC20, Ownable {
    /// @notice default sdkCoin decimals. Used when deploying sdkCoin ERC20 representation
    uint8 public constant DEFAULT_COSMOS_DECIMALS = 6;
    // TODO: Figure out naming and symbol for IBC denoms
    constructor(IICS20Transfer owner_) ERC20("IBC Token", "IBC") Ownable(address(owner_)) {
    }

    /// @inheritdoc IIBCERC20
    function mint(uint256 amount) external onlyOwner {
        _mint(owner(), amount);
    }

    /// @inheritdoc IIBCERC20
    function burn(uint256 amount) external onlyOwner {
        _burn(owner(), amount);
    }

    /// @notice function to return the ERC20 token decimals
    function decimals() public pure override returns (uint8) {
        return DEFAULT_COSMOS_DECIMALS;
    }
}
