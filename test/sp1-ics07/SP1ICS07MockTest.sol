// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

import { Test } from "forge-std/Test.sol";

import { IICS02ClientMsgs } from "../../contracts/msgs/IICS02ClientMsgs.sol";
import { IICS07TendermintMsgs } from "../../contracts/light-clients/msgs/IICS07TendermintMsgs.sol";

import { SP1ICS07Tendermint } from "../../contracts/light-clients/SP1ICS07Tendermint.sol";

import { SP1MockVerifier } from "@sp1-contracts/SP1MockVerifier.sol";

abstract contract SP1ICS07MockTest is Test {
    string public constant MOCK_CHAIN_ID = "mock-chain";
    bytes32 public constant MOCK_VKEY = keccak256("MOCK_VKEY");
    bytes32 public constant MOCK_ROOT = keccak256("MOCK_ROOT");
    bytes32 public constant MOCK_VAL_HASH = keccak256("MOCK_VAL_HASH");
    
    address roleManager = makeAddr("roleManager");

    SP1ICS07Tendermint public ics07Tendermint;

    function setUp() public {
        bytes32 firstConsensusStateHash = keccak256(abi.encode(newMockConsensusState(1)));

        ics07Tendermint = new SP1ICS07Tendermint(
            MOCK_VKEY,
            MOCK_VKEY,
            MOCK_VKEY,
            MOCK_VKEY,
            address(new SP1MockVerifier()),
            abi.encode(mockClientState()),
            firstConsensusStateHash,
            roleManager
        );
    }

    function mockClientState() public pure returns (IICS07TendermintMsgs.ClientState memory) {
        return IICS07TendermintMsgs.ClientState({
            chainId: MOCK_CHAIN_ID,
            trustLevel: IICS07TendermintMsgs.TrustThreshold({ numerator: 1, denominator: 3 }),
            latestHeight: IICS02ClientMsgs.Height({ revisionNumber: 0, revisionHeight: 1 }),
            trustingPeriod: 1 weeks,
            unbondingPeriod: 2 weeks,
            isFrozen: false,
            zkAlgorithm: IICS07TendermintMsgs.SupportedZkAlgorithm.Groth16
        });
    }

    /// @notice Create a new mock consensus state
    /// @param timestamp The timestamp of the consensus state in unix nanoseconds
    /// @return The new consensus state
    function newMockConsensusState(uint128 timestamp) public pure returns (IICS07TendermintMsgs.ConsensusState memory) {
        return IICS07TendermintMsgs.ConsensusState({
            timestamp: timestamp,
            root: MOCK_ROOT,
            nextValidatorsHash: MOCK_VAL_HASH
        });
    }
}
