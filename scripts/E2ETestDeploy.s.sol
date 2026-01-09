// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

/*
    This script is used for end-to-end testing
*/

// solhint-disable custom-errors,gas-custom-errors

import { stdJson } from "forge-std/StdJson.sol";
import { Script } from "forge-std/Script.sol";

import { IICS07TendermintMsgs } from "../contracts/light-clients/sp1-ics07/msgs/IICS07TendermintMsgs.sol";
import { ICS26Router } from "../contracts/ICS26Router.sol";
import { ICS20Transfer } from "../contracts/ICS20Transfer.sol";
import { ICS27GMP } from "../contracts/ICS27GMP.sol";
import { TestERC20 } from "../test/solidity-ibc/mocks/TestERC20.sol";
import { Strings } from "@openzeppelin-contracts/utils/Strings.sol";
import { ICS20Lib } from "../contracts/utils/ICS20Lib.sol";
import { ICS27Lib } from "../contracts/utils/ICS27Lib.sol";
import { ERC1967Proxy } from "@openzeppelin-contracts/proxy/ERC1967/ERC1967Proxy.sol";
import { DeployAccessManagerWithRoles } from "./deployments/DeployAccessManagerWithRoles.sol";
import { IBCERC20 } from "../contracts/utils/IBCERC20.sol";
import { Escrow } from "../contracts/utils/Escrow.sol";
import { ICS27Account } from "../contracts/utils/ICS27Account.sol";
import { SP1Verifier as SP1VerifierPlonk } from "@sp1-contracts/v5.0.0/SP1VerifierPlonk.sol";
import { SP1Verifier as SP1VerifierGroth16 } from "@sp1-contracts/v5.0.0/SP1VerifierGroth16.sol";
import { SP1MockVerifier } from "@sp1-contracts/SP1MockVerifier.sol";
import { AccessManager } from "@openzeppelin-contracts/access/manager/AccessManager.sol";

import { EVMIFTSendCallConstructor } from "../contracts/utils/EVMIFTSendCallConstructor.sol";
import { IFTBaseUpgradeable } from "../contracts/utils/IFTBaseUpgradeable.sol";
import { OwnableUpgradeable } from "@openzeppelin-upgradeable/access/OwnableUpgradeable.sol";
import { UUPSUpgradeable } from "@openzeppelin-contracts/proxy/utils/UUPSUpgradeable.sol";

/// @title TestIFT - A concrete IFT implementation for e2e testing with mint capability
contract TestIFT is IFTBaseUpgradeable, OwnableUpgradeable, UUPSUpgradeable {
    constructor() {
        _disableInitializers();
    }

    function initialize(
        address owner_,
        string calldata erc20Name,
        string calldata erc20Symbol,
        address ics27Gmp
    )
        external
        initializer
    {
        __Ownable_init(owner_);
        __IFTBase_init(erc20Name, erc20Symbol, ics27Gmp);
    }

    /// @notice Mint tokens for testing purposes
    function mint(address to, uint256 amount) external onlyOwner {
        _mint(to, amount);
    }

    // solhint-disable-next-line no-empty-blocks
    function _onlyAuthority() internal view override(IFTBaseUpgradeable) onlyOwner { }

    // solhint-disable-next-line no-empty-blocks
    function _authorizeUpgrade(address) internal view override(UUPSUpgradeable) onlyOwner { }
}

/// @dev See the Solidity Scripting tutorial: https://book.getfoundry.sh/tutorials/solidity-scripting
contract E2ETestDeploy is Script, IICS07TendermintMsgs, DeployAccessManagerWithRoles {
    using stdJson for string;

    struct DeployedContracts {
        address verifierPlonk;
        address verifierGroth16;
        address verifierMock;
        address ics26Router;
        address ics20Transfer;
        address ics27Gmp;
        address erc20;
        address ift;
        address evmIftConstructor;
    }

    function run() public returns (string memory) {
        address e2eFaucet = vm.envAddress("E2E_FAUCET_ADDRESS");

        vm.startBroadcast();
        DeployedContracts memory c = _deploy(e2eFaucet);
        vm.stopBroadcast();

        return _serializeJson(c);
    }

    function _deploy(address e2eFaucet) internal returns (DeployedContracts memory c) {
        // Phase 1: Deploy all independent contracts (can be batched/parallelized)
        c.verifierPlonk = address(new SP1VerifierPlonk());
        c.verifierGroth16 = address(new SP1VerifierGroth16());
        c.verifierMock = address(new SP1MockVerifier());
        AccessManager accessManager = new AccessManager(msg.sender);
        address routerLogic = address(new ICS26Router());
        address transferLogic = address(new ICS20Transfer());
        address gmpLogic = address(new ICS27GMP());
        address escrow = address(new Escrow());
        address ibcErc20 = address(new IBCERC20());
        address ics27Account = address(new ICS27Account());
        address iftLogic = address(new TestIFT());
        TestERC20 erc20 = new TestERC20();
        c.evmIftConstructor = address(new EVMIFTSendCallConstructor());

        // Phase 2: Deploy proxies (depend on logic contracts above)
        c.ics26Router =
            address(new ERC1967Proxy(routerLogic, abi.encodeCall(ICS26Router.initialize, (address(accessManager)))));

        c.ics20Transfer = address(
            new ERC1967Proxy(
                transferLogic,
                abi.encodeCall(
                    ICS20Transfer.initialize, (c.ics26Router, escrow, ibcErc20, address(0), address(accessManager))
                )
            )
        );

        c.ics27Gmp = address(
            new ERC1967Proxy(
                gmpLogic, abi.encodeCall(ICS27GMP.initialize, (c.ics26Router, ics27Account, address(accessManager)))
            )
        );

        c.ift = address(
            new ERC1967Proxy(
                iftLogic, abi.encodeCall(TestIFT.initialize, (msg.sender, "Test Interchain Token", "TIFT", c.ics27Gmp))
            )
        );

        // Phase 3: Configuration calls (depend on proxies above)
        accessManagerSetTargetRoles(accessManager, c.ics26Router, c.ics20Transfer, true);
        accessManagerSetRoles(
            accessManager, new address[](0), new address[](0), new address[](0), msg.sender, msg.sender, msg.sender
        );
        ICS26Router(c.ics26Router).addIBCApp(ICS20Lib.DEFAULT_PORT_ID, c.ics20Transfer);
        ICS26Router(c.ics26Router).addIBCApp(ICS27Lib.DEFAULT_PORT_ID, c.ics27Gmp);
        erc20.mint(e2eFaucet, type(uint256).max);
        c.erc20 = address(erc20);
    }

    function _serializeJson(DeployedContracts memory c) internal returns (string memory) {
        string memory json = "json";
        json.serialize("verifierPlonk", Strings.toHexString(c.verifierPlonk));
        json.serialize("verifierGroth16", Strings.toHexString(c.verifierGroth16));
        json.serialize("verifierMock", Strings.toHexString(c.verifierMock));
        json.serialize("ics26Router", Strings.toHexString(c.ics26Router));
        json.serialize("ics20Transfer", Strings.toHexString(c.ics20Transfer));
        json.serialize("ics27Gmp", Strings.toHexString(c.ics27Gmp));
        json.serialize("erc20", Strings.toHexString(c.erc20));
        json.serialize("ift", Strings.toHexString(c.ift));
        return json.serialize("evmIftConstructor", Strings.toHexString(c.evmIftConstructor));
    }
}
