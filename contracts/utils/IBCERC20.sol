// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

import { ERC20 } from "@openzeppelin-contracts/token/ERC20/ERC20.sol";
import { IICS20Transfer } from "../interfaces/IICS20Transfer.sol";
import { IIBCERC20 } from "../interfaces/IIBCERC20.sol";
import { IEscrow } from "../interfaces/IEscrow.sol";

contract IBCERC20 is IIBCERC20, ERC20 {
    /// @notice Storage of the IBCERC20 contract
    /// @dev It's implemented on a custom ERC-7201 namespace to reduce the risk of storage collisions when using with upgradeable contracts.
    /// @param _fullDenomPath The full IBC denom path for this token
    /// @param _escrow The escrow contract address
    /// @param _ics20 The ICS20 contract address
    struct IBCERC20Storage {
        string _fullDenomPath;
        address _escrow;
        address _ics20;
    }

    /// @notice ERC-7201 slot for the IBCERC20 storage
    /// @dev keccak256(abi.encode(uint256(keccak256("ibc.storage.IBCERC20")) - 1)) & ~bytes32(uint256(0xff))
    bytes32 private constant IBCERC20_STORAGE_SLOT = 0x1dd677b5a02f77610493322b5fdbbfdb607b541c6e6045daab3464e895dea800;

    /// @notice The full IBC denom path for this token
    string private _fullDenomPath;
    /// @notice The escrow contract address
    address public immutable ESCROW;
    /// @notice The ICS20 contract address
    address public immutable ICS20;

    /// @notice Unauthorized function call
    /// @param caller The caller of the function
    error IBCERC20Unauthorized(address caller);

    constructor(
        IICS20Transfer ics20_,
        IEscrow escrow_,
        string memory baseDenom_,
        string memory fullDenomPath_
    )
        ERC20(fullDenomPath_, baseDenom_)
    {
        _fullDenomPath = fullDenomPath_;
        ESCROW = address(escrow_);
        ICS20 = address(ics20_);
    }

    /// @inheritdoc IIBCERC20
    function fullDenomPath() public view returns (string memory) {
        return _fullDenomPath;
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
