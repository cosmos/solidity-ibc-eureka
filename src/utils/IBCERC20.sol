// SPDX-License-Identifier: MIT
pragma solidity >=0.8.25;

import { ERC20 } from "@openzeppelin/contracts/token/ERC20/ERC20.sol";
import { IICS20Transfer } from "../interfaces/IICS20Transfer.sol";
import { Ownable } from "@openzeppelin/contracts/access/Ownable.sol";

contract IBCERC20 is ERC20, Ownable {
    // TODO: Figure out naming and symbol for IBC denoms
    constructor(IICS20Transfer owner_) ERC20("IBC Token", "IBC") Ownable(address(owner_)) { }

    function mint(uint256 amount) external onlyOwner {
        _mint(owner(), amount);
    }

    function burn(uint256 amount) external onlyOwner {
        _burn(owner(), amount);
    }
}
