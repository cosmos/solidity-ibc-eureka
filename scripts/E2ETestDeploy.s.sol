// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.28;

/*
    This script is used for end-to-end testing
*/

// solhint-disable custom-errors,gas-custom-errors

import { stdJson } from "forge-std/StdJson.sol";
import { Script } from "forge-std/Script.sol";
import { ISP1ICS07Tendermint } from "../contracts/light-clients/SP1ICS07Tendermint.sol";
import { IICS07TendermintMsgs } from "../contracts/light-clients/msgs/IICS07TendermintMsgs.sol";
import { IICS26Router } from "../contracts/interfaces/IICS26Router.sol";
import { IICS20Transfer } from "../contracts/interfaces/IICS20Transfer.sol";
import { TestERC20 } from "../test/solidity-ibc/mocks/TestERC20.sol";
import { ICS20Lib } from "../contracts/utils/ICS20Lib.sol";
import { SP1Verifier as SP1VerifierPlonk } from "@sp1-contracts/v4.0.0-rc.3/SP1VerifierPlonk.sol";
import { SP1Verifier as SP1VerifierGroth16 } from "@sp1-contracts/v4.0.0-rc.3/SP1VerifierGroth16.sol";
import { SP1MockVerifier } from "@sp1-contracts/SP1MockVerifier.sol";
import { Strings } from "@openzeppelin-contracts/utils/Strings.sol";
import { DeployLib } from "./DeployLib.sol";

/// @dev See the Solidity Scripting tutorial: https://book.getfoundry.sh/guides/scripting-with-solidity
contract E2ETestDeploy is Script, IICS07TendermintMsgs {
    using stdJson for string;

    function run() public returns (string memory) {
        // ============ Step 1: Load parameters ==============
        string memory root = vm.projectRoot();
        string memory tendermintGenesisJson = vm.readFile(string.concat(root, "/scripts/genesis.json"));
        DeployLib.SP1ICS07TendermintGenesisJson memory genesis = DeployLib.loadTendermintGenesisFromJson(tendermintGenesisJson);
        ClientState memory trustedClientState = abi.decode(genesis.trustedClientState, (ClientState));

        address e2eFaucet = vm.envAddress("E2E_FAUCET_ADDRESS");

        // The verifier address can be set in the environment variables.
        // If not set, then the verifier is set based on the zkAlgorithm.
        // If set to "mock", then the verifier is set to a mock verifier.
        string memory verifierEnv = vm.envOr("VERIFIER", string(""));

        // ============ Step 2: Deploy the contracts ==============
        vm.startBroadcast();

        address verifier;
        if (keccak256(bytes(verifierEnv)) == keccak256(bytes("mock"))) {
            verifier = address(new SP1MockVerifier());
        } else if (bytes(verifierEnv).length > 0) {
            (bool success, address verifierAddr) = Strings.tryParseAddress(verifierEnv);
            require(success, string.concat("Invalid verifier address: ", verifierEnv));
            verifier = verifierAddr;
        } else if (trustedClientState.zkAlgorithm == SupportedZkAlgorithm.Plonk) {
            verifier = address(new SP1VerifierPlonk());
        } else if (trustedClientState.zkAlgorithm == SupportedZkAlgorithm.Groth16) {
            verifier = address(new SP1VerifierGroth16());
        } else {
            revert("Unsupported zk algorithm");
        }

        // Deploy the SP1 ICS07 Tendermint light client
        ISP1ICS07Tendermint ics07Tendermint = DeployLib.deployTendermintLightClient(genesis, verifier);

        // Deploy IBC Eureka
        (IICS26Router ics26Router, IICS20Transfer ics20Transfer) = DeployLib.deployIBCCore(DeployLib.DeploymentConfigJson(msg.sender, msg.sender, address(0), address(0)));
        // Wire Transfer app
        ics26Router.addIBCApp(ICS20Lib.DEFAULT_PORT_ID, address(ics20Transfer));


        // Deploy a test ERC20
        TestERC20 erc20 = new TestERC20();

        // Mint some tokens
        erc20.mint(e2eFaucet, type(uint256).max);

        vm.stopBroadcast();

        string memory json = "json";
        json.serialize("ics07Tendermint", Strings.toHexString(address(ics07Tendermint)));
        json.serialize("ics26Router", Strings.toHexString(address(ics26Router)));
        json.serialize("ics20Transfer", Strings.toHexString(address(ics20Transfer)));
        string memory finalJson = json.serialize("erc20", Strings.toHexString(address(erc20)));

        return finalJson;
    }
}
