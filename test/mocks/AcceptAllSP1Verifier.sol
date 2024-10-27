// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.28;

// solhint-disable no-empty-blocks

import { ISP1Verifier } from "@sp1-contracts/ISP1Verifier.sol";

/// @dev This SP1 verifier accepts all proofs, for testing purposes.
/// @dev It is required due to the issues we are running into with '@sp1-contracts/SP1MockVerifier.sol'.
/// @dev This contract can be removed once the issues are resolved.
contract AcceptAllSP1Verifier is ISP1Verifier {
    function verifyProof(bytes32, bytes calldata, bytes calldata) external view override {
        // Accept all proofs
    }
}
