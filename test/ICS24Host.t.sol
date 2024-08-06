// SPDX-License-Identifier: UNLICENSED
pragma solidity >=0.8.25 <0.9.0;

// solhint-disable custom-errors,max-line-length

import { Test } from "forge-std/Test.sol";
import { ICS24Host } from "../src/utils/ICS24Host.sol";
import { IICS26RouterMsgs } from "../src/msgs/IICS26RouterMsgs.sol";
import { Strings } from "@openzeppelin/contracts/utils/Strings.sol";
import { SafeCast } from "@openzeppelin/contracts/utils/math/SafeCast.sol";

contract ICS24HostTest is Test {
    function testFuzz_packetCommitmentBytes32(uint64 timestampInSeconds) public pure {
        vm.assume(timestampInSeconds < ICS24Host.SECONDS_THRESHOLD); // ~100 years from now
        vm.assume(timestampInSeconds > 946_681_200); // year 2000

        IICS26RouterMsgs.Packet memory packetWithSeconds = IICS26RouterMsgs.Packet({
            sequence: 1,
            timeoutTimestamp: timestampInSeconds,
            sourcePort: "source-port",
            sourceChannel: "source-channel",
            destPort: "destination-port",
            destChannel: "destination-channel",
            version: "ics20-1",
            data: bytes("data")
        });
        bytes32 packetCommitmentWithSeconds = ICS24Host.packetCommitmentBytes32(packetWithSeconds);

        IICS26RouterMsgs.Packet memory packetWithNanoSeconds = IICS26RouterMsgs.Packet({
            sequence: 1,
            timeoutTimestamp: SafeCast.toUint64(uint256(timestampInSeconds) * 1_000_000_000),
            sourcePort: "source-port",
            sourceChannel: "source-channel",
            destPort: "destination-port",
            destChannel: "destination-channel",
            version: "ics20-1",
            data: bytes("data")
        });
        bytes32 packetCommitmentWithNanoseconds = ICS24Host.packetCommitmentBytes32(packetWithNanoSeconds);

        assertEq(
            packetCommitmentWithSeconds,
            packetCommitmentWithNanoseconds,
            string(
                abi.encodePacked(
                    "timestamp in seconds: ",
                    Strings.toString(packetWithSeconds.timeoutTimestamp),
                    ", timestamp in nanoseconds: ",
                    Strings.toString(packetWithNanoSeconds.timeoutTimestamp)
                )
            )
        );
    }
}
