// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import { Test } from "forge-std/Test.sol"; 
import { Safe } from "../../contracts/utils/safe-global/safe-contracts/contracts/Safe.sol";
import { SafeProxyFactory } from "../../contracts/utils/safe-global/safe-contracts/contracts/proxies/SafeProxyFactory.sol";
import { TransparentUpgradeableProxy } from "@openzeppelin/proxy/transparent/TransparentUpgradeableProxy.sol";
import { ITransparentUpgradeableProxy } from "@openzeppelin/proxy/transparent/TransparentUpgradeableProxy.sol";
import { Enum } from "../../contracts/utils/safe-global/safe-contracts/contracts/common/Enum.sol";
import { ICS26Router } from "../../contracts/ICS26Router.sol";
import { ICS20Transfer } from "../../contracts/ICS20Transfer.sol";
import { ICSCore } from "../../contracts/ICSCore.sol";

contract MultisigWithIBCContractsTest is Test {
    Safe safeSingleton;
    SafeProxyFactory proxyFactory;
    Safe safeProxy; // Multisig proxy instance
    TransparentUpgradeableProxy ics26RouterProxy;
    TransparentUpgradeableProxy ics20TransferProxy;
    TransparentUpgradeableProxy icsCoreProxy;
    ICS26Router ics26Router;
    ICS20Transfer ics20Transfer;
    ICSCore icsCore;

    address[] owners;
    uint256 threshold;

function setUp() public {
        // Deploy Gnosis Safe Singleton contract
        safeSingleton = new Safe();
        emit log("Safe Singleton deployed successfully.");

        // Deploy Gnosis Safe Proxy Factory contract
        proxyFactory = new SafeProxyFactory();
        emit log("Safe Proxy Factory deployed successfully.");

        // Generate deterministic test addresses
        for (uint256 i = 0; i < 3; i++) {
            address owner = vm.addr(uint256(keccak256(abi.encodePacked(i))));
            owners.push(owner);
            emit log_named_address("Owner", owner);
        }

        // Set the threshold
        threshold = 2; // Two signatures required to execute a transaction

        // Deploy and initialize the Safe Proxy
        bytes memory initializer = abi.encodeWithSelector(
            Safe.setup.selector,
            owners,
            threshold,
            address(0), // Fallback handler
            "",
            address(0), // Payment receiver
            0,          // Payment amount
            address(0)  // Payment token
        );

        safeProxy = Safe(
            payable(proxyFactory.createProxyWithNonce(address(safeSingleton), initializer, 0))
        );
        emit log_named_address("Safe Proxy Address", address(safeProxy));

        // Step 1: Deploy Logic Contracts
        ICS26Router ics26RouterLogic = new ICS26Router(address(safeProxy));
        emit log_named_address("ICS26Router Logic Address", address(ics26RouterLogic));

        ICS20Transfer ics20TransferLogic = new ICS20Transfer(address(safeProxy));
        emit log_named_address("ICS20Transfer Logic Address", address(ics20TransferLogic));

        ICSCore icsCoreLogic = new ICSCore(address(safeProxy));
        emit log_named_address("ICSCore Logic Address", address(icsCoreLogic));

        // Step 2: Deploy Transparent Proxies
        bytes memory routerInitData = abi.encodeWithSelector(
            ICS26Router.initialize.selector,
            address(safeProxy)
        );
        ics26RouterProxy = new TransparentUpgradeableProxy(
            address(ics26RouterLogic),
            address(safeProxy), // Safe multisig as the admin
            routerInitData // Initialize during deployment
        );
        emit log_named_address("ICS26Router Proxy Address", address(ics26RouterProxy));

        bytes memory transferInitData = abi.encodeWithSelector(
            ICS20Transfer.initialize.selector,
            address(safeProxy)
        );
        ics20TransferProxy = new TransparentUpgradeableProxy(
            address(ics20TransferLogic),
            address(safeProxy), // Safe multisig as the admin
            transferInitData // Initialize during deployment
        );
        emit log_named_address("ICS20Transfer Proxy Address", address(ics20TransferProxy));

        bytes memory coreInitData = abi.encodeWithSelector(
            ICSCore.initialize.selector,
            address(safeProxy)
        );
        icsCoreProxy = new TransparentUpgradeableProxy(
            address(icsCoreLogic),
            address(safeProxy), // Safe multisig as the admin
            coreInitData // Initialize during deployment
        );
        emit log_named_address("ICSCore Proxy Address", address(icsCoreProxy));

        // Step 3: Assign Proxies to Interfaces
        ics26Router = ICS26Router(address(ics26RouterProxy));
        emit log("ICS26Router initialized successfully.");

        ics20Transfer = ICS20Transfer(address(ics20TransferProxy));
        emit log("ICS20Transfer initialized successfully.");

        icsCore = ICSCore(address(icsCoreProxy));
        emit log("ICSCore initialized successfully.");
    }

    function testOwnership() public {
        // Verify multisig is the owner of ICS26Router
        assertEq(ics26Router.owner(), address(safeProxy), "ICS26Router not owned by multisig");

        // Verify multisig is the owner of ICS20Transfer
        assertEq(ics20Transfer.owner(), address(safeProxy), "ICS20Transfer not owned by multisig");

        // Verify multisig is the owner of ICSCore
        assertEq(icsCore.owner(), address(safeProxy), "ICSCore not owned by multisig");
    }

    function testExecuteAddIBCApp() public {
        // Generate transaction to add IBC App from multisig
        bytes memory data = abi.encodeWithSelector(
            ICS26Router.addIBCApp.selector,
            "transfer", // Port ID
            address(ics20Transfer)
        );

        // Create the transaction hash
        bytes32 txHash = safeProxy.getTransactionHash(
            address(ics26Router),
            0, // value
            data,
            Enum.Operation.Call,
            0, // SafeTxGas
            0, // BaseGas
            0, // GasPrice
            address(0), // GasToken
            payable(address(0)), // RefundReceiver
            safeProxy.nonce()
        );

        // Generate valid signatures
        bytes memory signature;
        for (uint256 i = 0; i < threshold; i++) {
            uint256 privateKey = uint256(keccak256(abi.encodePacked(i))); // Generate private key deterministically
            (uint8 v, bytes32 r, bytes32 s) = vm.sign(privateKey, txHash);

            // Append signature
            signature = abi.encodePacked(signature, r, s, v);
        }

        // Execute the transaction
        vm.prank(owners[0]); // Simulate execution by one of the multisig owners
        bool success = safeProxy.execTransaction(
            address(ics26Router),
            0, // value
            data,
            Enum.Operation.Call,
            0, // SafeTxGas
            0, // BaseGas
            0, // GasPrice
            address(0), // GasToken
            payable(address(0)), // RefundReceiver
            signature
        );

        assertTrue(success, "Transaction execution failed");

        // Verify the IBC app is added
        assertEq(address(ics26Router.getIBCApp("transfer")), address(ics20Transfer), "IBC App not added correctly");
    }

    function testFailExecuteAddIBCAppWithInsufficientSignatures() public {
        // Generate transaction to add IBC App from multisig
        bytes memory data = abi.encodeWithSelector(
            ICS26Router.addIBCApp.selector,
            "transfer", // Port ID
            address(ics20Transfer)
        );

        // Create the transaction hash
        bytes32 txHash = safeProxy.getTransactionHash(
            address(ics26Router),
            0, // value
            data,
            Enum.Operation.Call,
            0, // SafeTxGas
            0, // BaseGas
            0, // GasPrice
            address(0), // GasToken
            payable(address(0)), // RefundReceiver
            safeProxy.nonce()
        );

        // Generate signatures, but provide fewer than the required threshold
        bytes memory signature;
        for (uint256 i = 0; i < threshold - 1; i++) { // Less than threshold
            uint256 privateKey = uint256(keccak256(abi.encodePacked(i))); // Generate private key deterministically
            (uint8 v, bytes32 r, bytes32 s) = vm.sign(privateKey, txHash);

            // Append signature
            signature = abi.encodePacked(signature, r, s, v);
        }

        // Attempt to execute the transaction
        vm.prank(owners[0]); // Simulate execution by one of the multisig owners
        bool success = safeProxy.execTransaction(
            address(ics26Router),
            0, // value
            data,
            Enum.Operation.Call,
            0, // SafeTxGas
            0, // BaseGas
            0, // GasPrice
            address(0), // GasToken
            payable(address(0)), // RefundReceiver
            signature
        );

        // The transaction should fail
        assertFalse(success, "Transaction unexpectedly succeeded with insufficient signatures");
    }

function testFailUpgradeTransferLogicNotEnoughSignatures() public {
    // Deploy new ICS20Transfer logic
    ICS20Transfer newICS20TransferLogic = new ICS20Transfer(address(safeProxy));
    emit log_named_address("New Deployed ICS20Transfer Logic Address", address(newICS20TransferLogic));

    // Generate transaction to upgrade the proxy
    bytes memory data = abi.encodeWithSelector(
        ITransparentUpgradeableProxy.upgradeToAndCall.selector,
        address(newICS20TransferLogic),
        "" // No initialization data
    );

    // Create the transaction hash
    bytes32 txHash = safeProxy.getTransactionHash(
        address(ics20TransferProxy),
        0, // value
        data,
        Enum.Operation.Call,
        0, // SafeTxGas
        0, // BaseGas
        0, // GasPrice
        address(0), // GasToken
        payable(address(0)), // RefundReceiver
        safeProxy.nonce()
    );

    // Generate signatures, but provide fewer than the required threshold
    bytes memory signature;
    for (uint256 i = 0; i < threshold - 1; i++) { // Less than threshold
        uint256 privateKey = uint256(keccak256(abi.encodePacked(i))); // Generate private key deterministically
        (uint8 v, bytes32 r, bytes32 s) = vm.sign(privateKey, txHash);

        // Append signature
        signature = abi.encodePacked(signature, r, s, v);
    }

    
    // Attempt to execute the transaction
    vm.prank(owners[0]); // Simulate execution by one of the multisig owners
    bool success = safeProxy.execTransaction(
        address(ics20TransferProxy),
        0, // value
        data,
        Enum.Operation.Call,
        0, // SafeTxGas
        0, // BaseGas
        0, // GasPrice
        address(0), // GasToken
        payable(address(0)), // RefundReceiver
        signature
    );
    // The transaction should fail
    assertFalse(success, "Upgrade transaction unexpectedly succeeded with insufficient signatures");
}


    function testSuccessUpgradeAllLogicContracts() public {
        // Deploy new logic contracts
        ICS26Router newICS26RouterLogic = new ICS26Router(address(safeProxy));
        emit log_named_address("New ICS26Router Logic Address", address(newICS26RouterLogic));

        ICS20Transfer newICS20TransferLogic = new ICS20Transfer(address(safeProxy));
        emit log_named_address("New ICS20Transfer Logic Address", address(newICS20TransferLogic));

        ICSCore newICSCoreLogic = new ICSCore(address(safeProxy));
        emit log_named_address("New ICSCore Logic Address", address(newICSCoreLogic));

        // Generate and execute upgrade transactions for each proxy
        address[] memory proxies = new address[](3);
        proxies[0] = address(ics26RouterProxy);
        proxies[1] = address(ics20TransferProxy);
        proxies[2] = address(icsCoreProxy);

        address[] memory newLogics = new address[](3);
        newLogics[0] = address(newICS26RouterLogic);
        newLogics[1] = address(newICS20TransferLogic);
        newLogics[2] = address(newICSCoreLogic);

        for (uint256 i = 0; i < proxies.length; i++) {
            bytes memory data = abi.encodeWithSelector(
                ITransparentUpgradeableProxy.upgradeToAndCall.selector,
                newLogics[i]
            );

            bytes32 txHash = safeProxy.getTransactionHash(
                proxies[i],
                0, // Value
                data,
                Enum.Operation.DelegateCall, // Call operation
                0, // SafeTxGas
                0, // BaseGas
                0 gwei, // GasPrice
                address(0), // GasToken
                payable(address(0)), // RefundReceiver
                safeProxy.nonce()
            );

            bytes memory signature;
            for (uint256 j = 0; j < threshold; j++) {
                uint256 privateKey = uint256(keccak256(abi.encodePacked(j)));
                (uint8 v, bytes32 r, bytes32 s) = vm.sign(privateKey, txHash);
                signature = abi.encodePacked(signature, r, s, v);
            }

            vm.prank(owners[0]);
            bool success = safeProxy.execTransaction(
                proxies[i],
                0, // Value
                data,
                Enum.Operation.DelegateCall, // Call operation
                0, // SafeTxGas
                0, // BaseGas
                0 gwei, // GasPrice
                address(0), // GasToken
                payable(address(0)), // RefundReceiver
                signature
            );

            emit log_named_uint("Upgrade Success", success ? 1 : 0);
            require(success, "Upgrade transaction failed");
        }
    }

}