// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

import { ERC20 } from "@openzeppelin/token/ERC20/ERC20.sol";
import { IICS20Transfer } from "../interfaces/IICS20Transfer.sol";
import { Ownable } from "@openzeppelin/access/Ownable.sol";
import { IIBCERC20 } from "../interfaces/IIBCERC20.sol";
import { IEscrow } from "../interfaces/IEscrow.sol";

contract IBCERC20 is IIBCERC20, ERC20, Ownable {
    /// @notice The full IBC denom path for this token
    string private _fullDenomPath;
    /// @notice The escrow contract address
    IEscrow private immutable ESCROW;

    constructor(
        IICS20Transfer owner_,
        IEscrow escrow_,
        string memory ibcDenom_,
        string memory baseDenom_,
        string memory fullDenomPath_
    )
        ERC20(ibcDenom_, baseDenom_)
        Ownable(address(owner_))
    {
        _fullDenomPath = fullDenomPath_;
        ESCROW = escrow_;
    }

    /// @inheritdoc IIBCERC20
    function fullDenomPath() public view returns (string memory) {
        return _fullDenomPath;
    }

    /// @inheritdoc IIBCERC20
    function mint(uint256 amount) external onlyOwner {
        _mint(address(ESCROW), amount);
    }

    /// @inheritdoc IIBCERC20
    function burn(uint256 amount) external onlyOwner {
        _burn(address(ESCROW), amount);
    }
}
