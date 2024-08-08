// SPDX-License-Identifier: UNLICENSED
pragma solidity >=0.8.25 <0.9.0;

// solhint-disable custom-errors,max-line-length
import { ERC20 } from "@openzeppelin/contracts/token/ERC20/ERC20.sol";

contract MockERC20Metadata is ERC20 {
    uint8 private _decimals;

    constructor(uint8 decimals_) ERC20("MetadataToken", "MTK") {
        _decimals = decimals_;
        _mint(msg.sender, 100_000_000_000_000_000_000); // Mint some tokens for testing
    }

    function decimals() public view override returns (uint8) {
        return _decimals;
    }
}
