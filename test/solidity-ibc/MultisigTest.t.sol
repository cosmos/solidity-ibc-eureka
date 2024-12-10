// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import "forge-std/Test.sol";
import "../../contracts/utils/safe-global/safe-contracts/contracts/Safe.sol";
import "../../contracts/utils/safe-global/safe-contracts/contracts/proxies/SafeProxyFactory.sol";

contract MultisigTest is Test {
    Safe safeSingleton;
    SafeProxyFactory proxyFactory;

    function setUp() public {
        // Attempt to deploy Gnosis Safe Singleton contract
        safeSingleton = new Safe();
        emit log("Gnosis Safe Singleton deployed successfully.");

        // Attempt to deploy Gnosis Safe Proxy Factory contract
        proxyFactory = new SafeProxyFactory();
        emit log("Gnosis Safe Proxy Factory deployed successfully.");
    }

    function testSafeContractsAvailable() public {
        // Assert the Safe Singleton contract is deployed
        assertTrue(address(safeSingleton) != address(0), "Safe Singleton deployment failed.");

        // Assert the Proxy Factory contract is deployed
        assertTrue(address(proxyFactory) != address(0), "Proxy Factory deployment failed.");
    }
}
