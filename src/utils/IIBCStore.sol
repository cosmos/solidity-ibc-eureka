// SPDX-License-Identifier: UNLICENSED
pragma solidity >=0.8.25;

// @title IBC Store Interface
interface IIBCStore {
    // @notice Gets the commitment for a given path.
    function getCommitment(bytes32 hashedPath) external view returns (bytes32);

    // @notice Get the next sequence to send for a given port and channel pair.
    function getNextSequenceSend(string calldata portId, string calldata channelId) external view returns (uint32);
}
