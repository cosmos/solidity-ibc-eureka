// SPDX-License-Identifier: MIT
pragma solidity >=0.8.25;

import { ERC20 } from "@openzeppelin/contracts/token/ERC20/ERC20.sol";
import { IICS20Transfer } from "../interfaces/IICS20Transfer.sol";
import { Ownable } from "@openzeppelin/contracts/access/Ownable.sol";
import { IIBCERC20 } from "../interfaces/IIBCERC20.sol";

contract IBCERC20 is IIBCERC20, ERC20, Ownable {
    uint8 public constant DEFAULT_COSMOS_DECIMALS = 6;
    uint8 private _decimals;
    // TODO: Figure out naming and symbol for IBC denoms
    constructor(IICS20Transfer owner_) ERC20("IBC Token", "IBC") Ownable(address(owner_)) { 
        _decimals = DEFAULT_COSMOS_DECIMALS;
    }
        
    /// @inheritdoc IIBCERC20
    function mint(uint256 amount) external onlyOwner {
        _mint(owner(), amount);
    }

    /// @inheritdoc IIBCERC20
    function burn(uint256 amount) external onlyOwner {
        _burn(owner(), amount);
    }

    function decimals() public view override returns (uint8) {
        return _decimals;
    }
}
