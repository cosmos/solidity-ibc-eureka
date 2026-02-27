// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

// solhint-disable custom-errors,gas-custom-errors

import { stdJson } from "forge-std/StdJson.sol";
import { Script } from "forge-std/Script.sol";
import { Strings } from "@openzeppelin-contracts/utils/Strings.sol";

import { SolanaIFTSendCallConstructor } from "../contracts/utils/SolanaIFTSendCallConstructor.sol";

/// @notice Deploys SolanaIFTSendCallConstructor with 6 static Solana PDAs from environment variables.
/// @dev Called separately from the main E2E deploy script because the PDAs depend on the Solana IFT mint
///      (created during the test) and the EVM IFT contract address (created by the main deploy script).
///      Required env vars: SOL_IFT_APP_STATE, SOL_IFT_APP_MINT_STATE, SOL_IFT_BRIDGE,
///      SOL_IFT_MINT, SOL_IFT_MINT_AUTHORITY, SOL_IFT_GMP_ACCOUNT
contract DeploySolanaIFTConstructor is Script {
    using stdJson for string;

    function run() public returns (string memory) {
        bytes32 appState = vm.envBytes32("SOL_IFT_APP_STATE");
        bytes32 appMintState = vm.envBytes32("SOL_IFT_APP_MINT_STATE");
        bytes32 iftBridge = vm.envBytes32("SOL_IFT_BRIDGE");
        bytes32 mint = vm.envBytes32("SOL_IFT_MINT");
        bytes32 mintAuthority = vm.envBytes32("SOL_IFT_MINT_AUTHORITY");
        bytes32 gmpAccount = vm.envBytes32("SOL_IFT_GMP_ACCOUNT");

        vm.startBroadcast();

        address deployed = address(
            new SolanaIFTSendCallConstructor(appState, appMintState, iftBridge, mint, mintAuthority, gmpAccount)
        );

        vm.stopBroadcast();

        string memory json = "json";
        json = json.serialize("solanaIftConstructor", Strings.toHexString(deployed));

        return json;
    }
}
