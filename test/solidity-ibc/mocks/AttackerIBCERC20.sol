// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

// solhint-disable no-empty-blocks

import { IIBCERC20 } from "../../../contracts/interfaces/IIBCERC20.sol";
import { ERC20 } from "@openzeppelin-contracts/token/ERC20/ERC20.sol";

contract AttackerIBCERC20 is IIBCERC20, ERC20 {
    address private escrowAddress;

    constructor(string memory fullDenomPath_, address escrowAddress_) ERC20(fullDenomPath_, fullDenomPath_) {
        escrowAddress = escrowAddress_;
    }

    /// @inheritdoc IIBCERC20
    function initialize(address, address, string memory) external { }

    /// @inheritdoc IIBCERC20
    function fullDenomPath() public pure returns (string memory) {
        return "transfer/client-0/uatom";
    }

    /// @inheritdoc IIBCERC20
    function mint(address, uint256) external { }

    function mintTo(address to, uint256 amount) external {
        _mint(to, amount);
    }

    /// @inheritdoc IIBCERC20
    function burn(address, uint256) external { }

    /// @inheritdoc IIBCERC20
    function escrow() external view returns (address) {
        return escrowAddress;
    }

    /// @inheritdoc IIBCERC20
    function ics20() external pure returns (address) {
        return address(0);
    }

    /// @inheritdoc IIBCERC20
    function grantMetadataSetterRole(address) external pure {
        revert("not implemented");
    }

    /// @inheritdoc IIBCERC20
    function revokeMetadataSetterRole(address) external pure {
        revert("not implemented");
    }

    /// @inheritdoc IIBCERC20
    function METADATA_SETTER_ROLE() external pure override returns (bytes32) {
        revert("not implemented");
    }

    /// @inheritdoc IIBCERC20
    function setMetadata(uint8, string calldata, string calldata) external pure {
        revert("not implemented");
    }
}
