// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

interface IICS24HostErrors {
    /// @notice Packet commitment not found
    /// @param path commitment path
    error IBCPacketCommitmentNotFound(bytes path);

    /// @notice Packet commitment already exists
    /// @param path commitment path
    error IBCPacketCommitmentAlreadyExists(bytes path);

    /// @notice Packet receipt already exists
    /// @param path commitment path
    error IBCPacketReceiptAlreadyExists(bytes path);

    /// @notice Packet acknowledgement already exists
    /// @param path commitment path
    error IBCPacketAcknowledgementAlreadyExists(bytes path);

    /// @notice Merkle prefix is invalid
    /// @param prefix The invalid prefix
    error InvalidMerklePrefix(bytes[] prefix);
}
