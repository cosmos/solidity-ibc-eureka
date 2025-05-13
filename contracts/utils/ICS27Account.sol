// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

import { IICS27Errors } from "../errors/IICS27Errors.sol";
import { IICS27Account } from "../interfaces/IICS27Account.sol";

import { ContextUpgradeable } from "@openzeppelin-upgradeable/utils/ContextUpgradeable.sol";
import { Address } from "@openzeppelin-contracts/utils/Address.sol";

contract ICS27Account is IICS27Errors, IICS27Account, ContextUpgradeable {
    /// @notice Storage of the ICS27Account contract
    /// @dev It's implemented on a custom ERC-7201 namespace to reduce the risk of storage collisions when using with
    /// upgradeable contracts.
    /// @param _ics27 The ICS27GMP contract address. Immutable.
    /// @custom:storage-location erc7201:ibc.storage.ICS27Account
    struct ICS27AccountStorage {
        address _ics27;
    }

    /// @notice ERC-7201 slot for the ICS27Account storage
    /// @dev keccak256(abi.encode(uint256(keccak256("ibc.storage.ICS27Account")) - 1)) & ~bytes32(uint256(0xff))
    bytes32 private constant ICS27ACCOUNT_STORAGE_SLOT =
        0x319583b012a10c350515da7d8fdefe3c302490627bf79c0be5b739020ce32c00;

    /// @dev This contract is meant to be deployed by a proxy, so the constructor is not used
    constructor() {
        _disableInitializers();
    }

    /// @inheritdoc IICS27Account
    function initialize(address ics27_) external initializer {
        __Context_init();

        ICS27AccountStorage storage $ = _getICS27AccountStorage();
        $._ics27 = ics27_;
    }

    /// @inheritdoc IICS27Account
    function sendValue(address payable recipient, uint256 amount) external onlySelf {
        Address.sendValue(recipient, amount);
    }

    /// @inheritdoc IICS27Account
    function functionCall(address target, bytes memory data) external onlyICS27 returns (bytes memory) {
        return Address.functionCall(target, data);
    }

    /// @inheritdoc IICS27Account
    function functionCallWithValue(
        address target,
        bytes memory data,
        uint256 value
    )
        external
        onlySelf
        returns (bytes memory)
    {
        return Address.functionCallWithValue(target, data, value);
    }

    /// @inheritdoc IICS27Account
    function functionDelegateCall(address target, bytes calldata data) external onlySelf returns (bytes memory) {
        return Address.functionDelegateCall(target, data);
    }

    /// @inheritdoc IICS27Account
    function ics27() external view returns (address) {
        return _getICS27AccountStorage()._ics27;
    }

    /// @notice Returns the storage of the ICS27Account contract
    function _getICS27AccountStorage() private pure returns (ICS27AccountStorage storage $) {
        // solhint-disable-next-line no-inline-assembly
        assembly {
            $.slot := ICS27ACCOUNT_STORAGE_SLOT
        }
    }

    modifier onlyICS27() {
        address ics27_ = _getICS27AccountStorage()._ics27;
        require(_msgSender() == ics27_, ICS27Unauthorized(ics27_, _msgSender()));
        _;
    }

    modifier onlySelf() {
        require(_msgSender() == address(this), ICS27Unauthorized(address(this), _msgSender()));
        _;
    }
}
