// SPDX-License-Identifier: UNLICENSED
pragma solidity >=0.8.25;

interface IICS02ClientMsgs {
    /// @notice Counterparty client information.
    /// @custom:spec
    /// https://github.com/cosmos/ibc/blob/67fe813f7e4ec603a7c5dec35bc654f3b012afda/spec/micro/README.md?plain=1#L91
    struct CounterpartyInfo {
        /// The client identifier of the counterparty chain.
        string clientId;
    }

    /// @notice Height of the counterparty chain
    struct Height {
        /// Previously known as "epoch"
        uint32 revisionNumber;
        /// The height of a block
        uint32 revisionHeight;
    }

    // NOTE: The merkle path prefix of the counterparty is omitted for now.
}
