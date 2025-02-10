// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

interface IRateLimitErrors {
    /// @notice Rate limit exceeded
    /// @param usage The usage of the token
    /// @param rateLimit The rate limit of the token
    error ICS20RateLimitExceeded(uint256 usage, uint256 rateLimit);
}
