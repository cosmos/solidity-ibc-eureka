// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

import { ERC20 } from "@openzeppelin-contracts/token/ERC20/ERC20.sol";
import { IICS20Transfer } from "../interfaces/IICS20Transfer.sol";
import { IIBCERC20 } from "../interfaces/IIBCERC20.sol";
import { IEscrow } from "../interfaces/IEscrow.sol";
import { ICS20Lib } from "../utils/ICS20Lib.sol";

contract IBCERC20 is IIBCERC20, ERC20 {
    /// @notice The full IBC denom path for this token
    ICS20Lib.Denom private _denom;
    /// @notice The escrow contract address
    address public immutable ESCROW;
    /// @notice The ICS20 contract address
    address public immutable ICS20;

    /// @notice Unauthorized function call
    /// @param caller The caller of the function
    error IBCERC20Unauthorized(address caller);

    /// @notice Invalid denom
    /// @param denom The invalid denom
    error IBCERC20InvalidDenom(ICS20Lib.Denom denom);

    constructor(
        IICS20Transfer ics20_,
        IEscrow escrow_,
        ICS20Lib.Denom memory denom_
    )
        ERC20(ICS20Lib.getPath(denom_), denom_.base)
    {
        require(bytes(denom_.base).length > 0, IBCERC20InvalidDenom(denom_));
        require(denom_.trace.length > 0, IBCERC20InvalidDenom(denom_));

        // copying into storage to avoid "Copying of type struct ... to storage not yet supported"
        _denom.base = denom_.base;
        for (uint256 i = 0; i < denom_.trace.length; i++) {
            _denom.trace.push(denom_.trace[i]);
        }
        ESCROW = address(escrow_);
        ICS20 = address(ics20_);
    }

    /// @inheritdoc IIBCERC20
    function fullDenom() public view returns (ICS20Lib.Denom memory) {
        return _denom;
    }

    /// @inheritdoc IIBCERC20
    function mint(uint256 amount) external onlyICS20 {
        _mint(ESCROW, amount);
    }

    /// @inheritdoc IIBCERC20
    function burn(uint256 amount) external onlyICS20 {
        _burn(ESCROW, amount);
    }

    modifier onlyICS20() {
        require(_msgSender() == ICS20, IBCERC20Unauthorized(_msgSender()));
        _;
    }
}
