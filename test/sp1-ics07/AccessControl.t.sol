// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

import { SP1ICS07TendermintTest } from "./SP1ICS07TendermintTest.sol";
import { stdJson } from "forge-std/StdJson.sol";

contract SP1ICS07AccessControlTest is SP1ICS07TendermintTest {
    address public submitter = makeAddr("submitter");

    function setUp() public {
        setUpTest("update_client_fixture-groth16.json", submitter);

        ClientState memory clientState = abi.decode(mockIcs07Tendermint.getClientState(), (ClientState));
    }
}
