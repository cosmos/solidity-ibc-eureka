// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

import { Script } from "forge-std/Script.sol";
import { ICS26Router } from "../contracts/ICS26Router.sol";
import { IICS02ClientMsgs } from "../contracts/msgs/IICS02ClientMsgs.sol";

contract FixMerklePrefix is Script {
    bytes[] public attestorMerklePrefix = [bytes("")];

    // TODO: enter the client id of your deployed light client
    string public constant CUSTOM_CLIENT_ID = "custom-0";

    // TODO: enter the address of your deployed ICS26 Router
    address public ics26RouterAddress = makeAddr("TODO");

    // solhint-disable-next-line function-max-lines
    function run() public {
        IICS02ClientMsgs.CounterpartyInfo memory info =
            ICS26Router(ics26RouterAddress).getCounterparty(CUSTOM_CLIENT_ID);
        info.merklePrefix = attestorMerklePrefix;

        vm.startBroadcast();

        ICS26Router(ics26RouterAddress).migrateClient(
            CUSTOM_CLIENT_ID, info, address(ICS26Router(ics26RouterAddress).getClient(CUSTOM_CLIENT_ID))
        );

        vm.stopBroadcast();
    }
}
