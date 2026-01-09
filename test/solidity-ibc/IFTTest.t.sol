// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

// solhint-disable custom-errors,max-line-length,no-inline-assembly,gas-small-strings,function-max-lines

import { Test } from "forge-std/Test.sol";

import { IIFTMsgs } from "../../contracts/msgs/IIFTMsgs.sol";

import { IIFT } from "../../contracts/interfaces/IIFT.sol";
import { IAccessManaged } from "@openzeppelin-contracts/access/manager/IAccessManaged.sol";
import { IIFTErrors } from "../../contracts/errors/IIFTErrors.sol";
import { IICS27GMP } from "../../contracts/interfaces/IICS27GMP.sol";

import { IFTOwnable } from "../../contracts/utils/IFTOwnable.sol";
import { IFTAccessManaged } from "../../contracts/utils/IFTAccessManaged.sol";
import { EVMIFTSendCallConstructor } from "../../contracts/utils/EVMIFTSendCallConstructor.sol";
import { ICS24Host } from "../../contracts/utils/ICS24Host.sol";
import { Strings } from "@openzeppelin-contracts/utils/Strings.sol";
import { TestHelper } from "./utils/TestHelper.sol";
import { ICS27Lib } from "../../contracts/utils/ICS27Lib.sol";
import { ERC1967Proxy } from "@openzeppelin-contracts/proxy/ERC1967/ERC1967Proxy.sol";
import { AccessManager } from "@openzeppelin-contracts/access/manager/AccessManager.sol";
import { OwnableUpgradeable } from "@openzeppelin-upgradeable/access/OwnableUpgradeable.sol";

contract IFTTest is Test {
    // solhint-disable gas-indexed-events
    IIFT public ift;

    EVMIFTSendCallConstructor public evmCallConstructor = new EVMIFTSendCallConstructor();
    TestHelper public th = new TestHelper();

    string public constant TOKEN_NAME = "Test IFT";
    string public constant TOKEN_SYMBOL = "TIFT";

    address public mockICS27 = makeAddr("mockICS27");
    // admin is the owner of the IFTOwnable and authority of the access manager
    address public admin = makeAddr("admin");

    function setUpOwnable() public {
        address impl = address(new IFTOwnable());
        ERC1967Proxy proxy = new ERC1967Proxy(
            impl, abi.encodeCall(IFTOwnable.initialize, (admin, TOKEN_NAME, TOKEN_SYMBOL, mockICS27))
        );
        ift = IIFT(address(proxy));
    }

    function setUpAccessManaged() public {
        address impl = address(new IFTAccessManaged());
        AccessManager manager = new AccessManager(admin);
        ERC1967Proxy proxy = new ERC1967Proxy(
            impl, abi.encodeCall(IFTAccessManaged.initialize, (address(manager), TOKEN_NAME, TOKEN_SYMBOL, mockICS27))
        );
        ift = IIFT(address(proxy));
    }

    function fixtureregisterBridgeTC() public returns (RegisterIFTBridgeTestCase[] memory) {
        address unauthorized = makeAddr("unauthorized");

        RegisterIFTBridgeTestCase[] memory testCases = new RegisterIFTBridgeTestCase[](7);

        testCases[0] = RegisterIFTBridgeTestCase({
            name: "success: ownable admin registers",
            caller: admin,
            ownable: true,
            clientId: th.FIRST_CLIENT_ID(),
            counterpartyIFT: "0x123",
            iftSendCallConstructor: address(evmCallConstructor),
            expectedRevert: ""
        });
        testCases[1] = RegisterIFTBridgeTestCase({
            name: "success: access managed admin registers",
            caller: admin,
            ownable: false,
            clientId: th.FIRST_CLIENT_ID(),
            counterpartyIFT: "0x123",
            iftSendCallConstructor: address(evmCallConstructor),
            expectedRevert: ""
        });
        testCases[2] = RegisterIFTBridgeTestCase({
            name: "revert: ownable unauthorized caller",
            caller: unauthorized,
            ownable: true,
            clientId: th.FIRST_CLIENT_ID(),
            counterpartyIFT: "0x123",
            iftSendCallConstructor: address(evmCallConstructor),
            expectedRevert: abi.encodeWithSelector(OwnableUpgradeable.OwnableUnauthorizedAccount.selector, unauthorized)
        });
        testCases[3] = RegisterIFTBridgeTestCase({
            name: "revert: access managed unauthorized caller",
            caller: makeAddr("unauthorized"),
            ownable: false,
            clientId: th.FIRST_CLIENT_ID(),
            counterpartyIFT: "0x123",
            iftSendCallConstructor: address(evmCallConstructor),
            expectedRevert: abi.encodeWithSelector(IAccessManaged.AccessManagedUnauthorized.selector, unauthorized)
        });
        testCases[4] = RegisterIFTBridgeTestCase({
            name: "revert: empty clientId",
            caller: admin,
            ownable: true,
            clientId: "",
            counterpartyIFT: "0x123",
            iftSendCallConstructor: address(evmCallConstructor),
            expectedRevert: abi.encodeWithSelector(IIFTErrors.IFTEmptyClientId.selector)
        });
        testCases[5] = RegisterIFTBridgeTestCase({
            name: "revert: empty counterparty IFT address",
            caller: admin,
            ownable: true,
            clientId: th.FIRST_CLIENT_ID(),
            counterpartyIFT: "",
            iftSendCallConstructor: address(evmCallConstructor),
            expectedRevert: abi.encodeWithSelector(IIFTErrors.IFTEmptyCounterpartyAddress.selector)
        });
        testCases[6] = RegisterIFTBridgeTestCase({
            name: "revert: empty iftSendCallConstructor address",
            caller: admin,
            ownable: true,
            clientId: th.FIRST_CLIENT_ID(),
            counterpartyIFT: "0x123",
            iftSendCallConstructor: address(0),
            expectedRevert: abi.encodeWithSelector(IIFTErrors.IFTZeroAddressConstructor.selector)
        });

        return testCases;
    }

    function tableRegisterIFTBridgeTest(RegisterIFTBridgeTestCase memory registerBridgeTC) public {
        if (registerBridgeTC.ownable) {
            setUpOwnable();
        } else {
            setUpAccessManaged();
        }
        vm.startPrank(registerBridgeTC.caller);

        if (registerBridgeTC.expectedRevert.length != 0) {
            vm.expectRevert(registerBridgeTC.expectedRevert);
        } else {
            vm.expectEmit(true, true, true, true);
            emit IIFT.IFTBridgeRegistered(
                registerBridgeTC.clientId, registerBridgeTC.counterpartyIFT, registerBridgeTC.iftSendCallConstructor
            );
        }

        ift.registerIFTBridge(
            registerBridgeTC.clientId, registerBridgeTC.counterpartyIFT, registerBridgeTC.iftSendCallConstructor
        );

        if (registerBridgeTC.expectedRevert.length != 0) {
            return;
        }

        IIFTMsgs.IFTBridge memory bridge = ift.getIFTBridge(registerBridgeTC.clientId);
        assertEq(bridge.clientId, registerBridgeTC.clientId);
        assertEq(bridge.counterpartyIFTAddress, registerBridgeTC.counterpartyIFT);
        assertEq(address(bridge.iftSendCallConstructor), address(evmCallConstructor));
    }

    function fixtureRemoveBridgeTC() public returns (RemoveIFTBridgeTestCase[] memory) {
        address unauthorized = makeAddr("unauthorized");

        RemoveIFTBridgeTestCase[] memory testCases = new RemoveIFTBridgeTestCase[](5);

        testCases[0] = RemoveIFTBridgeTestCase({
            name: "success: ownable admin removes",
            caller: admin,
            ownable: true,
            clientId: th.FIRST_CLIENT_ID(),
            expectedRevert: ""
        });
        testCases[1] = RemoveIFTBridgeTestCase({
            name: "success: access managed admin removes",
            caller: admin,
            ownable: false,
            clientId: th.FIRST_CLIENT_ID(),
            expectedRevert: ""
        });
        testCases[2] = RemoveIFTBridgeTestCase({
            name: "revert: ownable unauthorized caller",
            caller: unauthorized,
            ownable: true,
            clientId: th.FIRST_CLIENT_ID(),
            expectedRevert: abi.encodeWithSelector(OwnableUpgradeable.OwnableUnauthorizedAccount.selector, unauthorized)
        });
        testCases[3] = RemoveIFTBridgeTestCase({
            name: "revert: access managed unauthorized caller",
            caller: unauthorized,
            ownable: false,
            clientId: th.FIRST_CLIENT_ID(),
            expectedRevert: abi.encodeWithSelector(IAccessManaged.AccessManagedUnauthorized.selector, unauthorized)
        });
        testCases[4] = RemoveIFTBridgeTestCase({
            name: "revert: clientId not registered",
            caller: admin,
            ownable: true,
            clientId: th.INVALID_ID(),
            expectedRevert: abi.encodeWithSelector(IIFTErrors.IFTBridgeNotFound.selector, th.INVALID_ID())
        });

        return testCases;
    }

    function tableRemoveIFTBridgeTest(RemoveIFTBridgeTestCase memory removeBridgeTC) public {
        if (removeBridgeTC.ownable) {
            setUpOwnable();
        } else {
            setUpAccessManaged();
        }
        // First register the bridge
        vm.startPrank(admin);
        ift.registerIFTBridge(
            th.FIRST_CLIENT_ID(), "0x123", address(evmCallConstructor)
        );
        vm.stopPrank();

        // Now attempt to remove the bridge
        vm.startPrank(removeBridgeTC.caller);

        if (removeBridgeTC.expectedRevert.length != 0) {
            vm.expectRevert(removeBridgeTC.expectedRevert);
        } else {
            vm.expectEmit(true, true, true, true);
            emit IIFT.IFTBridgeRemoved(removeBridgeTC.clientId);
        }

        ift.removeIFTBridge(removeBridgeTC.clientId);

        if (removeBridgeTC.expectedRevert.length != 0) {
            return;
        }

        vm.expectRevert(abi.encodeWithSelector(IIFTErrors.IFTBridgeNotFound.selector, removeBridgeTC.clientId));
        ift.getIFTBridge(removeBridgeTC.clientId);
    }

    function tableIFTTransferTest(IFTTransferTestCase memory transferTC) public {
        if (transferTC.ownable) {
            setUpOwnable();
        } else {
            setUpAccessManaged();
        }

        // First register the bridge
        vm.startPrank(admin);
        ift.registerIFTBridge(
            transferTC.clientId, "0x123", address(evmCallConstructor)
        );
        vm.stopPrank();

        // random sequence number
        uint64 seq = uint64(vm.randomUint(1, type(uint64).max));
        vm.mockCall(
            address(mockICS27),
            IICS27GMP.sendCall.selector,
            abi.encode(seq)
        );

        // Mint some tokens to the caller
        uint256 initialBalance = 1_000_000 ether;
        vm.deal(transferTC.caller, initialBalance);

        if (transferTC.expectedRevert.length != 0) {
            vm.expectRevert(transferTC.expectedRevert);
        } else {
            vm.expectEmit(true, true, true, true);
            emit IIFT.IFTTransferInitiated(
                transferTC.clientId,
                seq,
                transferTC.caller,
                transferTC.receiver,
                transferTC.amount
            );
        }

        vm.startPrank(transferTC.caller);
        ift.iftTransfer(
            transferTC.clientId,
            transferTC.receiver,
            transferTC.amount,
            transferTC.timeoutTimestamp
        );
        vm.stopPrank();

        if (transferTC.expectedRevert.length != 0) {
            return;
        }

        IIFTMsgs.PendingTransfer memory pending = ift.getPendingTransfer(transferTC.clientId, seq);
        assertEq(pending.sender, transferTC.caller);
        assertEq(pending.amount, transferTC.amount);
    }

    struct IFTTransferTestCase {
        string name;
        address caller;
        bool ownable;
        string clientId;
        string receiver;
        uint256 amount;
        uint64 timeoutTimestamp;
        bytes expectedRevert;
    }

    struct RemoveIFTBridgeTestCase {
        string name;
        address caller;
        bool ownable;
        string clientId;
        bytes expectedRevert;
    }

    struct RegisterIFTBridgeTestCase {
        string name;
        address caller;
        bool ownable;
        string clientId;
        address iftSendCallConstructor;
        string counterpartyIFT;
        bytes expectedRevert;
    }
}
