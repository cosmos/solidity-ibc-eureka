// SPDX-License-Identifier: UNLICENSED
pragma solidity >=0.8.25;

// @title Light Client Interface
// @notice ILightClient is the light client interface for the IBC Eureka light client
interface ILightClient {
    // @notice Initializes the light client with a trusted client state and consensus state
    // @dev Should be used in the constructor of the light client contract
    struct MsgInitialize {
        /// Initial client state
        bytes client_state;
        /// Initial consensus state
        bytes consensus_state;
    }
}
