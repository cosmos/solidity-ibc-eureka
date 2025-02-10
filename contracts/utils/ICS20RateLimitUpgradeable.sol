// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

import { AccessControlUpgradeable } from "@openzeppelin-upgradeable/access/AccessControlUpgradeable.sol";

abstract contract ICS20RateLimitUpgradeable is AccessControlUpgradeable {
    /// @notice Storage of the ICS20RateLimit contract
    /// @dev It's implemented on a custom ERC-7201 namespace to reduce the risk of storage collisions when using with upgradeable contracts.
    /// @param rateLimits Mapping of token addresses to their rate limits
    struct ICS20RateLimitStorage {
        mapping(address token => uint256) rateLimits;
    }

    /// @notice ERC-7201 slot for the ICS20RateLimit storage
    /// @dev keccak256(abi.encode(uint256(keccak256("ibc.storage.ICS20RateLimit")) - 1)) & ~bytes32(uint256(0xff))
    bytes32 private constant ICS20RATELIMIT_STORAGE_SLOT =
        0xc7cd134226e58c84bf05772acb0cd1a5f7ad8109284407e942f521929a147000;

    /// @notice Role identifier for the rate limiter role
    bytes32 private constant RATE_LIMITER_ROLE = keccak256("RATE_LIMITER_ROLE");

    /// @notice Grants the rate limiter role to an account
    /// @dev The caller must be authorized by the derived contract
    /// @param account The account to grant the role to
    function grantRateLimiterRole(address account) external {
        _authorizeSetRateLimiterRole(account);
        _grantRole(RATE_LIMITER_ROLE, account);
    }

    /// @notice Revokes the rate limiter role from an account
    /// @dev The caller must be authorized by the derived contract
    /// @param account The account to revoke the role from
    function revokeRateLimiterRole(address account) external {
        _authorizeSetRateLimiterRole(account);
        _revokeRole(RATE_LIMITER_ROLE, account);
    }

    /// @notice Authorizes the granting or revoking of the rate limiter role
    /// @param account The account to authorize
    function _authorizeSetRateLimiterRole(address account) internal virtual;

    /// @notice Returns the storage of the ICS20RateLimit contract
    function _getICS20RateLimitStorage() internal pure returns (ICS20RateLimitStorage storage $) {
        // solhint-disable-next-line no-inline-assembly
        assembly {
            $.slot := ICS20RATELIMIT_STORAGE_SLOT
        }
    }
}
