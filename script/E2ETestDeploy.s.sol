// SPDX-License-Identifier: UNLICENSED
pragma solidity >=0.8.25 <0.9.0;

import { BaseScript } from "./Base.s.sol";
import { SP1ICS07Tendermint } from "@cosmos/sp1-ics07-tendermint/SP1ICS07Tendermint.sol";
import { SP1Verifier } from "@sp1-contracts/v1.0.1/SP1Verifier.sol";

/// @dev See the Solidity Scripting tutorial: https://book.getfoundry.sh/tutorials/solidity-scripting
contract E2ETestDeploy is BaseScript {
    // solhint-disable-next-line no-empty-blocks
    function run() public returns (address, address, address, address, address) {
        vm.startBroadcast();

        SP1Verifier verifier = new SP1Verifier();
        ics07Tendermint = new SP1ICS07Tendermint(
            genesis.updateClientVkey,
            genesis.membershipVkey,
            genesis.ucAndMembershipVkey,
            address(verifier),
            genesis.trustedClientState,
            trustedConsensusHash
        );

        vm.stopBroadcast();
    }
}
