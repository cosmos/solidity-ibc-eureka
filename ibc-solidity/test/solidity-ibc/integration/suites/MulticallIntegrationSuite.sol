// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.28;

// solhint-disable custom-errors,max-line-length,max-states-count,gas-small-strings

import { IICS26RouterMsgs } from "../../../../contracts/msgs/IICS26RouterMsgs.sol";
import { IIBCMulticallMsgs } from "../../../../contracts/msgs/IIBCMulticallMsgs.sol";
import { IIFTMsgs } from "../../../../contracts/msgs/IIFTMsgs.sol";

import { IICS26Router, IICS26RouterAccessControlled } from "../../../../contracts/interfaces/IICS26Router.sol";
import { IIBCMulticall } from "../../../../contracts/interfaces/IIBCMulticall.sol";
import { IIFT } from "../../../../contracts/interfaces/IIFT.sol";
import { IIFTRateLimit } from "../../../../contracts/interfaces/IIFTRateLimit.sol";
import { IIFTErrors } from "../../../../contracts/errors/IIFTErrors.sol";
import { IAccessManaged } from "@openzeppelin-contracts/access/manager/IAccessManaged.sol";

import { IbcImpl } from "../../utils/IbcImpl.sol";
import { LightClientDriverTest } from "../drivers/LightClientDriverTest.sol";
import { TestHelper } from "../../utils/TestHelper.sol";
import { IntegrationEnv } from "../../utils/IntegrationEnv.sol";
import { IBCMulticall } from "../../../../contracts/utils/IBCMulticall.sol";
import { IFTAccessManaged } from "../../../../contracts/utils/IFTAccessManaged.sol";
import { EVMIFTSendCallConstructor } from "../../../../contracts/utils/EVMIFTSendCallConstructor.sol";
import { IBCRolesLib } from "../../../../contracts/utils/IBCRolesLib.sol";
import { ICS24Host } from "../../../../contracts/utils/ICS24Host.sol";
import { Address } from "@openzeppelin-contracts/utils/Address.sol";
import { Strings } from "@openzeppelin-contracts/utils/Strings.sol";
import { RateLimiter } from "@openzeppelin-contracts/utils/RateLimiter.sol";
import { ERC1967Proxy } from "@openzeppelin-contracts/proxy/ERC1967/ERC1967Proxy.sol";
import { DeployAccessManagerWithRoles } from "../../../../scripts/deployments/DeployAccessManagerWithRoles.sol";

/// @dev Relays to a router on chain A that only relayers may call, batching through `IBCMulticall`
abstract contract MulticallIntegrationSuite is LightClientDriverTest, DeployAccessManagerWithRoles {
    IbcImpl public ibcImplA;
    IbcImpl public ibcImplB;

    TestHelper public th = new TestHelper();
    IntegrationEnv public integrationEnv;

    IFTAccessManaged public iftOnA;
    IFTAccessManaged public iftOnB;
    IBCMulticall public multicall;
    address public relayer;

    uint208 public constant MAX_RATE_LIMIT_CAPACITY = type(uint208).max;
    uint48 public constant RATE_LIMIT_WINDOW = 1 days;

    function setUp() public {
        integrationEnv = new IntegrationEnv();

        ibcImplA = new IbcImpl(integrationEnv.permit2());
        ibcImplB = new IbcImpl(integrationEnv.permit2());

        ibcImplA.addClient(th.FIRST_CLIENT_ID(), _newDriver(ibcImplB.ics26Router()));
        ibcImplB.addClient(th.FIRST_CLIENT_ID(), _newDriver(ibcImplA.ics26Router()));

        iftOnA = _deployIFT(ibcImplA);
        iftOnB = _deployIFT(ibcImplB);
        EVMIFTSendCallConstructor sendCallConstructor = new EVMIFTSendCallConstructor();
        iftOnA.registerIFTBridge(
            th.FIRST_CLIENT_ID(), Strings.toChecksumHexString(address(iftOnB)), address(sendCallConstructor)
        );
        iftOnB.registerIFTBridge(
            th.FIRST_CLIENT_ID(), Strings.toChecksumHexString(address(iftOnA)), address(sendCallConstructor)
        );

        // Only relayers may relay on chain A. `ibcImplA` updates the client while building proofs, so it relays too.
        ibcImplA.accessManager()
            .setTargetFunctionRole(
                address(ibcImplA.ics26Router()), IBCRolesLib.ics26RelayerSelectors(), IBCRolesLib.RELAYER_ROLE
            );
        ibcImplA.accessManager().grantRole(IBCRolesLib.RELAYER_ROLE, address(ibcImplA), 0);

        IBCMulticall logic = new IBCMulticall();
        multicall = IBCMulticall(
            address(
                new ERC1967Proxy(
                    address(logic), abi.encodeCall(IBCMulticall.initialize, (address(ibcImplA.accessManager())))
                )
            )
        );
        accessManagerSetMulticallRoles(ibcImplA.accessManager(), address(multicall), false);

        relayer = makeAddr("relayer");
        ibcImplA.accessManager().grantRole(IBCRolesLib.RELAYER_ROLE, relayer, 0);
    }

    /// @dev Two refunds that only fit the rate limit one at a time: the first lands and the second stays retryable
    function testFuzz_success_partialTimeoutBatch(uint256 amount) public {
        amount = bound(amount, 1, MAX_RATE_LIMIT_CAPACITY / 2);
        address sender = integrationEnv.createUser();
        IICS26RouterMsgs.Packet[2] memory packets = _sendTimingOutTransfers(sender, amount);

        iftOnA.setIFTRateLimit(uint208(amount), RATE_LIMIT_WINDOW);
        IIBCMulticallMsgs.Call[] memory calls = _timeoutCalls(packets, true);

        vm.expectEmit(address(multicall));
        emit IIBCMulticall.CallFailed(1, abi.encodeWithSelector(RateLimiter.RateLimitExceeded.selector));
        vm.prank(relayer);
        IIBCMulticallMsgs.Result[] memory results = multicall.multicall(calls);

        assertTrue(results[0].success, "first timeout should succeed");
        assertFalse(results[1].success, "second timeout should fail");
        assertEq(results[1].returnData, abi.encodeWithSelector(RateLimiter.RateLimitExceeded.selector));
        assertEq(iftOnA.balanceOf(sender), amount, "only the first transfer should be refunded");
        _assertPendingTransferNotFound(packets[0]);
        _assertStillPending(packets[1], amount);

        // Once the refund fits, relaying the failed timeout again refunds the sender
        iftOnA.setIFTRateLimit(MAX_RATE_LIMIT_CAPACITY, RATE_LIMIT_WINDOW);
        IIBCMulticallMsgs.Call[] memory retry = new IIBCMulticallMsgs.Call[](1);
        retry[0] = calls[1];
        vm.prank(relayer);
        results = multicall.multicall(retry);

        assertTrue(results[0].success, "retried timeout should succeed");
        assertEq(iftOnA.balanceOf(sender), 2 * amount, "both transfers should be refunded");
        _assertPendingTransferNotFound(packets[1]);
    }

    /// @dev A failing call that is not allowed to fail reverts the whole batch with its own error
    function testFuzz_failure_strictCallRevertsBatch(uint256 amount) public {
        amount = bound(amount, 1, MAX_RATE_LIMIT_CAPACITY / 2);
        address sender = integrationEnv.createUser();
        IICS26RouterMsgs.Packet[2] memory packets = _sendTimingOutTransfers(sender, amount);

        iftOnA.setIFTRateLimit(uint208(amount), RATE_LIMIT_WINDOW);
        IIBCMulticallMsgs.Call[] memory calls = _timeoutCalls(packets, false);

        vm.expectRevert(RateLimiter.RateLimitExceeded.selector);
        vm.prank(relayer);
        multicall.multicall(calls);

        assertEq(iftOnA.balanceOf(sender), 0, "the first refund should be rolled back");
        _assertStillPending(packets[0], amount);
        _assertStillPending(packets[1], amount);
    }

    function testFuzz_success_recvBatch(uint256 amount) public {
        amount = bound(amount, 1, MAX_RATE_LIMIT_CAPACITY / 2);
        address sender = integrationEnv.createUser();
        address receiver = integrationEnv.createUser();
        string memory clientId = th.FIRST_CLIENT_ID();
        string memory receiverStr = Strings.toHexString(receiver);

        deal(address(iftOnB), sender, 2 * amount, true);
        IIBCMulticallMsgs.Call[] memory calls = new IIBCMulticallMsgs.Call[](2);
        for (uint256 i = 0; i < 2; ++i) {
            vm.recordLogs();
            vm.prank(sender);
            iftOnB.iftTransfer(clientId, receiverStr, amount);
            calls[i] = IIBCMulticallMsgs.Call({
                target: address(ibcImplA.ics26Router()),
                allowFailure: true,
                callData: abi.encodeCall(
                    IICS26RouterAccessControlled.recvPacket, (ibcImplA.recvPacketMsg(_extractPacketFromLogs()))
                )
            });
        }

        vm.prank(relayer);
        IIBCMulticallMsgs.Result[] memory results = multicall.multicall(calls);

        assertTrue(results[0].success && results[1].success, "both receives should succeed");
        assertEq(iftOnA.balanceOf(receiver), 2 * amount, "receiver should be minted both transfers");
    }

    function test_failure_unauthorizedCaller() public {
        address user = integrationEnv.createUser();
        IIBCMulticallMsgs.Call[] memory calls = new IIBCMulticallMsgs.Call[](0);

        vm.expectRevert(abi.encodeWithSelector(IAccessManaged.AccessManagedUnauthorized.selector, user));
        vm.prank(user);
        multicall.multicall(calls);
    }

    /// @dev Without `RELAYER_ROLE` the multicall contract cannot relay to the router
    function test_failure_multicallWithoutRelayerRole() public {
        address sender = integrationEnv.createUser();
        IICS26RouterMsgs.Packet[2] memory packets = _sendTimingOutTransfers(sender, 1);
        IIBCMulticallMsgs.Call[] memory calls = _timeoutCalls(packets, true);
        ibcImplA.accessManager().revokeRole(IBCRolesLib.RELAYER_ROLE, address(multicall));

        bytes memory unauthorized =
            abi.encodeWithSelector(IAccessManaged.AccessManagedUnauthorized.selector, address(multicall));

        vm.prank(relayer);
        IIBCMulticallMsgs.Result[] memory results = multicall.multicall(calls);
        assertFalse(results[0].success || results[1].success, "no timeout should succeed");
        assertEq(results[0].returnData, unauthorized);

        calls[0].allowFailure = false;
        vm.expectRevert(unauthorized);
        vm.prank(relayer);
        multicall.multicall(calls);
    }

    function test_failure_targetWithoutCode() public {
        address noCode = makeAddr("noCode");
        bytes memory emptyCode = abi.encodeWithSelector(Address.AddressEmptyCode.selector, noCode);
        IIBCMulticallMsgs.Call[] memory calls = new IIBCMulticallMsgs.Call[](1);
        calls[0] = IIBCMulticallMsgs.Call({ target: noCode, allowFailure: true, callData: "" });

        vm.expectEmit(address(multicall));
        emit IIBCMulticall.CallFailed(0, emptyCode);
        vm.prank(relayer);
        IIBCMulticallMsgs.Result[] memory results = multicall.multicall(calls);
        assertFalse(results[0].success, "call without code should fail");
        assertEq(results[0].returnData, emptyCode);

        calls[0].allowFailure = false;
        vm.expectRevert(emptyCode);
        vm.prank(relayer);
        multicall.multicall(calls);
    }

    function test_upgrade() public {
        IBCMulticall newLogic = new IBCMulticall();

        vm.expectRevert(abi.encodeWithSelector(IAccessManaged.AccessManagedUnauthorized.selector, relayer));
        vm.prank(relayer);
        multicall.upgradeToAndCall(address(newLogic), "");

        multicall.upgradeToAndCall(address(newLogic), "");
        assertEq(multicall.authority(), address(ibcImplA.accessManager()), "authority should be preserved");
    }

    function _deployIFT(IbcImpl ibcImpl) private returns (IFTAccessManaged ift) {
        ift = IFTAccessManaged(
            address(
                new ERC1967Proxy(
                    address(new IFTAccessManaged()),
                    abi.encodeCall(
                        IFTAccessManaged.initialize,
                        (address(ibcImpl.accessManager()), "Test IFT", "TIFT", address(ibcImpl.ics27Gmp()))
                    )
                )
            )
        );
        bytes4[] memory selectors = new bytes4[](2);
        selectors[0] = IIFT.registerIFTBridge.selector;
        selectors[1] = IIFTRateLimit.setIFTRateLimit.selector;
        ibcImpl.accessManager().setTargetFunctionRole(address(ift), selectors, IBCRolesLib.ADMIN_ROLE);
        ift.setIFTRateLimit(MAX_RATE_LIMIT_CAPACITY, RATE_LIMIT_WINDOW);
    }

    /// @dev Sends two transfers of `amount` from chain A and lets them time out
    function _sendTimingOutTransfers(
        address sender,
        uint256 amount
    )
        private
        returns (IICS26RouterMsgs.Packet[2] memory packets)
    {
        string memory clientId = th.FIRST_CLIENT_ID();
        string memory receiverStr = Strings.toHexString(integrationEnv.createUser());
        uint64 timeout = uint64(block.timestamp) + 1;

        deal(address(iftOnA), sender, 2 * amount, true);
        for (uint256 i = 0; i < 2; ++i) {
            vm.recordLogs();
            vm.prank(sender);
            iftOnA.iftTransfer(clientId, receiverStr, amount, timeout);
            packets[i] = _extractPacketFromLogs();
        }
        vm.warp(block.timestamp + 100);
    }

    function _timeoutCalls(
        IICS26RouterMsgs.Packet[2] memory packets,
        bool allowFailure
    )
        private
        returns (IIBCMulticallMsgs.Call[] memory calls)
    {
        calls = new IIBCMulticallMsgs.Call[](2);
        for (uint256 i = 0; i < 2; ++i) {
            calls[i] = IIBCMulticallMsgs.Call({
                target: address(ibcImplA.ics26Router()),
                // Only the second call may fail, the first one always fits the rate limit
                allowFailure: i == 0 || allowFailure,
                callData: abi.encodeCall(
                    IICS26RouterAccessControlled.timeoutPacket, (ibcImplA.timeoutPacketMsg(packets[i]))
                )
            });
        }
    }

    function _assertStillPending(IICS26RouterMsgs.Packet memory packet, uint256 amount) private view {
        bytes32 path = ICS24Host.packetCommitmentKeyCalldata(packet.sourceClient, packet.sequence);
        assertEq(
            ibcImplA.ics26Router().getCommitment(path),
            ICS24Host.packetCommitmentBytes32(packet),
            "packet commitment should be kept"
        );
        IIFTMsgs.PendingTransfer memory pending = iftOnA.getPendingTransfer(packet.sourceClient, packet.sequence);
        assertEq(pending.amount, amount, "transfer should stay pending");
    }

    function _assertPendingTransferNotFound(IICS26RouterMsgs.Packet memory packet) private {
        vm.expectRevert(
            abi.encodeWithSelector(IIFTErrors.IFTPendingTransferNotFound.selector, packet.sourceClient, packet.sequence)
        );
        iftOnA.getPendingTransfer(packet.sourceClient, packet.sequence);
    }

    function _extractPacketFromLogs() private view returns (IICS26RouterMsgs.Packet memory) {
        bytes memory packetBz = th.getValueFromEvent(IICS26Router.SendPacket.selector);
        return abi.decode(packetBz, (IICS26RouterMsgs.Packet));
    }
}
