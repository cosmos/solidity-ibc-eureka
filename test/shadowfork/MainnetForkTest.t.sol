// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

import { Test } from "forge-std/Test.sol";

import { ICS20Transfer } from "../../contracts/ICS20Transfer.sol";
import { ICS26Router } from "../../contracts/ICS26Router.sol";
import { ICS20Lib } from "../../contracts/utils/ICS20Lib.sol";
import { ERC1967Utils } from "@openzeppelin-contracts/proxy/ERC1967/ERC1967Utils.sol";

contract MainnetForkTest is Test {
    // solhint-disable-next-line var-name-mixedcase
    string public ETH_RPC_URL = vm.envString("ETH_RPC_URL");

    string public clientId = "cosmoshub-0";

    // WARNING: As the mainnet contracts may not be up to date, some interface functions may not work as expected
    // In this case, you should make the calls manually instead of casting to the interface

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

        assertTrue(ics26Proxy.hasRole(ics26Proxy.RELAYER_ROLE(), relayer), "Relayer not found");
    }

    function test_migrate_ics26() public {
        address timelockedAdmin = ics26Proxy.getTimelockedAdmin();
        assertTrue(timelockedAdmin != address(0), "Timelocked admin not found");

        address newLogic = address(new ICS26Router());
        vm.prank(timelockedAdmin);
        ics26Proxy.upgradeToAndCall(address(newLogic), bytes(""));

        // Check that the implementation has been updated
        bytes32 value = vm.load(address(ics26Proxy), ERC1967Utils.IMPLEMENTATION_SLOT);
        address implementation = address(uint160(uint256(value)));
        assertEq(implementation, newLogic, "Implementation not updated");

        // Check that the relayer is still whitelisted
        assertTrue(ics26Proxy.hasRole(ics26Proxy.RELAYER_ROLE(), relayer), "Relayer not found");
    }

    function test_migrate_ics20() public {
        address timelockedAdmin = ics26Proxy.getTimelockedAdmin();
        assertTrue(timelockedAdmin != address(0), "Timelocked admin not found");

        // Verify that the current implementation does not have ERC20_CUSTOMIZER_ROLE
        vm.expectRevert();
        ics20Proxy.ERC20_CUSTOMIZER_ROLE();

        address newLogic = address(new ICS20Transfer());
        vm.prank(timelockedAdmin);
        ics20Proxy.upgradeToAndCall(address(newLogic), bytes(""));

        // Check that the implementation has been updated
        bytes32 value = vm.load(address(ics20Proxy), ERC1967Utils.IMPLEMENTATION_SLOT);
        address implementation = address(uint160(uint256(value)));
        assertEq(implementation, newLogic, "Implementation not updated");

        // Verify that the current implementation does have ERC20_CUSTOMIZER_ROLE
        bytes32 erc20CustomizerRole = ics20Proxy.ERC20_CUSTOMIZER_ROLE();
        address customizer = makeAddr("customizer");
        vm.prank(timelockedAdmin);
        ics20Proxy.grantERC20CustomizerRole(customizer);
        assertTrue(ics20Proxy.hasRole(erc20CustomizerRole, customizer), "Customizer not found");
    }
}
