// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.28;

import { Vm } from "forge-std/Vm.sol";
import { stdJson } from "forge-std/StdJson.sol";

abstract contract Deployments {
    using stdJson for string;

    string internal constant DEPLOYMENT_DIR = "/deployments/";

    struct ProxiedICS26RouterDeployment {
        address implementation;
        address proxy;
        address timeLockAdmin;
        address portCustomizer;
        address clientIdCustomizer;
        address[] relayers;
    }

    function loadProxiedICS26RouterDeployment(
        Vm vm,
        string memory json
    )
    public
    pure
    returns (ProxiedICS26RouterDeployment memory)
    {
        ProxiedICS26RouterDeployment memory fixture = ProxiedICS26RouterDeployment({
            implementation: vm.parseJsonAddress(json, ".ics26Router.implementation"),
            proxy: vm.parseJsonAddress(json, ".ics26Router.proxy"),
            timeLockAdmin: vm.parseJsonAddress(json, ".ics26Router.timeLockAdmin"),
            portCustomizer: vm.parseJsonAddress(json, ".ics26Router.portCustomizer"),
            clientIdCustomizer: vm.parseJsonAddress(json, ".ics26Router.clientIdCustomizer"),
            relayers: vm.parseJsonAddressArray(json, ".ics26Router.relayers")
        });

        return fixture;
    }

    struct ProxiedICS20TransferDeployment {
        // transparant proxies
        address ics26Router;

        // implementation addresses
        address implementation;
        address escrowImplementation;
        address ibcERC20Implementation;

        // admin control
        address[] pausers;
        address[] unpausers;
        address tokenOperator;
        address permit2;
        address proxy;
    }

    // TODO: Move these to ops repo
    function loadProxiedICS20TransferDeployment(
        Vm vm,
        string memory json
    )
    public
    pure
    returns (ProxiedICS20TransferDeployment memory)
    {
        // abi.decode(vm.parseJson(json, ".ics20Transfer"), (ProxiedICS20TransferDeployment));
        ProxiedICS20TransferDeployment memory fixture = ProxiedICS20TransferDeployment({
            escrowImplementation: vm.parseJsonAddress(json, ".ics20Transfer.escrowImplementation"),
            ibcERC20Implementation: vm.parseJsonAddress(json, ".ics20Transfer.ibcERC20Implementation"),
            ics26Router: vm.parseJsonAddress(json, ".ics20Transfer.ics26Router"),
            implementation: vm.parseJsonAddress(json, ".ics20Transfer.implementation"),
            pausers: vm.parseJsonAddressArray(json, ".ics20Transfer.pausers"),
            unpausers: vm.parseJsonAddressArray(json, ".ics20Transfer.unpausers"),
            tokenOperator: vm.parseJsonAddress(json, ".ics20Transfer.tokenOperator"),
            permit2: vm.parseJsonAddress(json, ".ics20Transfer.permit2"),
            proxy: vm.parseJsonAddress(json, ".ics20Transfer.proxy")
        });

        return fixture;
    }
}
