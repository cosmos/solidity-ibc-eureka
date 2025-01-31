// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

interface IICS20TransferMsgs {
    /// @notice Message for sending a transfer
    /// @param tokens The tokens to transfer
    /// @param receiver The receiver of the transfer on the counterparty chain
    /// @param sourceClient The source client identifier
    /// @param destPort The destination port on the counterparty chain
    /// @param timeoutTimestamp The absolute timeout timestamp in unix seconds
    /// @param memo Optional memo. If forwarding is used, this memo will be consumed by the final destination chain
    /// @param forwarding Optional forwarding information
    struct SendTransferMsg {
        ERC20Token[] tokens;
        string receiver;
        string sourceClient;
        string destPort;
        uint64 timeoutTimestamp;
        string memo;
        Forwarding forwarding;
    }

    /// @notice ERC20Token holds the contract address and amount of an ERC20 token to be transferred.
    /// @param contractAddress The contract address of the ERC20 token
    /// @param amount The amount of the ERC20 token to be transferred
    struct ERC20Token {
        address contractAddress;
        uint256 amount;
    }

    /// @notice Hop defines a port ID, client ID pair specifying either
    /// where tokens must be forwarded next in a multihop transfer,
    /// or the trace of an existing token.
    /// @param portId The port ID
    /// @param clientID The client ID
    struct Hop {
        string portId;
        string clientId;
    }

    /// @notice Forwarding defines a list of port ID, client ID pairs determining the path
    /// through which a packet must be forwarded
    /// @param hops intermediate path through which packet will be forwarded
    struct Forwarding {
        Hop[] hops;
    }

    // ICS20Transfer payload data structures:

    /// @notice FungibleTokenPacketDataV2 is the payload for a fungible token transfer packet in ics20-2.
    /// @dev See FungibleTokenPacketDataV2V2 spec:
    /// https://github.com/cosmos/ibc/tree/master/spec/app/ics-020-fungible-token-transfer#data-structures
    /// @param tokens The tokens to be transferred
    /// @param sender The sender of the token
    /// @param receiver The receiver of the token
    /// @param memo Optional memo
    /// @param forwarding Optional forwarding information
    struct FungibleTokenPacketDataV2 {
        Token[] tokens;
        string sender;
        string receiver;
        string memo;
        ForwardingPacketData forwarding;
    }

    /// @notice ForwardingPacketData defines a list of port ID, client ID pairs determining the path
    /// through which a packet must be forwarded, and the destination memo string to be used in the
    /// final destination of the tokens.
    /// @param destination_memo Optional memo consumed by final destination chain
    /// @param hops Optional intermediate path through which packet will be forwarded.
    struct ForwardingPacketData {
        string destinationMemo;
        Hop[] hops;
    }

    /// @notice Token holds the denomination and amount of a token to be transferred.
    /// @param denom The token denomination
    /// @param amount The token amount
    struct Token {
        Denom denom;
        uint256 amount;
    }

    /// @notice Denom holds the base denom of a Token and a trace of the chains it was sent through.
    /// @param base The base token denomination
    /// @param trace The trace of the token
    struct Denom {
        string base;
        Hop[] trace;
    }
}
