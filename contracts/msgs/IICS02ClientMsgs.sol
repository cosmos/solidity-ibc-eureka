// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

interface IICS02ClientMsgs {
    /// @notice Height of the counterparty chain
    /// @param revisionNumber The revision number of the counterparty chain
    /// @param revisionHeight The height of the counterparty chain
    struct Height {
        uint32 revisionNumber;
        uint32 revisionHeight;
    }
}
