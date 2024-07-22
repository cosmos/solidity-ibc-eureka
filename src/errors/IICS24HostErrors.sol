// SPDX-License-Identifier: UNLICENSED
pragma solidity >=0.8.25;

// @title IBC Store Interface
interface IICS24HostErrors {
    /// @param path commitment path
    error IBCPacketCommitmentNotFound(bytes path);

    /// @param path commitment path
    error IBCPacketCommitmentAlreadyExists(bytes path);

    /// @param path commitment path
    error IBCPacketReceiptAlreadyExists(bytes path);

    /// @param path commitment path
    error IBCPacketAcknowledgementAlreadyExists(bytes path);
}
