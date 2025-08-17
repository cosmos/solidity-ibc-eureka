// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

import { ILightClient } from "../../interfaces/ILightClient.sol";
import { ILightClientMsgs } from "../../msgs/ILightClientMsgs.sol";
import { IICS02ClientMsgs } from "../../msgs/IICS02ClientMsgs.sol";
import { IAttestorMsgs } from "./IAttestorMsgs.sol";
import { IAttestorErrors } from "./IAttestorErrors.sol";

import { AccessControl } from "@openzeppelin-contracts/access/AccessControl.sol";
import { TransientSlot } from "@openzeppelin-contracts/utils/TransientSlot.sol";

/// @title Ethereum Attestor Light Client
/// @notice Implements ILightClient using EVM attestor addresses and ECDSA signatures over attested packets
contract AttestorLightClient is ILightClient, IAttestorErrors, AccessControl {
    using TransientSlot for *;

    // Anyone-or-role gating as in SP1 client
    bytes32 public constant PROOF_SUBMITTER_ROLE = keccak256("PROOF_SUBMITTER_ROLE");

    IAttestorMsgs.ClientState public clientState;

    // height => timestamp (seconds)
    mapping(uint64 => uint64) private _timestampAtHeight;
    // Sorted by insertion semantics; linear neighbor scan is acceptable initially
    uint64[] private _heights;
    // Fast membership for attestors
    mapping(address => bool) private _isAttestor;

    /// @param clientStateEncoded abi.encode(IAttestorMsgs.ClientState)
    /// @param initialTimestamp seconds timestamp for latestHeight
    /// @param roleManager address(0) to allow anyone, else manager for roles
    constructor(bytes memory clientStateEncoded, uint64 initialTimestamp, address roleManager) {
        clientState = abi.decode(clientStateEncoded, (IAttestorMsgs.ClientState));

        // seed attestor set
        for (uint256 i = 0; i < clientState.attestors.length; i++) {
            _isAttestor[clientState.attestors[i]] = true;
        }

        // seed consensus timestamp storage
        _timestampAtHeight[clientState.latestHeight.revisionHeight] = initialTimestamp;
        _heights.push(clientState.latestHeight.revisionHeight);

        if (roleManager == address(0)) {
            _grantRole(PROOF_SUBMITTER_ROLE, address(0));
        } else {
            _grantRole(DEFAULT_ADMIN_ROLE, roleManager);
            _grantRole(PROOF_SUBMITTER_ROLE, roleManager);
        }
    }

    // ILightClient
    function updateClient(bytes calldata updateMsg)
        external
        notFrozen
        onlyProofSubmitter
        returns (ILightClientMsgs.UpdateResult)
    {
        IAttestorMsgs.MsgUpdateClient memory msg_ = abi.decode(updateMsg, (IAttestorMsgs.MsgUpdateClient));

        _verifySignatures(msg_.packets, msg_.signatures);

        // Timestamp rules mirroring CosmWasm
        uint64 existingTs = _timestampAtHeight[msg_.newHeight];
        if (existingTs != 0) {
            // Exact match required
            if (existingTs != msg_.timestamp) revert TimestampMismatch();
            return ILightClientMsgs.UpdateResult.NoOp;
        }

        // Monotonicity vs neighbors
        (bool hasPrev, uint64 prevH, uint64 prevTs, bool hasNext, uint64 nextH, uint64 nextTs) = _findNeighbors(
            msg_.newHeight
        );
        if (hasPrev && hasNext) {
            if (!(msg_.timestamp > prevTs && msg_.timestamp < nextTs)) revert NotMonotonic();
        } else if (hasPrev && !hasNext) {
            if (msg_.timestamp < prevTs) revert NotMonotonic();
        } else if (!hasPrev && hasNext) {
            if (msg_.timestamp > nextTs) revert NotMonotonic();
        }

        // Persist new consensus timestamp
        _timestampAtHeight[msg_.newHeight] = msg_.timestamp;
        _insertHeightSorted(msg_.newHeight);
        if (msg_.newHeight > clientState.latestHeight.revisionHeight) {
            clientState.latestHeight.revisionHeight = msg_.newHeight;
        }

        // Cache packets for empty-proof membership in the same tx
        _cachePackets(msg_.newHeight, msg_.packets, msg_.timestamp);

        return ILightClientMsgs.UpdateResult.Update;
    }

    function verifyMembership(ILightClientMsgs.MsgVerifyMembership calldata msg_)
        external
        notFrozen
        onlyProofSubmitter
        returns (uint256)
    {
        if (msg_.value.length == 0) revert EmptyValue();
        uint64 ts = _timestampAtHeight[msg_.proofHeight.revisionHeight];
        if (ts == 0) revert ConsensusStateNotFound();

        if (msg_.proof.length == 0) {
            // cached path
            bytes32 key = keccak256(abi.encode(msg_.proofHeight.revisionHeight, keccak256(msg_.value)));
            uint256 cachedTs = key.asUint256().tload();
            if (cachedTs == 0) revert ConsensusStateNotFound();
            return ts;
        }

        IAttestorMsgs.MembershipProof memory proof = abi.decode(msg_.proof, (IAttestorMsgs.MembershipProof));
        _verifySignatures(proof.packets, proof.signatures);

        // membership by byte equality
        bool found;
        for (uint256 i = 0; i < proof.packets.length; i++) {
            if (proof.packets[i].length == msg_.value.length && keccak256(proof.packets[i]) == keccak256(msg_.value)) {
                found = true;
                break;
            }
        }
        if (!found) revert InvalidSignature(); // reuse error to avoid new error; indicates invalid proof wrt value

        return ts;
    }

    function verifyNonMembership(ILightClientMsgs.MsgVerifyNonMembership calldata)
        external
        view
        notFrozen
        onlyProofSubmitter
        returns (uint256)
    {
        revert FeatureNotSupported();
    }

    function misbehaviour(bytes calldata) external notFrozen onlyProofSubmitter {
        clientState.isFrozen = true;
    }

    function upgradeClient(bytes calldata) external view notFrozen onlyProofSubmitter {
        revert FeatureNotSupported();
    }

    function getClientState() external view returns (bytes memory) {
        return abi.encode(clientState);
    }

    // Helpers
    function _verifySignatures(bytes[] memory packets, bytes[] memory signatures) private view {
        // digest = sha256(abi.encode(packets))
        bytes32 digest = sha256(abi.encode(packets));

        uint256 sigs = signatures.length;
        if (sigs < clientState.minRequiredSigs) revert InsufficientSignatures(sigs, clientState.minRequiredSigs);

        // track uniqueness
        // Use a memory mapping via hashing (small sets expected)
        address[] memory seen = new address[](sigs);
        for (uint256 i = 0; i < sigs; i++) {
            address signer = _recover(digest, signatures[i]);
            if (!_isAttestor[signer]) revert UnknownSigner(signer);
            // check duplicate
            for (uint256 j = 0; j < i; j++) {
                if (seen[j] == signer) revert DuplicateSigner(signer);
            }
            seen[i] = signer;
        }
    }

    function _recover(bytes32 digest, bytes memory sig) private pure returns (address) {
        if (sig.length != 65) revert InvalidSignature();
        bytes32 r;
        bytes32 s;
        uint8 v;
        // solhint-disable-next-line no-inline-assembly
        assembly {
            r := mload(add(sig, 0x20))
            s := mload(add(sig, 0x40))
            v := byte(0, mload(add(sig, 0x60)))
        }
        if (v < 27) v += 27;
        address recovered = ecrecover(digest, v, r, s);
        if (recovered == address(0)) revert InvalidSignature();
        return recovered;
    }

    function _cachePackets(uint64 height, bytes[] memory packets, uint64 /*timestamp*/ ) private {
        for (uint256 i = 0; i < packets.length; i++) {
            bytes32 key = keccak256(abi.encode(height, keccak256(packets[i])));
            key.asUint256().tstore(1);
        }
    }

    function _insertHeightSorted(uint64 h) private {
        // naive insert keeping order; append then bubble once
        // acceptable for small sets
        if (_heights.length == 0 || _heights[_heights.length - 1] < h) {
            _heights.push(h);
            return;
        }
        // find position to insert; linear scan from end
        uint256 i = _heights.length;
        _heights.push(h);
        while (i > 0 && _heights[i - 1] > h) {
            _heights[i] = _heights[i - 1];
            unchecked {
                i--;
            }
        }
        _heights[i] = h;
    }

    function _findNeighbors(uint64 h)
        private
        view
        returns (bool hasPrev, uint64 prevH, uint64 prevTs, bool hasNext, uint64 nextH, uint64 nextTs)
    {
        uint256 n = _heights.length;
        if (n == 0) return (false, 0, 0, false, 0, 0);
        // find first index where _heights[idx] >= h
        uint256 idx = 0;
        while (idx < n && _heights[idx] < h) {
            unchecked {
                idx++;
            }
        }
        if (idx > 0) {
            hasPrev = true;
            prevH = _heights[idx - 1];
            prevTs = _timestampAtHeight[prevH];
        }
        if (idx < n) {
            // idx points to first >= h
            if (_heights[idx] > h) {
                hasNext = true;
                nextH = _heights[idx];
                nextTs = _timestampAtHeight[nextH];
            } else if (idx + 1 < n) {
                hasNext = true;
                nextH = _heights[idx + 1];
                nextTs = _timestampAtHeight[nextH];
            }
        }
    }

    modifier notFrozen() {
        if (clientState.isFrozen) revert ClientFrozen();
        _;
    }

    modifier onlyProofSubmitter() {
        if (!hasRole(PROOF_SUBMITTER_ROLE, address(0))) {
            _checkRole(PROOF_SUBMITTER_ROLE);
        }
        _;
    }
}


