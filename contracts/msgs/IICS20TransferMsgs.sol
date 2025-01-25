// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

import { ICS20Lib } from "../utils/ICS20Lib.sol";

interface IICS20TransferMsgs {
    /// @notice Message for sending a transfer
    /// @param tokens The tokens to transfer
    /// @param receiver The receiver of the transfer on the counterparty chain
    /// @param sourceClient The source client identifier
    /// @param destPort The destination port on the counterparty chain
    /// @param timeoutTimestamp The absolute timeout timestamp in unix seconds
    /// @param memo Optional memo
    struct SendTransferMsg {
        ICS20Lib.Token[] tokens;
        string receiver;
        string sourceClient;
        string destPort;
        uint64 timeoutTimestamp;
        string memo;
        Forwarding forwarding;
    }

    /// @notice Forwarding defines a list of port ID, channel ID pairs determining the path
    /// through which a packet must be forwarded, and an unwind boolean indicating if
    /// the coin should be unwinded to its native chain before forwarding.
    /// @param hops Optional intermediate path through which packet will be forwarded
    struct Forwarding {
        // TODO: Do we want unwinding in the solidity implementation?
        // bool unwind;
        ICS20Lib.Hop[] hops;
    }
}
