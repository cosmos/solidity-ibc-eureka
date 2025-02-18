// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.28;

/*
    This script is used for deploying auxiliary contracts to a live network (it could be local, but is geared towards testnet)
*/

import { Script } from "forge-std/Script.sol";
import { TestnetERC20 } from "./TestnetERC20.sol";
import { SP1Verifier as SP1VerifierPlonk } from "@sp1-contracts/v4.0.0-rc.3/SP1VerifierPlonk.sol";
import { SP1Verifier as SP1VerifierGroth16 } from "@sp1-contracts/v4.0.0-rc.3/SP1VerifierGroth16.sol";
import { SP1MockVerifier } from "@sp1-contracts/SP1MockVerifier.sol";
import { TestnetLightClient } from "./TestnetLightClient.sol";
import "forge-std/console.sol";

/// @dev See the Solidity Scripting tutorial: https://book.getfoundry.sh/guides/scripting-with-solidity
contract MintTestnetTokens is Script {
    function run() public {
        address to = vm.promptAddress("Enter the address to mint tokens to");
        uint256 amount = vm.promptUint("Enter the amount of tokens to mint");
        TestnetERC20 testnetERC20 = TestnetERC20(address(0xA4ff49eb6E2Ea77d7D8091f1501385078642603f));

        vm.startBroadcast();
        testnetERC20.mint(to, amount);
        // address admin = vm.promptAddress("Enter the admin address");
        //
        // // Deploy SP1 verifiers
        // address mockVerifier = address(new SP1MockVerifier());
        // address plonkVerifier = address(new SP1VerifierPlonk());
        // address groth16Verifier = address(new SP1VerifierGroth16());
        //
        // address testnetERC20 = address(new TestnetERC20(admin));
        //
        // address testnetLightClient = address(new TestnetLightClient());
        //
        // vm.stopBroadcast();
        //
        // console.log("SP1 mock verifier:", mockVerifier);
        // console.log("SP1 plonk verifier:", plonkVerifier);
        // console.log("SP1 groth16 verifier:", groth16Verifier);
        // console.log("Testnet ERC20:", testnetERC20);
        // console.log("Testnet light client:", testnetLightClient);
    }
}
