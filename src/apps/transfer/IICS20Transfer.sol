// SPDX-License-Identifier: UNLICENSED
pragma solidity >=0.8.25;

interface IICS20Transfer {
    /// @notice Called when a packet is handled in onSendPacket and a transfer has been initiated
    /// @param sender The sender of the packet
    /// @param receiver The receiver of the packet on the counterparty chain
    /// @param tokenContract The ERC20 token contract address
    /// @param amount The amount of tokens transferred
    /// @param memo The memo field of the packet
    event ICS20Transfer(address sender, string receiver, address tokenContract, uint256 amount, string memo);

    // TODO: If we need error and/or success result (resp.Result in the go code), parsing the acknowledgement is needed
    /// @notice Called after handling acknowledgement in onAcknowledgementPacket
    /// @param sender The sender of the packet
    /// @param receiver The receiver of the packet on the counterparty chain
    /// @param tokenContract The ERC20 token contract address
    /// @param amount The amount of tokens transferred
    /// @param memo The memo field of the packet
    /// @param acknowledgement The acknowledgement data
    /// @param success Whether the acknowledgement received was a success or error
    event ICS20Acknowledgement(address sender, string receiver, address tokenContract, uint256 amount, string memo, bytes acknowledgement, bool success);

    /// @notice Called after handling a timeout in onTimeoutPacket
    /// @param sender The sender of the packet
    /// @param tokenContract The ERC20 token contract address
    /// @param memo The memo field of the packet
    event ICS20Timeout(address sender, address tokenContract, string memo);

    struct UnwrappedFungibleTokenPacketData {
        address erc20ContractAddress;
        uint256 amount;
        address sender;
        string receiver;
        string memo;
    }
}