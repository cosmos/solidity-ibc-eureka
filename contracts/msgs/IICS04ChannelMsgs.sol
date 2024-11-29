// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

interface IICS04ChannelMsgs {
    /// @notice Channel information.
    /// @custom:spec
    /// https://github.com/cosmos/ibc/blob/67fe813f7e4ec603a7c5dec35bc654f3b012afda/spec/micro/README.md?plain=1#L91
    /// @param counterpartyId The counterparty channel identifier from the counterparty chain.
    /// @param merklePrefix The counterparty chain's merkle prefix.
    struct Channel {
        string counterpartyId;
        bytes[] merklePrefix;
    }
}
