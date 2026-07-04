// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

// solhint-disable gas-small-strings

import { Test } from "forge-std/Test.sol";

import { ICS20Transfer } from "../../contracts/ICS20Transfer.sol";
import { ICS26Router } from "../../contracts/ICS26Router.sol";
import { ICS20Lib } from "../../contracts/utils/ICS20Lib.sol";

contract MainnetForkTest is Test {
    // solhint-disable-next-line var-name-mixedcase
    string public ETH_RPC_URL = vm.envString("ETH_RPC_URL");

    string public clientId = "cosmoshub-0";

    ICS26Router public ics26Proxy = ICS26Router(0x3aF134307D5Ee90faa2ba9Cdba14ba66414CF1A7);
    ICS20Transfer public ics20Proxy = ICS20Transfer(0xa348CfE719B63151F228e3C30EB424BA5a983012);

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

    function test_postUpgradeAuthorityInvariant() public view {
        address authority = ics26Proxy.authority();
        assertNotEq(authority, address(0), "ICS26 authority is not set");
        assertEq(ics20Proxy.authority(), authority, "ICS20 authority mismatch");
    }
}
