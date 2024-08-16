// SPDX-License-Identifier: UNLICENSED
pragma solidity >=0.8.25 <0.9.0;

// solhint-disable custom-errors,max-line-length,avoid-low-level-calls

import { TestERC20 } from "./mocks/TestERC20.sol";
import { IICS20TransferMsgs } from "../src/msgs/IICS20TransferMsgs.sol";
import { ICS20Lib } from "../src/utils/ICS20Lib.sol";
import { ICS24Host } from "../src/utils/ICS24Host.sol";
import { FixtureTest } from "./fixtures/FixtureTest.t.sol";

contract BenchmarkTest is FixtureTest {
    function test_ICS20TransferWithSP1Fixture() public {
        Fixture memory ackFixture = loadInitialFixture("acknowledgePacket.json");

        // Step 1: Transfer from Ethereum to Cosmos
        sendTransfer(ackFixture);

        // Step 2: Cosmos has received the packet and commited an acknowledgement, which we will now prove and process
        (bool success,) = address(ics26Router).call(ackFixture.msg);
        assertTrue(success);

        // ack should be deleted
        bytes32 path = ICS24Host.packetCommitmentKeyCalldata(
            ackFixture.packet.sourcePort, ackFixture.packet.sourceChannel, ackFixture.packet.sequence
        );
        bytes32 storedCommitment = ics26Router.getCommitment(path);
        assertEq(storedCommitment, 0);

        // Step 3: Cosmos has sent the tokens back and commited a packet, which we will now prove and receive
        Fixture memory recvFixture = loadFixture("receivePacket.json");

        (success,) = address(ics26Router).call(recvFixture.msg);
        assertTrue(success);

        // ack is written
        bytes32 storedAck = ics26Router.getCommitment(
            ICS24Host.packetAcknowledgementCommitmentKeyCalldata(
                recvFixture.packet.destPort, recvFixture.packet.destChannel, recvFixture.packet.sequence
            )
        );
        assertEq(storedAck, ICS24Host.packetAcknowledgementCommitmentBytes32(ICS20Lib.SUCCESSFUL_ACKNOWLEDGEMENT_JSON));
    }

    function test_ICS20TransferNativeSdkCoinWithSP1Fixture() public {
        Fixture memory recvNativeFixture = loadInitialFixture("receiveNativePacket.json");

        (bool success,) = address(ics26Router).call(recvNativeFixture.msg);
        assertTrue(success);

        bytes32 storedAck = ics26Router.getCommitment(
            ICS24Host.packetAcknowledgementCommitmentKeyCalldata(
                recvNativeFixture.packet.destPort,
                recvNativeFixture.packet.destChannel,
                recvNativeFixture.packet.sequence
            )
        );
        assertEq(storedAck, ICS24Host.packetAcknowledgementCommitmentBytes32(ICS20Lib.SUCCESSFUL_ACKNOWLEDGEMENT_JSON));
    }

    function test_ICS20TimeoutWithSP1Fixture() public {
        Fixture memory timeoutFixture = loadInitialFixture("timeoutPacket.json");

        // Step 1: Transfer from Ethereum to Cosmos
        vm.warp(timeoutFixture.packet.timeoutTimestamp - 30);
        sendTransfer(timeoutFixture);

        // Step 2: Timeout
        vm.warp(timeoutFixture.packet.timeoutTimestamp + 45);
        (bool success,) = address(ics26Router).call(timeoutFixture.msg);
        assertTrue(success);

        // ack should be deleted
        bytes32 path = ICS24Host.packetCommitmentKeyCalldata(
            timeoutFixture.packet.sourcePort, timeoutFixture.packet.sourceChannel, timeoutFixture.packet.sequence
        );
        assertEq(ics26Router.getCommitment(path), 0);
    }

    function sendTransfer(Fixture memory fixture) internal {
        TestERC20 erc20 = TestERC20(fixture.erc20Address);

        ICS20Lib.PacketDataJSON memory packetData = this.unmarshalJSON(fixture.packet.data);

        address user = ICS20Lib.mustHexStringToAddress(packetData.sender);

        uint256 amountToSend = packetData.amount * 1e12;
        erc20.mint(user, amountToSend);
        vm.prank(user);
        erc20.approve(address(ics20Transfer), amountToSend);

        vm.prank(user);
        ics20Transfer.sendTransfer(
            IICS20TransferMsgs.SendTransferMsg({
                denom: packetData.denom,
                amount: amountToSend,
                receiver: packetData.receiver,
                sourceChannel: fixture.packet.sourceChannel,
                destPort: fixture.packet.destPort,
                timeoutTimestamp: fixture.packet.timeoutTimestamp,
                memo: packetData.memo
            })
        );

        bytes32 path = ICS24Host.packetCommitmentKeyCalldata(
            fixture.packet.sourcePort, fixture.packet.sourceChannel, fixture.packet.sequence
        );
        assertEq(ics26Router.getCommitment(path), ICS24Host.packetCommitmentBytes32(fixture.packet));
    }

    function unmarshalJSON(bytes calldata bz) external pure returns (ICS20Lib.PacketDataJSON memory) {
        return ICS20Lib.unmarshalJSON(bz);
    }
}
