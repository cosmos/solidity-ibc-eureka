// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

interface IICS20TransferMsgsOld {
    /// @notice Message for sending a transfer
    /// @param denom The denomination of the token, usually the contract address
    /// @param amount The amount of tokens to transfer
    /// @param receiver The receiver of the transfer on the counterparty chain
    /// @param sourceChannel The source channel (client identifier)
    /// @param destPort The destination port on the counterparty chain
    /// @param timeoutTimestamp The absolute timeout timestamp in unix seconds
    /// @param memo Optional memo
    struct SendTransferMsg {
        string denom;
        uint256 amount;
        string receiver;
        string sourceChannel;
        string destPort;
        uint64 timeoutTimestamp;
        string memo;
    }
}
