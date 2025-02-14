// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.28;

// solhint-disable no-empty-blocks gas-custom-errors

import { ERC20 } from "@openzeppelin-contracts/token/ERC20/ERC20.sol";

contract TestnetERC20 is ERC20 {
    mapping(address adminAddress => bool isAdmin) public admins;

    constructor(address initialAdmin) ERC20("Testnet ERC20", "TNERC") { 
        admins[initialAdmin] = true;
    }

    function mint(address _to, uint256 _amount) external onlyAdmins{
        _mint(_to, _amount);
    }

    modifier onlyAdmins() {
        require(admins[msg.sender], "Not an admin");
        _;
    }
}
