// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

import { IICS27GMPMsgs } from "../msgs/IICS27GMPMsgs.sol";

interface IICS27GMP {
    /// @notice The address of the ICS26Router contract
    /// @return The address of the ICS26Router contract
    function ics26() external view returns (address);

    /// @notice Retrieve the Account beacon contract address
    /// @return The Escrow beacon contract address
    function getAccountBeacon() external view returns (address);

    /// @notice Send a GMP packet by calling IICS26Router.sendPacket
    /// @param msg_ The message for sending a GMP packet
    /// @return sequence The sequence number of the packet created
    function sendCall(IICS27GMPMsgs.SendCallMsg calldata msg_) external returns (uint64 sequence);

    /// @notice Initializes the contract instead of a constructor
    /// @dev Meant to be called only once from the proxy
    /// @param ics26_ The ICS26Router contract address
    /// @param accountLogic The address of the ICS27Account logic contract
    function initialize(address ics26_, address accountLogic) external;
}
