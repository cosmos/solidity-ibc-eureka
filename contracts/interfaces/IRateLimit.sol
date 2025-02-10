// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

interface IRateLimit {
    /// @notice The role identifier for the rate limiter role
    function RATE_LIMITER_ROLE() external view returns (bytes32);

    /// @notice Sets the rate limit for a token
    /// @dev The caller must have the rate limiter role
    /// @param token The token address
    /// @param rateLimit The rate limit to set
    function setRateLimit(address token, uint256 rateLimit) external;

    /// @notice Grants the rate limiter role to an account
    /// @dev The caller must be authorized by the derived contract
    /// @param account The account to grant the role to
    function grantRateLimiterRole(address account) external;

    /// @notice Revokes the rate limiter role from an account
    /// @dev The caller must be authorized by the derived contract
    /// @param account The account to revoke the role from
    function revokeRateLimiterRole(address account) external;
}
