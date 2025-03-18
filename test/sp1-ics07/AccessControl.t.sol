// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

import { IAccessControl } from "@openzeppelin/contracts/access/IAccessControl.sol";

import { SP1ICS07MockTest } from "./SP1ICS07MockTest.sol";

contract SP1ICS07AccessControlTest is SP1ICS07MockTest {

    function test_success_setProofSubmitter() public {
        bytes32 defaultAdminRole = ics07Tendermint.DEFAULT_ADMIN_ROLE();
        bytes32 proofSubmitterRole = ics07Tendermint.PROOF_SUBMITTER_ROLE();

        // Ensure that the submitter is the role manager
        assert(ics07Tendermint.hasRole(defaultAdminRole, roleManager));
        assert(ics07Tendermint.hasRole(proofSubmitterRole, roleManager));

        address submitter = makeAddr("submitter");
        vm.prank(roleManager);
        ics07Tendermint.grantRole(proofSubmitterRole, submitter);
        assert(ics07Tendermint.hasRole(proofSubmitterRole, submitter));

        vm.prank(roleManager);
        ics07Tendermint.revokeRole(proofSubmitterRole, submitter);
        assertFalse(ics07Tendermint.hasRole(proofSubmitterRole, submitter));
    }

    function test_failure_setProofSubmitter() public {
        bytes32 defaultAdminRole = ics07Tendermint.DEFAULT_ADMIN_ROLE();
        bytes32 proofSubmitterRole = ics07Tendermint.PROOF_SUBMITTER_ROLE();

        address unauthorized = makeAddr("unauthorized");
        address submitter = makeAddr("submitter");
        vm.expectRevert(
            abi.encodeWithSelector(
                IAccessControl.AccessControlUnauthorizedAccount.selector,
                unauthorized,
                defaultAdminRole
            )
        );
        vm.prank(unauthorized);
        ics07Tendermint.grantRole(proofSubmitterRole, submitter);
        assertFalse(ics07Tendermint.hasRole(proofSubmitterRole, submitter));

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
        address submitter = makeAddr("submitter");
        vm.prank(roleManager);
        ics07Tendermint.grantRole(proofSubmitterRole, submitter);
        assert(ics07Tendermint.hasRole(proofSubmitterRole, submitter));

        // renounce the submitter role
        vm.prank(submitter);
        ics07Tendermint.renounceRole(proofSubmitterRole, submitter);
        assertFalse(ics07Tendermint.hasRole(proofSubmitterRole, submitter));

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
        address submitter = makeAddr("submitter");
        vm.prank(roleManager);
        ics07Tendermint.grantRole(proofSubmitterRole, submitter);
        assert(ics07Tendermint.hasRole(proofSubmitterRole, submitter));

        // fail to renounce the submitter role
        vm.expectRevert(
            abi.encodeWithSelector(
                IAccessControl.AccessControlBadConfirmation.selector
            )
        );
        vm.prank(unauthorized);
        ics07Tendermint.renounceRole(proofSubmitterRole, submitter);
        assert(ics07Tendermint.hasRole(proofSubmitterRole, submitter));

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
}
