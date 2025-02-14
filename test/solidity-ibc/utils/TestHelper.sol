// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.28;

import { Vm } from "forge-std/Vm.sol";
import { Test } from "forge-std/Test.sol";

import { ICS20Lib } from "../../../contracts/utils/ICS20Lib.sol";
import { ICS24Host } from "../../../contracts/utils/ICS24Host.sol";

contract TestHelper is Test {
    /// @notice The first client ID used in the test
    string public constant FIRST_CLIENT_ID = "client-0";
    /// @notice The default starting balance for the ERC20 token
    uint256 public constant DEFAULT_ERC20_STARTING_BALANCE = type(uint256).max;

    /// @notice The default merkle prefix used in cosmos chains
    bytes[] private _cosmosMerklePrefix = [bytes("ibc"), bytes("")];
    /// @notice Empty merkle prefix used in the test
    bytes[] private _emptyMerklePrefix = [bytes("")];

    bytes[] private _singleSuccessAck = [ICS20Lib.SUCCESSFUL_ACKNOWLEDGEMENT_JSON];
    bytes[] private _singleErrorAck = [ICS24Host.UNIVERSAL_ERROR_ACK];

    function COSMOS_MERKLE_PREFIX() external view returns (bytes[] memory) {
        return _cosmosMerklePrefix;
    }

    function EMPTY_MERKLE_PREFIX() external view returns (bytes[] memory) {
        return _emptyMerklePrefix;
    }

    function SINGLE_SUCCESS_ACK() external view returns (bytes[] memory) {
        return _singleSuccessAck;
    }

    function SINGLE_ERROR_ACK() external view returns (bytes[] memory) {
        return _singleErrorAck;
    }

    /// @dev retuns a random base64 string
    function randomString() public returns (string memory) {
        uint256 randomNum = vm.randomUint();
        return vm.toBase64(abi.encodePacked(randomNum));
    }
}
