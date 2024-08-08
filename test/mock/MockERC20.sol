// SPDX-License-Identifier: UNLICENSED
pragma solidity >=0.8.25 <0.9.0;

// solhint-disable custom-errors,max-line-length
import { ERC20 } from "@openzeppelin/contracts/token/ERC20/ERC20.sol";

contract MockERC20 is ERC20 {
    constructor() ERC20("BaseToken", "BTK") {
        _mint(msg.sender, 100_000_000_000_000_000_000); // Mint some tokens for testing
    }
}
