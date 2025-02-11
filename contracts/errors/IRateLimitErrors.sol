// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

interface IRateLimitErrors {
    /// @notice Rate limit exceeded
    /// @param rateLimit The rate limit of the token
    /// @param usage The usage of the token
    error RateLimitExceeded(uint256 rateLimit, uint256 usage);
}
