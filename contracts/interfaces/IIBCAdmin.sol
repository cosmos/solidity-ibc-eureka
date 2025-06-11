// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

/// @title IIBCAdmin
/// @notice Interface for tracking the timelocked admin and governance admin for IBC contracts.
interface IIBCAdmin {
    /// @notice Returns the timelocked admin address
    /// @return The timelocked admin address
    function timelockedAdmin() external view returns (address);
    /// @notice Returns the governance admin address
    /// @return The governance admin address, 0 if not set
    function govAdmin() external view returns (address);
    /// @notice Returns the access manager address
    /// @return The access manager address, 0 if not set
    function accessManager() external view returns (address);
    /// @notice Sets the timelocked admin address
    /// @dev Either admin can set the timelocked admin address.
    /// @param newTimelockedAdmin The new timelocked admin address
    function setTimelockedAdmin(address newTimelockedAdmin) external;
    /// @notice Sets the governance admin address
    /// @dev Either admin can set the governance admin address.
    /// @dev Since timelocked admin is timelocked, this operation can be stopped by the govAdmin.
    /// @param newGovAdmin The new governance admin address
    function setGovAdmin(address newGovAdmin) external;
    /// @notice Sets the access manager address
    /// @param newAccessManager The new access manager address
    /// @dev Either admin can set the access manager address.
    /// @dev The access manager is used to control access to IBC contracts.
    function setAccessManager(address newAccessManager) external;
    /// @notice This funtion initializes the timelockedAdmin, and the accessManager
    /// @param timelockedAdmin_ The timelocked admin address, assumed to be timelocked
    /// @param accessManager_ The address of the AccessManager contract, which this contract is an admin of
    function initialize(address timelockedAdmin_, address accessManager_) external;
}
