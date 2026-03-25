// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

// solhint-disable custom-errors,gas-custom-errors

import {Script, console} from "forge-std/Script.sol";
import {IICS02ClientMsgs} from "../contracts/msgs/IICS02ClientMsgs.sol";
import {ICS26Router} from "../contracts/ICS26Router.sol";

/// @title WireTestnetClients
/// @notice Foundry script to wire attestation light clients on Ethereum Sepolia and Base Sepolia.
/// @dev Usage:
///   Ethereum Sepolia:
///     PRIVATE_KEY=0x... forge script scripts/WireTestnetClients.s.sol --sig "wireEthSepolia()" --rpc-url <ETH_SEPOLIA_RPC> --broadcast
///   Base Sepolia:
///     PRIVATE_KEY=0x... forge script scripts/WireTestnetClients.s.sol --sig "wireBaseSepolia()" --rpc-url <BASE_SEPOLIA_RPC> --broadcast
contract WireTestnetClients is Script {
    // ── Ethereum Sepolia addresses ──────────────────────────────────────
    address constant ETH_SEPOLIA_ICS26_ROUTER =
        0xe20BccD900Fa1B48f46F5a483d9De063b07eDFCC;
    address constant ETH_SEPOLIA_ATTESTATION_LC =
        0xc0Fd41975632060b5b24d26d2561692B683A4A65;
    string constant ETH_SEPOLIA_COUNTERPARTY_CLIENT_ID = "attestations-0";

    // ── Base Sepolia addresses ──────────────────────────────────────────
    address constant BASE_SEPOLIA_ICS26_ROUTER =
        0x04357D2434523a31B6f89E0414053AeafCD10dee;
    address constant BASE_SEPOLIA_ATTESTATION_LC =
        0x1C57c821e1a02D4B5F97D27de50f153530f95785;
    string constant BASE_SEPOLIA_COUNTERPARTY_CLIENT_ID = "attestations-1";

    // ── Shared constants ────────────────────────────────────────────────
    string constant CUSTOM_CLIENT_ID = "wfchain-1";

    /// @notice Build the merkle prefix for a Cosmos chain counterparty verified via attestor LC.
    /// @dev Attestor LCs use keccak256(commitment_path) directly, so prefix is [""].
    ///      SP1 LCs use ICS23 merkle proofs against IAVL, so prefix would be ["ibc", ""].
    function _cosmosMerklePrefix() internal pure returns (bytes[] memory) {
        bytes[] memory prefix = new bytes[](1);
        prefix[0] = bytes("");
        return prefix;
    }

    /// @notice Wire the attestation light client on Ethereum Sepolia.
    function wireEthSepolia() public {
        uint256 deployerPrivateKey = vm.envUint("PRIVATE_KEY");
        vm.startBroadcast(deployerPrivateKey);

        IICS02ClientMsgs.CounterpartyInfo
            memory counterpartyInfo = IICS02ClientMsgs.CounterpartyInfo({
                clientId: ETH_SEPOLIA_COUNTERPARTY_CLIENT_ID,
                merklePrefix: _cosmosMerklePrefix()
            });

        string memory clientId = ICS26Router(ETH_SEPOLIA_ICS26_ROUTER)
            .addClient(
                CUSTOM_CLIENT_ID,
                counterpartyInfo,
                ETH_SEPOLIA_ATTESTATION_LC
            );

        vm.stopBroadcast();

        console.log("Ethereum Sepolia - client added with ID:", clientId);
    }

    /// @notice Wire the attestation light client on Base Sepolia.
    function wireBaseSepolia() public {
        uint256 deployerPrivateKey = vm.envUint("PRIVATE_KEY");
        vm.startBroadcast(deployerPrivateKey);

        IICS02ClientMsgs.CounterpartyInfo
            memory counterpartyInfo = IICS02ClientMsgs.CounterpartyInfo({
                clientId: BASE_SEPOLIA_COUNTERPARTY_CLIENT_ID,
                merklePrefix: _cosmosMerklePrefix()
            });

        string memory clientId = ICS26Router(BASE_SEPOLIA_ICS26_ROUTER)
            .addClient(
                CUSTOM_CLIENT_ID,
                counterpartyInfo,
                BASE_SEPOLIA_ATTESTATION_LC
            );

        vm.stopBroadcast();

        console.log("Base Sepolia - client added with ID:", clientId);
    }
}
