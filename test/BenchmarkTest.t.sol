// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.28;

// solhint-disable custom-errors,max-line-length,avoid-low-level-calls

import { TestERC20 } from "./mocks/TestERC20.sol";
import { IICS20TransferMsgs } from "../src/msgs/IICS20TransferMsgs.sol";
import { ICS20Lib } from "../src/utils/ICS20Lib.sol";
import { ICS24Host } from "../src/utils/ICS24Host.sol";
import { FixtureTest } from "./fixtures/FixtureTest.t.sol";

contract BenchmarkTest is FixtureTest {
    function test_ICS20TransferWithSP1Fixtures_Plonk() public {
        ICS20TransferWithSP1FixturesTest("acknowledgeMultiPacket_1-plonk.json", "receiveMultiPacket_1-plonk.json", 1);
    }

    function test_ICS20TransferWithSP1Fixtures_Groth16() public {
        ICS20TransferWithSP1FixturesTest(
            "acknowledgeMultiPacket_1-groth16.json", "receiveMultiPacket_1-groth16.json", 1
        );
    }

    function test_ICS20TransferWithSP1Fixtures_100Packets_Plonk() public {
        ICS20TransferWithSP1FixturesTest(
            "acknowledgeMultiPacket_100-plonk.json", "receiveMultiPacket_100-plonk.json", 100
        );
    }

    function test_ICS20TransferWithSP1Fixtures_25Packets_Groth16() public {
        ICS20TransferWithSP1FixturesTest(
            "acknowledgeMultiPacket_25-groth16.json", "receiveMultiPacket_25-groth16.json", 25
        );
    }

    function ICS20TransferWithSP1FixturesTest(string memory ackFix, string memory recvFix, uint64 numPackets) public {
        Fixture memory ackFixture = loadInitialFixture(ackFix);

        // Step 1: Transfer from Ethereum to Cosmos
        for (uint64 i = 0; i < numPackets; i++) {
            sendTransfer(ackFixture);
        }

        // Step 2: Cosmos has received the packet and commited an acknowledgement, which we will now prove and process
        (bool success,) = address(ics26Router).call(ackFixture.msg);
        assertTrue(success);

        // ack should be deleted
        bytes32 path =
            ICS24Host.packetCommitmentKeyCalldata(ackFixture.packet.sourceChannel, ackFixture.packet.sequence);
        bytes32 storedCommitment = ics26Router.IBC_STORE().getCommitment(path);
        assertEq(storedCommitment, 0);

        // Step 3: Cosmos has sent the tokens back and commited a packet, which we will now prove and receive
        Fixture memory recvFixture = loadFixture(recvFix);

        (success,) = address(ics26Router).call(recvFixture.msg);
        assertTrue(success);

        // ack is written
        bytes32 storedAck = ics26Router.IBC_STORE().getCommitment(
            ICS24Host.packetAcknowledgementCommitmentKeyCalldata(
                recvFixture.packet.destChannel, recvFixture.packet.sequence
            )
        );
        assertEq(storedAck, ICS24Host.packetAcknowledgementCommitmentBytes32(singleSuccessAck));
    }

    function test_ICS20TransferNativeSdkCoinWithSP1Fixtures_Plonk() public {
        ICS20TransferNativeSdkCoinWithSP1FixtureTest("receiveNativePacket-plonk.json");
    }

    function test_ICS20TransferNativeSdkCoinWithSP1Fixtures_Groth16() public {
        ICS20TransferNativeSdkCoinWithSP1FixtureTest("receiveNativePacket-groth16.json");
    }

    function ICS20TransferNativeSdkCoinWithSP1FixtureTest(string memory recvNatFix) public {
        Fixture memory recvNativeFixture = loadInitialFixture(recvNatFix);

        (bool success,) = address(ics26Router).call(recvNativeFixture.msg);
        assertTrue(success);

        bytes32 storedAck = ics26Router.IBC_STORE().getCommitment(
            ICS24Host.packetAcknowledgementCommitmentKeyCalldata(
                recvNativeFixture.packet.destChannel, recvNativeFixture.packet.sequence
            )
        );
        assertEq(storedAck, ICS24Host.packetAcknowledgementCommitmentBytes32(singleSuccessAck));
    }

    function test_ICS20TimeoutWithSP1Fixtures_Plonk() public {
        ICS20TimeoutWithSP1FixtureTest("timeoutPacket-plonk.json");
    }

    function test_ICS20TimeoutWithSP1Fixtures_Groth16() public {
        ICS20TimeoutWithSP1FixtureTest("timeoutPacket-groth16.json");
    }

    function ICS20TimeoutWithSP1FixtureTest(string memory timeoutFix) public {
        Fixture memory timeoutFixture = loadInitialFixture(timeoutFix);

        // Step 1: Transfer from Ethereum to Cosmos
        vm.warp(timeoutFixture.packet.timeoutTimestamp - 30);
        sendTransfer(timeoutFixture);

        // Step 2: Timeout
        vm.warp(timeoutFixture.packet.timeoutTimestamp + 45);
        (bool success,) = address(ics26Router).call(timeoutFixture.msg);
        assertTrue(success);

        // ack should be deleted
        bytes32 path =
            ICS24Host.packetCommitmentKeyCalldata(timeoutFixture.packet.sourceChannel, timeoutFixture.packet.sequence);
        assertEq(ics26Router.IBC_STORE().getCommitment(path), 0);
    }

    function sendTransfer(Fixture memory fixture) internal {
        TestERC20 erc20 = TestERC20(fixture.erc20Address);

        ICS20Lib.PacketDataJSON memory packetData = this.unmarshalJSON(fixture.packet.payloads[0].value);

        address user = ICS20Lib.mustHexStringToAddress(packetData.sender);

        uint256 amountToSend = packetData.amount;
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
                destPort: fixture.packet.payloads[0].destPort,
                timeoutTimestamp: fixture.packet.timeoutTimestamp,
                memo: packetData.memo
            })
        );

        bytes32 path = ICS24Host.packetCommitmentKeyCalldata(fixture.packet.sourceChannel, fixture.packet.sequence);
        assertEq(ics26Router.IBC_STORE().getCommitment(path), ICS24Host.packetCommitmentBytes32(fixture.packet));
    }

    function unmarshalJSON(bytes calldata bz) external pure returns (ICS20Lib.PacketDataJSON memory) {
        return ICS20Lib.unmarshalJSON(bz);
    }
}
