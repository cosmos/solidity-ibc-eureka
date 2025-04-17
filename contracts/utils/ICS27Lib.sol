// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.28;

library ICS27Lib {
    /// @notice ICS27_VERSION is the version string for ICS27 packet data.
    string internal constant ICS27_VERSION = "ics27-2";

    /// @notice ICS27_ENCODING is the encoding string for ICS27 packet data.
    string internal constant ICS27_ENCODING = "application/x-solidity-abi";

    /// @notice DEFAULT_PORT_ID is the default port id for ICS27.
    string internal constant DEFAULT_PORT_ID = "gmpport";

    /// @notice KECCAK256_ICS27_VERSION is the keccak256 hash of the ICS27_VERSION.
    bytes32 internal constant KECCAK256_ICS27_VERSION = keccak256(bytes(ICS27_VERSION));

    /// @notice KECCAK256_ICS27_ENCODING is the keccak256 hash of the ICS27_ENCODING.
    bytes32 internal constant KECCAK256_ICS27_ENCODING = keccak256(bytes(ICS27_ENCODING));

}
