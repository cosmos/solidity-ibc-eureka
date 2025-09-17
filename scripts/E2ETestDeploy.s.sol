// SPDX-License-Identifier: MIT
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
import { ICS27GMP } from "../contracts/ICS27GMP.sol";
import { TestERC20 } from "../test/solidity-ibc/mocks/TestERC20.sol";
import { Strings } from "@openzeppelin-contracts/utils/Strings.sol";
import { ICS20Lib } from "../contracts/utils/ICS20Lib.sol";
import { ICS27Lib } from "../contracts/utils/ICS27Lib.sol";
import { ERC1967Proxy } from "@openzeppelin-contracts/proxy/ERC1967/ERC1967Proxy.sol";
import { DeployAccessManagerWithRoles } from "./deployments/DeployAccessManagerWithRoles.sol";
import { IBCERC20 } from "../contracts/utils/IBCERC20.sol";
import { Escrow } from "../contracts/utils/Escrow.sol";
import { ICS27Account } from "../contracts/utils/ICS27Account.sol";
import { SP1Verifier as SP1VerifierPlonk } from "@sp1-contracts/v5.0.0/SP1VerifierPlonk.sol";
import { SP1Verifier as SP1VerifierGroth16 } from "@sp1-contracts/v5.0.0/SP1VerifierGroth16.sol";
import { SP1MockVerifier } from "@sp1-contracts/SP1MockVerifier.sol";
import { AccessManager } from "@openzeppelin-contracts/access/manager/AccessManager.sol";
import { IBCXERC20 } from "../contracts/demo/IBCXERC20.sol";

/// @dev See the Solidity Scripting tutorial: https://getfoundry.sh/guides/scripting-with-solidity
contract E2ETestDeploy is Script, IICS07TendermintMsgs, DeployAccessManagerWithRoles {
    using stdJson for string;

    string internal constant SP1_GENESIS_DIR = "/scripts/";

    // solhint-disable-next-line function-max-lines
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
        address ics26RouterLogic = address(new ICS26Router());
        address ics20TransferLogic = address(new ICS20Transfer());
        address ics27GmpLogic = address(new ICS27GMP());
        address ibcxerc20Logic = address(new IBCXERC20());

        AccessManager accessManager = new AccessManager(msg.sender);

        ERC1967Proxy routerProxy =
            new ERC1967Proxy(ics26RouterLogic, abi.encodeCall(ICS26Router.initialize, (address(accessManager))));

        ERC1967Proxy transferProxy = new ERC1967Proxy(
            ics20TransferLogic,
            abi.encodeCall(
                ICS20Transfer.initialize,
                (
                    address(routerProxy),
                    address(new Escrow()),
                    address(new IBCERC20()),
                    address(0),
                    address(accessManager)
                )
            )
        );

        ERC1967Proxy gmpProxy = new ERC1967Proxy(
            ics27GmpLogic,
            abi.encodeCall(
                ICS27GMP.initialize, (address(routerProxy), address(new ICS27Account()), address(accessManager))
            )
        );

        // Wire up the IBCAdmin and access control
        accessManagerSetTargetRoles(accessManager, address(routerProxy), address(transferProxy), true);

        accessManagerSetRoles(
            accessManager, new address[](0), new address[](0), new address[](0), msg.sender, msg.sender, msg.sender
        );

        // Wire Transfer app
        ICS26Router(address(routerProxy)).addIBCApp(ICS20Lib.DEFAULT_PORT_ID, address(transferProxy));
        ICS26Router(address(routerProxy)).addIBCApp(ICS27Lib.DEFAULT_PORT_ID, address(gmpProxy));

        // Mint some tokens
        TestERC20 erc20 = new TestERC20();
        erc20.mint(e2eFaucet, type(uint256).max);

        IBCXERC20 ibcxerc20 = IBCXERC20(
            address(
                new ERC1967Proxy(
                    ibcxerc20Logic,
                    abi.encodeCall(IBCXERC20.initialize, (msg.sender, "WildFlower", "uwfdeposit", address(gmpProxy)))
                )
            )
        );
        ibcxerc20.mint(e2eFaucet, type(uint256).max / 2);

        vm.stopBroadcast();

        string memory json = "json";
        json.serialize("verifierPlonk", Strings.toHexString(address(verifierPlonk)));
        json.serialize("verifierGroth16", Strings.toHexString(address(verifierGroth16)));
        json.serialize("verifierMock", Strings.toHexString(address(verifierMock)));
        json.serialize("ics26Router", Strings.toHexString(address(routerProxy)));
        json.serialize("ics20Transfer", Strings.toHexString(address(transferProxy)));
        json.serialize("ics27Gmp", Strings.toHexString(address(gmpProxy)));
        json.serialize("ibcxerc20", Strings.toHexString(address(ibcxerc20)));
        string memory finalJson = json.serialize("erc20", Strings.toHexString(address(erc20)));

        return finalJson;
    }
}
