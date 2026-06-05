// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

import { ECDSA } from "@openzeppelin-contracts/utils/cryptography/ECDSA.sol";

import { ICometBFTClientErrors } from "../errors/ICometBFTClientErrors.sol";

/// @title CometBFT secp256k1eth ECDSA
/// @notice Recovers go-ethereum compatible [R || S || V] signatures with V in {0,1}.
library CometBFTECDSA {
    uint256 private constant SIGNATURE_LENGTH = 65;
    uint256 private constant SECP256K1_HALF_ORDER = 0x7FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF5D576E7357A4501DDFE92F46681B20A0;

    function recover(bytes32 digest, bytes memory signature) internal pure returns (address) {
        if (signature.length != SIGNATURE_LENGTH) {
            revert ICometBFTClientErrors.InvalidSignatureLength(signature.length);
        }

        bytes32 r;
        bytes32 s;
        uint8 v;
        assembly ("memory-safe") {
            r := mload(add(signature, 0x20))
            s := mload(add(signature, 0x40))
            v := byte(0, mload(add(signature, 0x60)))
        }

        if (v > 1) {
            revert ICometBFTClientErrors.InvalidSignatureV(v);
        }
        if (uint256(s) > SECP256K1_HALF_ORDER) {
            revert ICometBFTClientErrors.InvalidSignatureS(s);
        }

        (address recovered, ECDSA.RecoverError err,) = ECDSA.tryRecover(digest, v + 27, r, s);
        if (err != ECDSA.RecoverError.NoError || recovered == address(0)) {
            revert ICometBFTClientErrors.SignatureInvalid(signature);
        }

        return recovered;
    }
}
