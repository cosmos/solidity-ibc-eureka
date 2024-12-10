// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import "forge-std/Test.sol";
import "../../contracts/utils/safe-global/safe-contracts/contracts/Safe.sol";
import "../../contracts/utils/safe-global/safe-contracts/contracts/proxies/SafeProxyFactory.sol";
import "../../contracts/utils/safe-global/safe-contracts/contracts/common/Enum.sol";

contract MultisigTest is Test {
    Safe safeSingleton;
    SafeProxyFactory proxyFactory;
    Safe safeProxy; // Reused proxy instance
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
    }

    function testSafeInitialization() public {
        // Verify the proxy is initialized
        address[] memory actualOwners = safeProxy.getOwners();
        for (uint256 i = 0; i < owners.length; i++) {
            assertEq(actualOwners[i], owners[i], string(abi.encodePacked("Owner mismatch at index ", i)));
        }
        assertEq(safeProxy.getThreshold(), threshold, "Threshold mismatch");

        // Verify the owners of the Safe
        assertEq(actualOwners.length, owners.length, "Number of owners mismatch");

        for (uint256 i = 0; i < owners.length; i++) {
            emit log_named_address(string(abi.encodePacked("Expected Owner ", i)), owners[i]);
            emit log_named_address(string(abi.encodePacked("Actual Owner ", i)), actualOwners[i]);
            assertEq(actualOwners[i], owners[i], string(abi.encodePacked("Owner mismatch at index ", i)));
        }
    }

    function testExecuteTransaction() public {
        // Verify proxy setup
        address[] memory actualOwners = safeProxy.getOwners();
        uint256 proxyThreshold = safeProxy.getThreshold();
        assertEq(actualOwners.length, owners.length, "Proxy owners mismatch");
        assertEq(proxyThreshold, threshold, "Proxy threshold mismatch");

        // Fund the Safe proxy for testing
        address to = address(vm.addr(100)); // Destination address
        uint256 value = 1 ether;
        vm.deal(address(safeProxy), value);

        // Create the transaction hash
        bytes32 txHash = safeProxy.getTransactionHash(
            to,
            value,
            "",
            Enum.Operation.Call,
            0, // SafeTxGas
            0, // BaseGas
            0, // GasPrice
            address(0), // GasToken
            payable(address(0)), // RefundReceiver
            safeProxy.nonce()
        );

        emit log_bytes32(txHash);

        // Generate valid signatures
        bytes memory signature;
        for (uint256 i = 0; i < threshold; i++) {
            uint256 privateKey = uint256(keccak256(abi.encodePacked(i))); // Generate private key deterministically
            (uint8 v, bytes32 r, bytes32 s) = vm.sign(privateKey, txHash);

            // Verify recovered owner matches expected owner
            address recoveredOwner = ecrecover(txHash, v, r, s);
            emit log_named_address("Expected Owner", owners[i]);
            emit log_named_address("Recovered Owner", recoveredOwner);
            assertEq(recoveredOwner, owners[i], "Recovered owner mismatch");

            // Append signature
            signature = abi.encodePacked(signature, r, s, v);
        }

        emit log_bytes(signature);

        // Execute the transaction
        vm.prank(owners[0]); // Simulate execution by the first owner
        bool success = safeProxy.execTransaction(
            to,
            value,
            "",
            Enum.Operation.Call,
            0, // SafeTxGas
            0, // BaseGas
            0, // GasPrice
            address(0), // GasToken
            payable(address(0)), // RefundReceiver
            signature
        );

        // Verify the transaction succeeded
        assertTrue(success, "Transaction execution failed.");
        assertEq(to.balance, value, "Transaction value not transferred.");
    }
}
