// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

import { IICS20TransferMsgs } from "../msgs/IICS20TransferMsgs.sol";
import { ISignatureTransfer } from "@uniswap/permit2/src/interfaces/ISignatureTransfer.sol";

interface IICS20Transfer {
    /// @notice The role identifier for the delegate sender role
    /// @dev This role is required to call `sendTransferWithSender`
    /// @return The role identifier
    function DELEGATE_SENDER_ROLE() external view returns (bytes32);

    /// @notice The role identifier for the token operator role
    /// @dev This role is required to grant and revoke rate limiter and metadata setter roles
    /// @return The role identifier
    function TOKEN_OPERATOR_ROLE() external view returns (bytes32);

    /// @notice The role identifier for the ERC20 customizer role
    /// @dev This role is required to call `setCustomERC20`
    /// @return The role identifier
    function ERC20_CUSTOMIZER_ROLE() external view returns (bytes32);

    /// @notice Checks if an account has the token operator role
    /// @param account The account to check
    /// @return True if the account has the token operator role
    function isTokenOperator(address account) external view returns (bool);

    /// @notice Send a transfer by constructing a message and calling IICS26Router.sendPacket
    /// @param msg_ The message for sending a transfer
    /// @return sequence The sequence number of the packet created
    function sendTransfer(IICS20TransferMsgs.SendTransferMsg calldata msg_) external returns (uint64 sequence);

    /// @notice Send a permit2 transfer by constructing a message and calling IICS26Router.sendPacket
    /// @param msg_ The message for sending a transfer
    /// @param permit The permit data
    /// @param signature The signature of the permit data
    /// @return sequence The sequence number of the packet created
    function sendTransferWithPermit2(
        IICS20TransferMsgs.SendTransferMsg calldata msg_,
        ISignatureTransfer.PermitTransferFrom calldata permit,
        bytes calldata signature
    )
        external
        returns (uint64 sequence);

    /// @notice Send a transfer by constructing a message and calling IICS26Router.sendPacket with the provided sender
    /// @dev This is a permissioned function requiring the `DELEGATE_SENDER_ROLE`
    /// @dev Useful for contracts that need to refund the tokens to a sender.
    /// @param msg_ The message for sending a transfer
    /// @param sender The sender of the transfer
    /// @return sequence The sequence number of the packet created
    function sendTransferWithSender(
        IICS20TransferMsgs.SendTransferMsg calldata msg_,
        address sender
    )
        external
        returns (uint64 sequence);

    /// @notice Inserts a custom ERC20 contract for a given IBC denom
    /// @dev This must be called prior to the first transfer of the token so that the is no existing entry for the
    /// denom.
    /// @dev This function requires the `ERC20_CUSTOMIZER_ROLE`
    /// @param denom The IBC denom
    /// @param token The address of the custom ERC20 contract
    function setCustomERC20(string calldata denom, address token) external;

    /// @notice Retrieve the escrow contract address
    /// @param clientId The client identifier
    /// @return The escrow contract address
    function getEscrow(string calldata clientId) external view returns (address);

    /// @notice Retrieve the ERC20 contract address for the given IBC denom
    /// @param denom The IBC denom
    /// @return The ERC20 contract address
    function ibcERC20Contract(string calldata denom) external view returns (address);

    /// @notice Retrieve the full IBC denom path for the given token address
    /// @param token The token address
    /// @return The full IBC denom path
    function ibcERC20Denom(address token) external view returns (string memory);

    /// @notice Retrieve the Escrow beacon contract address
    /// @return The Escrow beacon contract address
    function getEscrowBeacon() external view returns (address);

    /// @notice Retrieve the IBCERC20 beacon contract address
    /// @return The IBCERC20 beacon contract address
    function getIBCERC20Beacon() external view returns (address);

    /// @notice Retrieve the ICS26Router contract address
    /// @return The ICS26Router contract address
    function ics26() external view returns (address);

    /// @notice Retrieve the Permit2 contract address
    /// @return The Permit2 contract address
    function getPermit2() external view returns (address);

    /// @notice Initializes the contract instead of a constructor
    /// @dev Meant to be called only once from the proxy
    /// @param ics26Router The ICS26Router contract address
    /// @param escrowLogic The address of the Escrow logic contract
    /// @param ibcERC20Logic The address of the IBCERC20 logic contract
    /// @param permit2 The address of the permit2 contract
    function initialize(address ics26Router, address escrowLogic, address ibcERC20Logic, address permit2) external;

    /// @notice Upgrades the implementation of the escrow beacon contract
    /// @dev The caller must be the ICS26Router admin
    /// @param newEscrowLogic The address of the new escrow logic contract
    function upgradeEscrowTo(address newEscrowLogic) external;

    /// @notice Upgrades the implementation of the ibcERC20 beacon contract
    /// @dev The caller must be the ICS26Router admin
    /// @param newIbcERC20Logic The address of the new ibcERC20 logic contract
    function upgradeIBCERC20To(address newIbcERC20Logic) external;

    /// @notice Grants the delegate sender role to an account
    /// @dev The caller must be the ICS26Router admin
    /// @param account The account to grant the role to
    function grantDelegateSenderRole(address account) external;

    /// @notice Revokes the delegate sender role from an account
    /// @dev The caller must be the ICS26Router admin
    /// @param account The account to revoke the role from
    function revokeDelegateSenderRole(address account) external;

    /// @notice Grants the token operator role to an account
    /// @dev The caller must be the ICS26Router admin
    /// @param account The account to grant the role to
    function grantTokenOperatorRole(address account) external;

    /// @notice Revokes the token operator role from an account
    /// @dev The caller must be the ICS26Router admin
    /// @param account The account to revoke the role from
    function revokeTokenOperatorRole(address account) external;

    /// @notice Grants the ERC20 customizer role to an account
    /// @dev The caller must be the ICS26Router admin
    /// @param account The account to grant the role to
    function grantERC20CustomizerRole(address account) external;

    /// @notice Revokes the ERC20 customizer role from an account
    /// @dev The caller must be the ICS26Router admin
    /// @param account The account to revoke the role from
    function revokeERC20CustomizerRole(address account) external;

    // --------------------- Events --------------------- //

    /// @notice Emitted when an IBCERC20 contract is created
    /// @param contractAddress The address of the IBCERC20 contract
    /// @param fullDenomPath The full IBC denom path for this token
    event IBCERC20ContractCreated(address indexed contractAddress, string fullDenomPath);
}
