// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

import { Test } from "forge-std/Test.sol";

import { ICS20Transfer } from "../../contracts/ICS20Transfer.sol";
import { ICS26Router } from "../../contracts/ICS26Router.sol";
import { ICS20Lib } from "../../contracts/utils/ICS20Lib.sol";
import { IBCERC20 } from "../../contracts/utils/IBCERC20.sol";
import { Escrow } from "../../contracts/utils/Escrow.sol";
import { AccessManager } from "@openzeppelin-contracts/access/manager/AccessManager.sol";
import { IBCRolesLib } from "../../contracts/utils/IBCRolesLib.sol";

import { IIBCUUPSUpgradeable } from "./utils/v2.0.0/IIBCUUPSUpgradeable.sol";
import { DeployAccessManagerWithRoles } from "../../scripts/deployments/DeployAccessManagerWithRoles.sol";

contract MainnetForkTest is Test, DeployAccessManagerWithRoles {
    // solhint-disable-next-line var-name-mixedcase
    string public ETH_RPC_URL = vm.envString("ETH_RPC_URL");

    string public clientId = "cosmoshub-0";

    // WARNING: As the mainnet contracts may not be up to date, some interface functions may not work as expected
    // In this case, you should add the necessary interfaces to the `./utils/vX.Y.Z/` directory

    ICS26Router public ics26Proxy = ICS26Router(0x3aF134307D5Ee90faa2ba9Cdba14ba66414CF1A7);
    ICS20Transfer public ics20Proxy = ICS20Transfer(0xa348CfE719B63151F228e3C30EB424BA5a983012);
    address public relayer = 0xC4C09A23dDBd1fF0f313885265113F83622284C2;

    function setUp() public {
        uint256 forkId = vm.createFork(ETH_RPC_URL);
        vm.selectFork(forkId);
    }

    function test_validate_constants() public view {
        address transfer = address(ics26Proxy.getIBCApp(ICS20Lib.DEFAULT_PORT_ID));
        assertEq(transfer, address(ics20Proxy), "ICS20Proxy address is not correct");

        address router = ics20Proxy.ics26();
        assertEq(router, address(ics26Proxy), "ICS26Router address is not correct");

        address ics07 = address(ics26Proxy.getClient(clientId));
        assertTrue(ics07 != address(0), "Client not found");
    }

    function test_success_upgradeAll() public {
        address timelockedAdmin = IIBCUUPSUpgradeable(address(ics26Proxy)).getTimelockedAdmin();

        address escrowLogic = address(new Escrow());
        address ibcERC20Logic = address(new IBCERC20());
        ICS26Router ics26RouterLogic = new ICS26Router();
        ICS20Transfer ics20TransferLogic = new ICS20Transfer();

        AccessManager accessManager = new AccessManager(timelockedAdmin);

        vm.startPrank(timelockedAdmin);
        // Grant basic roles
        accessManagerSetTargetRoles(accessManager, address(ics26Proxy), address(ics20Proxy), false);
        accessManager.grantRole(IBCRolesLib.RELAYER_ROLE, relayer, 0);

        // Upgrade all the contracts (ics20 must be upgraded first)
        ics20Proxy.upgradeToAndCall(
            address(ics20TransferLogic), abi.encodeCall(ICS20Transfer.initializeV2, address(accessManager))
        );

        ics26Proxy.upgradeToAndCall(
            address(ics26RouterLogic), abi.encodeCall(ICS26Router.initializeV2, (address(accessManager)))
        );

        ics20Proxy.upgradeEscrowTo(escrowLogic);
        ics20Proxy.upgradeIBCERC20To(ibcERC20Logic);

        Escrow(ics20Proxy.getEscrow(clientId)).initializeV2(); // can be called by anyone, but only once
        vm.stopPrank();
    }
}
