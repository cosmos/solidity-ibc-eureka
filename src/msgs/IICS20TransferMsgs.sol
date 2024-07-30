// SPDX-License-Identifier: MIT
pragma solidity >=0.8.25;

interface IICS20TransferMsgs {
    /// @notice Message for sending a transfer
    struct SendTransferMsg {
        /// This is expected to be contract address of the token contract
        string denom;
        /// The amount of tokens to transfer
        uint256 amount;
        /// The receiver of the transfer on the counterparty chain
        string receiver;
        /// The source channel (client identifier)
        string sourceChannel;
        /// The destination port on the counterparty chain
        string destPort;
        /// The absolute timeout timestamp
        uint32 timeoutTimestamp;
        /// Optional memo
        string memo;
    }
}
