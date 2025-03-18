// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

import { IICS07TendermintMsgs } from "../../contracts/light-clients/msgs/IICS07TendermintMsgs.sol";
import { ILightClientMsgs } from "../../contracts/msgs/ILightClientMsgs.sol";
import { IICS02ClientMsgs } from "../../contracts/msgs/IICS02ClientMsgs.sol";
import { IUpdateClientMsgs } from "../../contracts/light-clients/msgs/IUpdateClientMsgs.sol";

import { IAccessControl } from "@openzeppelin/contracts/access/IAccessControl.sol";

import { SP1ICS07MockTest } from "./SP1ICS07MockTest.sol";

contract SP1ICS07AccessControlTest is SP1ICS07MockTest {

    function test_success_setProofSubmitter() public {
        bytes32 defaultAdminRole = ics07Tendermint.DEFAULT_ADMIN_ROLE();
        bytes32 proofSubmitterRole = ics07Tendermint.PROOF_SUBMITTER_ROLE();

        // Ensure that the submitter is the role manager
        assert(ics07Tendermint.hasRole(defaultAdminRole, roleManager));
        assert(ics07Tendermint.hasRole(proofSubmitterRole, roleManager));

        address newSubmitter = makeAddr("newSubmitter");
        vm.prank(roleManager);
        ics07Tendermint.grantRole(proofSubmitterRole, newSubmitter);
        assert(ics07Tendermint.hasRole(proofSubmitterRole, newSubmitter));

        vm.prank(roleManager);
        ics07Tendermint.revokeRole(proofSubmitterRole, newSubmitter);
        assertFalse(ics07Tendermint.hasRole(proofSubmitterRole, newSubmitter));
    }

    function test_failure_setProofSubmitter() public {
        bytes32 defaultAdminRole = ics07Tendermint.DEFAULT_ADMIN_ROLE();
        bytes32 proofSubmitterRole = ics07Tendermint.PROOF_SUBMITTER_ROLE();

        address unauthorized = makeAddr("unauthorized");
        address newSubmitter = makeAddr("newSubmitter");
        vm.expectRevert(
            abi.encodeWithSelector(
                IAccessControl.AccessControlUnauthorizedAccount.selector,
                unauthorized,
                defaultAdminRole
            )
        );
        vm.prank(unauthorized);
        ics07Tendermint.grantRole(proofSubmitterRole, newSubmitter);
        assertFalse(ics07Tendermint.hasRole(proofSubmitterRole, newSubmitter));

        vm.expectRevert(
            abi.encodeWithSelector(
                IAccessControl.AccessControlUnauthorizedAccount.selector,
                unauthorized,
                defaultAdminRole
            )
        );
        vm.prank(unauthorized);
        ics07Tendermint.revokeRole(proofSubmitterRole, roleManager);
        assert(ics07Tendermint.hasRole(proofSubmitterRole, roleManager));
    }

    function test_success_roleRenounce() public {
        bytes32 defaultAdminRole = ics07Tendermint.DEFAULT_ADMIN_ROLE();
        bytes32 proofSubmitterRole = ics07Tendermint.PROOF_SUBMITTER_ROLE();

        // give the submitter the role
        address newSubmitter = makeAddr("newSubmitter");
        vm.prank(roleManager);
        ics07Tendermint.grantRole(proofSubmitterRole, newSubmitter);
        assert(ics07Tendermint.hasRole(proofSubmitterRole, newSubmitter));

        // renounce the submitter role
        vm.prank(newSubmitter);
        ics07Tendermint.renounceRole(proofSubmitterRole, newSubmitter);
        assertFalse(ics07Tendermint.hasRole(proofSubmitterRole, newSubmitter));

        // renounce the admin role
        vm.prank(roleManager);
        ics07Tendermint.renounceRole(defaultAdminRole, roleManager);
        assertFalse(ics07Tendermint.hasRole(defaultAdminRole, roleManager));
    }

    function test_failure_roleRenounce() public {
        bytes32 defaultAdminRole = ics07Tendermint.DEFAULT_ADMIN_ROLE();
        bytes32 proofSubmitterRole = ics07Tendermint.PROOF_SUBMITTER_ROLE();

        // give the submitter the role
        address unauthorized = makeAddr("unauthorized");
        address newSubmitter = makeAddr("newSubmitter");
        vm.prank(roleManager);
        ics07Tendermint.grantRole(proofSubmitterRole, newSubmitter);
        assert(ics07Tendermint.hasRole(proofSubmitterRole, newSubmitter));

        // fail to renounce the submitter role
        vm.expectRevert(
            abi.encodeWithSelector(
                IAccessControl.AccessControlBadConfirmation.selector
            )
        );
        vm.prank(unauthorized);
        ics07Tendermint.renounceRole(proofSubmitterRole, newSubmitter);
        assert(ics07Tendermint.hasRole(proofSubmitterRole, newSubmitter));

        // fail to renounce the admin role
        vm.expectRevert(
            abi.encodeWithSelector(
                IAccessControl.AccessControlBadConfirmation.selector
            )
        );
        vm.prank(unauthorized);
        ics07Tendermint.renounceRole(defaultAdminRole, roleManager);
        assert(ics07Tendermint.hasRole(defaultAdminRole, roleManager));
    }

    function test_success_updateClient() public {
        bytes32 proofSubmitterRole = ics07Tendermint.PROOF_SUBMITTER_ROLE();

        // role manager is the submitter
        bytes memory updateMsg = newUpdateClientMsg();
        vm.prank(roleManager);
        ILightClientMsgs.UpdateResult res = ics07Tendermint.updateClient(updateMsg);
        assert(res == ILightClientMsgs.UpdateResult.Update);

        // submitter is not the role manager
        updateMsg = newUpdateClientMsg();
        vm.prank(proofSubmitter);
        res = ics07Tendermint.updateClient(updateMsg);
        assert(res == ILightClientMsgs.UpdateResult.Update);

        // role manager allows anyone to update the client
        vm.prank(roleManager);
        ics07Tendermint.grantRole(proofSubmitterRole, address(0));

        // anyone can update the client
        address anyAddr = makeAddr("anyAddr");
        updateMsg = newUpdateClientMsg();
        vm.prank(anyAddr);
        res = ics07Tendermint.updateClient(updateMsg);
        assert(res == ILightClientMsgs.UpdateResult.Update);
    }

    function test_failure_updateClient() public {
        // unauthorized account
        address unauthorized = makeAddr("unauthorized");
        bytes memory updateMsg = newUpdateClientMsg();
        vm.expectRevert(
            abi.encodeWithSelector(
                IAccessControl.AccessControlUnauthorizedAccount.selector,
                unauthorized,
                ics07Tendermint.PROOF_SUBMITTER_ROLE()
            )
        );
        vm.prank(unauthorized);
        ics07Tendermint.updateClient(updateMsg);
    }
}
