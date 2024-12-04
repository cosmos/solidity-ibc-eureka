// SPDX-License-Identifier: MIT
// Based on OpenZeppelin Contracts (utils/Multicall.sol)

pragma solidity ^0.8.28;

import { Context } from "@openzeppelin/utils/Context.sol";

/**
 * @dev Provides a function to batch together multiple calls in a single external call without reverting on failure.
 *
 * Consider any assumption about calldata validation performed by the sender may be violated if it's not especially
 * careful about sending transactions invoking {multipass}. For example, a relay address that filters function
 * selectors won't filter calls nested within a {multipass} operation.
 *
 * NOTE: This is based on
 * https://github.com/OpenZeppelin/openzeppelin-contracts/blob/master/contracts/utils/Multicall.sol
 */
abstract contract Multipass is Context {
    /**
     * @dev Receives and executes a batch of function calls on this contract. Doesn't revert if any of the calls fail.
     * @custom:oz-upgrades-unsafe-allow-reachable delegatecall
     */
    function multipass(bytes[] calldata data) external virtual returns (bytes[] memory results) {
        bytes memory context =
            msg.sender == _msgSender() ? new bytes(0) : msg.data[msg.data.length - _contextSuffixLength():];

        results = new bytes[](data.length);
        for (uint256 i = 0; i < data.length; i++) {
            (, results[i]) = address(this).delegatecall(bytes.concat(data[i], context));
        }
        return results;
    }
}
