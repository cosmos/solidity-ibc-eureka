// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

import { ERC20Upgradeable } from "@openzeppelin-upgradeable/token/ERC20/ERC20Upgradeable.sol";
import { IIBCERC20 } from "../interfaces/IIBCERC20.sol";
import { IIBCUUPSUpgradeable } from "../interfaces/IIBCUUPSUpgradeable.sol";
import { UUPSUpgradeable } from "@openzeppelin-contracts/proxy/utils/UUPSUpgradeable.sol";

contract IBCERC20 is IIBCERC20, ERC20Upgradeable, UUPSUpgradeable {
    /// @notice Storage of the IBCERC20 contract
    /// @dev It's implemented on a custom ERC-7201 namespace to reduce the risk of storage collisions when using with upgradeable contracts.
    /// @param _fullDenomPath The full IBC denom path for this token
    /// @param _escrow The escrow contract address
    /// @param _ics20 The ICS20 contract address
    /// @param _ics26 The ICS26 contract address, used for upgradeability
    struct IBCERC20Storage {
        string _fullDenomPath;
        address _escrow;
        address _ics20;
        IIBCUUPSUpgradeable _ics26;
    }

    /// @notice ERC-7201 slot for the IBCERC20 storage
    /// @dev keccak256(abi.encode(uint256(keccak256("ibc.storage.IBCERC20")) - 1)) & ~bytes32(uint256(0xff))
    bytes32 private constant IBCERC20_STORAGE_SLOT = 0x1dd677b5a02f77610493322b5fdbbfdb607b541c6e6045daab3464e895dea800;

    /// @notice Unauthorized function call
    /// @param caller The caller of the function
    error IBCERC20Unauthorized(address caller);

    /// @dev This contract is meant to be deployed by a proxy, so the constructor is not used
    constructor() {
        _disableInitializers();
    }

    /// @notice Initializes the IBCERC20 contract
    /// @dev This function is meant to be called by a proxy
    /// @param ics20_ The ICS20 contract address
    /// @param escrow_ The escrow contract address
    /// @param ics26_ The ICS26 contract address, used for upgradeability
    /// @param baseDenom_ The base denom for this token
    /// @param fullDenomPath_ The full IBC denom path for this token
    function initialize(
        address ics20_,
        address escrow_,
        address ics26_,
        string memory baseDenom_,
        string memory fullDenomPath_
    ) external initializer {
        __ERC20_init(fullDenomPath_, baseDenom_);

        IBCERC20Storage storage $ = _getIBCERC20Storage();

        $._fullDenomPath = fullDenomPath_;
        $._escrow = escrow_;
        $._ics20 = ics20_;
        $._ics26 = IIBCUUPSUpgradeable(ics26_);
    }

    /// @inheritdoc IIBCERC20
    function fullDenomPath() public view returns (string memory) {
        return _getIBCERC20Storage()._fullDenomPath;
    }

    /// @inheritdoc IIBCERC20
    function mint(uint256 amount) external onlyICS20 {
        _mint(_getIBCERC20Storage()._escrow, amount);
    }

    /// @inheritdoc IIBCERC20
    function burn(uint256 amount) external onlyICS20 {
        _burn(_getIBCERC20Storage()._escrow, amount);
    }

    /// @inheritdoc IIBCERC20
    function escrow() external view returns (address) {
        return _getIBCERC20Storage()._escrow;
    }

    /// @inheritdoc IIBCERC20
    function ics20() external view returns (address) {
        return _getIBCERC20Storage()._ics20;
    }

    /// @inheritdoc IIBCERC20
    function ics26() external view returns (address) {
        return address(_getIBCERC20Storage()._ics26);
    }

    /// @inheritdoc UUPSUpgradeable
    function _authorizeUpgrade(address) internal view override {
        require(_getIBCERC20Storage()._ics26.isAdmin(_msgSender()), IBCERC20Unauthorized(_msgSender()));
    }

    /// @notice Returns the storage of the IBCERC20 contract
    function _getIBCERC20Storage() private pure returns (IBCERC20Storage storage $) {
        // solhint-disable-next-line no-inline-assembly
        assembly {
            $.slot := IBCERC20_STORAGE_SLOT
        }
    }

    modifier onlyICS20() {
        require(_msgSender() == _getIBCERC20Storage()._ics20, IBCERC20Unauthorized(_msgSender()));
        _;
    }
}
