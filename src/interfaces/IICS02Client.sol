// SPDX-License-Identifier: UNLICENSED
pragma solidity >=0.8.25;

// @title ICS02 Light Client Router Interface
// @notice IICS02Client is an interface for the IBC Eureka client router
interface IICS02Client {
    // @notice Counterparty client information.
    // @custom:spec
    // https://github.com/cosmos/ibc/blob/67fe813f7e4ec603a7c5dec35bc654f3b012afda/spec/micro/README.md?plain=1#L91
    struct CounterpartyInfo {
        /// The client identifier of the counterparty chain.
        string clientId;
    }

    /// @notice Height of the counterparty chain
    struct Height {
        /// Previously known as "epoch"
        uint32 revision_number;
        /// The height of a block
        uint32 revision_height;
    }

    // NOTE: The merkle path prefix of the counterparty is omitted for now.

    // @notice Returns the counterparty client information given the client identifier.
    // @param clientId The client identifier
    // @return The counterparty client information
    function getCounterparty(string calldata clientId) external view returns (CounterpartyInfo memory);

    // @notice Returns the creator of the client given the client identifier.
    // @param clientId The client identifier
    // @return The address of the client creator
    function getCreator(string calldata clientId) external view returns (address);

    // @notice Returns the address of the client contract given the client identifier.
    // @param clientId The client identifier
    // @return The address of the client contract
    function getClient(string calldata clientId) external view returns (address);

    // @notice Adds a client to the client router.
    // @param clientType The client type, e.g., "07-tendermint".
    // @param client The address of the client contract
    // @return The client identifier
    function addClient(string calldata clientType, address client) external returns (string memory);

    // @notice Adds a counterparty to the client router.
    // @param clientId The client identifier
    // @param counterpartyInfo The counterparty client information
    function addCounterparty(string calldata clientId, CounterpartyInfo calldata counterpartyInfo) external;
}
