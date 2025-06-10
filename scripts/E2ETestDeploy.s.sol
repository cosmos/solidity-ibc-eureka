// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.28;

/*
    This script is used for end-to-end testing
*/

// solhint-disable custom-errors,gas-custom-errors

import { stdJson } from "forge-std/StdJson.sol";
import { Script } from "forge-std/Script.sol";
import { IICS07TendermintMsgs } from "../contracts/light-clients/msgs/IICS07TendermintMsgs.sol";
import { ICS26Router } from "../contracts/ICS26Router.sol";
import { ICS20Transfer } from "../contracts/ICS20Transfer.sol";
import { ICS26Router } from "../contracts/ICS26Router.sol";
import { TestERC20 } from "../test/solidity-ibc/mocks/TestERC20.sol";
import { Strings } from "@openzeppelin-contracts/utils/Strings.sol";
import { ICS20Lib } from "../contracts/utils/ICS20Lib.sol";
import { ERC1967Proxy } from "@openzeppelin-contracts/proxy/ERC1967/ERC1967Proxy.sol";
import { IBCERC20 } from "../contracts/utils/IBCERC20.sol";
import { Escrow } from "../contracts/utils/Escrow.sol";
import { DeployProxiedICS20Transfer } from "./deployments/DeployProxiedICS20Transfer.sol";
import { DeployProxiedICS26Router } from "./deployments/DeployProxiedICS26Router.sol";
import { SP1Verifier as SP1VerifierPlonk } from "@sp1-contracts/v5.0.0/SP1VerifierPlonk.sol";
import { SP1Verifier as SP1VerifierGroth16 } from "@sp1-contracts/v5.0.0/SP1VerifierGroth16.sol";
import { SP1MockVerifier } from "@sp1-contracts/SP1MockVerifier.sol";

/// @dev See the Solidity Scripting tutorial: https://book.getfoundry.sh/tutorials/solidity-scripting
contract E2ETestDeploy is Script, IICS07TendermintMsgs, DeployProxiedICS26Router, DeployProxiedICS20Transfer {
    using stdJson for string;

    string internal constant SP1_GENESIS_DIR = "/scripts/";

    address[] public publicRelayers = [address(0)];

    function run() public returns (string memory) {
        // ============ Step 1: Load parameters ==============
        address e2eFaucet = vm.envAddress("E2E_FAUCET_ADDRESS");

        // ============ Step 2: Deploy the contracts ==============

        vm.startBroadcast();

        // Deploy the SP1 verifiers for testing
        address verifierPlonk = address(new SP1VerifierPlonk());
        address verifierGroth16 = address(new SP1VerifierGroth16());
        address verifierMock = address(new SP1MockVerifier());

        // Deploy IBC Eureka with proxy
        address escrowLogic = address(new Escrow());
        address ibcERC20Logic = address(new IBCERC20());
        address ics26RouterLogic = address(new ICS26Router());
        address ics20TransferLogic = address(new ICS20Transfer());

        ERC1967Proxy routerProxy =
            deployProxiedICS26Router(ics26RouterLogic, msg.sender, msg.sender, msg.sender, publicRelayers);

        ERC1967Proxy transferProxy = deployProxiedICS20Transfer(
            ics20TransferLogic,
            address(routerProxy),
            escrowLogic,
            ibcERC20Logic,
            new address[](0),
            new address[](0),
            address(0),
            address(0)
        );

        ICS26Router ics26Router = ICS26Router(address(routerProxy));
        ICS20Transfer ics20Transfer = ICS20Transfer(address(transferProxy));
        TestERC20 erc20 = new TestERC20();
        // Wire Transfer app
        ics26Router.addIBCApp(ICS20Lib.DEFAULT_PORT_ID, address(ics20Transfer));

        // Mint some tokens
        erc20.mint(e2eFaucet, type(uint256).max);

        vm.stopBroadcast();

        string memory json = "json";
        json.serialize("verifierPlonk", Strings.toHexString(address(verifierPlonk)));
        json.serialize("verifierGroth16", Strings.toHexString(address(verifierGroth16)));
        json.serialize("verifierMock", Strings.toHexString(address(verifierMock)));
        json.serialize("ics26Router", Strings.toHexString(address(ics26Router)));
        json.serialize("ics20Transfer", Strings.toHexString(address(ics20Transfer)));
        json.serialize("ibcERC20Logic", Strings.toHexString(address(ibcERC20Logic)));
        // TODO: resolve finalJson vs json
        string memory finalJson = json.serialize("erc20", Strings.toHexString(address(erc20)));

        return finalJson;
    }
}
