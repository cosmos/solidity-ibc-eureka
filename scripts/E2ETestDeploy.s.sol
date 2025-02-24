// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.28;

/*
    This script is used for end-to-end testing
*/

// solhint-disable custom-errors,gas-custom-errors

import { stdJson } from "forge-std/StdJson.sol";
import { Script } from "forge-std/Script.sol";
import { SP1ICS07Tendermint } from "../contracts/light-clients/SP1ICS07Tendermint.sol";
import { IICS07TendermintMsgs } from "../contracts/light-clients/msgs/IICS07TendermintMsgs.sol";
import { ICS26Router } from "../contracts/ICS26Router.sol";
import { ICS20Transfer } from "../contracts/ICS20Transfer.sol";
import { TestERC20 } from "../test/solidity-ibc/mocks/TestERC20.sol";
import { Strings } from "@openzeppelin-contracts/utils/Strings.sol";
import { ICS20Lib } from "../contracts/utils/ICS20Lib.sol";
import { ERC1967Proxy } from "@openzeppelin-contracts/proxy/ERC1967/ERC1967Proxy.sol";
import { IBCERC20 } from "../contracts/utils/IBCERC20.sol";
import { Escrow } from "../contracts/utils/Escrow.sol";
import { Deployments } from "./helpers/Deployments.sol";
import { DeploySP1ICS07Tendermint } from "./deployments/DeploySP1ICS07Tendermint.sol";

/// @dev See the Solidity Scripting tutorial: https://book.getfoundry.sh/tutorials/solidity-scripting
contract E2ETestDeploy is Script, IICS07TendermintMsgs, DeploySP1ICS07Tendermint, Deployments {
    using stdJson for string;

    string internal constant SP1_GENESIS_DIR = "/scripts/";

    address public verifier;

    function run() public returns (string memory) {
        // ============ Step 1: Load parameters ==============
        ConsensusState memory trustedConsensusState;
        ClientState memory trustedClientState;
        SP1ICS07Tendermint ics07Tendermint;

        string memory root = vm.projectRoot();
        string memory path = string.concat(root, SP1_GENESIS_DIR, "genesis.json");

        Deployments.SP1ICS07TendermintDeployment memory genesis = Deployments.loadSP1ICS07TendermintDeployment(vm, path);

        genesis.verifier = vm.envOr("VERIFIER", string(""));

        address e2eFaucet = vm.envAddress("E2E_FAUCET_ADDRESS");

        // ============ Step 2: Deploy the contracts ==============

        vm.startBroadcast();

        (ics07Tendermint, trustedConsensusState, trustedClientState) = deploySP1ICS07Tendermint(genesis);

        // Deploy IBC Eureka with proxy
        address escrowLogic = address(new Escrow());
        address ibcERC20Logic = address(new IBCERC20());
        address ics26RouterLogic = address(new ICS26Router());
        address ics20TransferLogic = address(new ICS20Transfer());

        ERC1967Proxy routerProxy = new ERC1967Proxy(
            ics26RouterLogic, abi.encodeWithSelector(ICS26Router.initialize.selector, msg.sender, msg.sender)
        );

        ERC1967Proxy transferProxy = new ERC1967Proxy(
            ics20TransferLogic,
            abi.encodeWithSelector(
                ICS20Transfer.initialize.selector,
                address(routerProxy),
                escrowLogic,
                ibcERC20Logic,
                address(0),
                address(0)
            )
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
        json.serialize("ics07Tendermint", Strings.toHexString(address(ics07Tendermint)));
        json.serialize("ics26Router", Strings.toHexString(address(ics26Router)));
        json.serialize("ics20Transfer", Strings.toHexString(address(ics20Transfer)));
        string memory finalJson = json.serialize("erc20", Strings.toHexString(address(erc20)));

        return finalJson;
    }
}
