// SPDX-License-Identifier: UNLICENSED
pragma solidity >=0.8.25;

// @title ICS26 Router Interface
// @notice IICS26Router is an interface for the IBC Eureka router
interface IICS26Router {
    // @notice Returns the address of the admin
    // @dev This should be satisfied by openzeppelin's Ownable contract
    // @dev address(0) is returned if the router has no admin
    // @return The address of the admin
    function owner() external view returns (address);

    // @notice Returns the address of the IBC application given the port identifier
    // @param portId The port identifier
    // @return The address of the IBC application contract
    function getIBCApp(string calldata portId) external view returns (address);

    // @notice Adds an IBC application to the router
    // @dev Only the admin can submit non-empty port identifiers. The default port identifier
    // is the address of the IBC application contract.
    // @param portId The port identifier, only admin can submit non-empty port identifiers.
    // @param app The address of the IBC application contract
    function addIBCApp(string calldata portId, address app) external;
}
