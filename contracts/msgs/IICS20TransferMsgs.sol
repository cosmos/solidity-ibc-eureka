// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

interface IICS20TransferMsgs {
    /// @notice Message for sending a transfer
    /// @param tokens The tokens to transfer
    /// @param receiver The receiver of the transfer on the counterparty chain
    /// @param sourceClient The source client identifier
    /// @param destPort The destination port on the counterparty chain
    /// @param timeoutTimestamp The absolute timeout timestamp in unix seconds
    /// @param memo Optional memo
    struct SendTransferMsg {
        ERC20Token[] tokens;
        string receiver;
        string sourceClient;
        string destPort;
        uint64 timeoutTimestamp;
        string memo;
        Forwarding forwarding;
    }

    // TODO: Document
    struct ERC20Token {
        address contractAddress;
        uint256 amount;
    }

    /// @notice Forwarding defines a list of port ID, channel ID pairs determining the path
    /// through which a packet must be forwarded
    /// @param hops Optional intermediate path through which packet will be forwarded
    struct Forwarding {
        Hop[] hops;
    }

    /// @notice Hop defines a port ID, channel ID pair specifying where tokens must be forwarded
    /// next in a multihop transfer, or the trace of an existing token.
    /// @param portId The port ID
    /// @param channelId The channel ID
    struct Hop {
        string portId;
        string clientId;
    }
}
