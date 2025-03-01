// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.28;

/*
    This script is used for end-to-end testing
*/

// solhint-disable custom-errors,gas-custom-errors

import { stdJson } from "forge-std/StdJson.sol";
import { Script } from "forge-std/Script.sol";
import { SP1ICS07Tendermint } from "../contracts/light-clients/SP1ICS07Tendermint.sol";
import { IICS07TendermintMsgs } from "../contracts/light-clients/msgs/IICS07TendermintMsgs.sol";
import { TestERC20 } from "../test/solidity-ibc/mocks/TestERC20.sol";
import { ICS26Router } from "../contracts/ICS26Router.sol";
import { ICS20Transfer } from "../contracts/ICS20Transfer.sol";
import { ERC1967Proxy } from "@openzeppelin-contracts/proxy/ERC1967/ERC1967Proxy.sol";
import { IBCERC20 } from "../contracts/utils/IBCERC20.sol";
import { ICS20Lib } from "../contracts/utils/ICS20Lib.sol";
import { SP1Verifier as SP1VerifierPlonk } from "@sp1-contracts/v4.0.0-rc.3/SP1VerifierPlonk.sol";
import { SP1Verifier as SP1VerifierGroth16 } from "@sp1-contracts/v4.0.0-rc.3/SP1VerifierGroth16.sol";
import { SP1MockVerifier } from "@sp1-contracts/SP1MockVerifier.sol";
import { Strings } from "@openzeppelin-contracts/utils/Strings.sol";
import { Escrow } from "../contracts/utils/Escrow.sol";
import { TendermintLib } from "./utils/TendermintLib.sol";

/// @dev See the Solidity Scripting tutorial: https://book.getfoundry.sh/guides/scripting-with-solidity
contract E2ETestDeploy is Script, IICS07TendermintMsgs {
    using stdJson for string;

    function run() public returns (string memory) {
        // ============ Step 1: Load parameters ==============
        string memory root = vm.projectRoot();
        string memory tendermintGenesisJson = vm.readFile(string.concat(root, "/scripts/genesis.json"));
        TendermintLib.SP1ICS07TendermintGenesisJson memory genesis = TendermintLib.loadTendermintGenesisFromJson(tendermintGenesisJson);
        ClientState memory trustedClientState = abi.decode(genesis.trustedClientState, (ClientState));

        address e2eFaucet = vm.envAddress("E2E_FAUCET_ADDRESS");

        // The verifier address can be set in the environment variables.
        // If not set, then the verifier is set based on the zkAlgorithm.
        // If set to "mock", then the verifier is set to a mock verifier.
        string memory verifierEnv = vm.envOr("VERIFIER", string(""));

        // ============ Step 2: Deploy the contracts ==============
        vm.startBroadcast();

        address verifier;
        if (keccak256(bytes(verifierEnv)) == keccak256(bytes("mock"))) {
            verifier = address(new SP1MockVerifier());
        } else if (bytes(verifierEnv).length > 0) {
            (bool success, address verifierAddr) = Strings.tryParseAddress(verifierEnv);
            require(success, string.concat("Invalid verifier address: ", verifierEnv));
            verifier = verifierAddr;
        } else if (trustedClientState.zkAlgorithm == SupportedZkAlgorithm.Plonk) {
            verifier = address(new SP1VerifierPlonk());
        } else if (trustedClientState.zkAlgorithm == SupportedZkAlgorithm.Groth16) {
            verifier = address(new SP1VerifierGroth16());
        } else {
            revert("Unsupported zk algorithm");
        }

        // Deploy the SP1 ICS07 Tendermint light client
                // Deploy new light client
        IICS07TendermintMsgs.ConsensusState memory trustedConsensusState =
            abi.decode(genesis.trustedConsensusState, (IICS07TendermintMsgs.ConsensusState));

        // Deploy the SP1 ICS07 Tendermint light client
        SP1ICS07Tendermint ics07Tendermint = new SP1ICS07Tendermint(
            genesis.updateClientVkey,
            genesis.membershipVkey,
            genesis.ucAndMembershipVkey,
            genesis.misbehaviourVkey,
            verifier,
            genesis.trustedClientState,
            keccak256(abi.encode(trustedConsensusState))
        );

        // Deploy IBC Eureka
        address escrowLogic = address(new Escrow());
        address ibcERC20Logic = address(new IBCERC20());
        address ics26RouterLogic = address(new ICS26Router());
        address ics20TransferLogic = address(new ICS20Transfer());

        ERC1967Proxy routerProxy = new ERC1967Proxy(
            ics26RouterLogic,
            abi.encodeWithSelector(
                ICS26Router.initialize.selector,
                msg.sender,
                msg.sender
            )
        );

        ERC1967Proxy transferProxy = new ERC1967Proxy(
            ics20TransferLogic,
            abi.encodeWithSelector(
                ICS20Transfer.initialize.selector,
                address(routerProxy),
                escrowLogic,
                ibcERC20Logic,
                msg.sender,
                address(0)
            )
        );

        ICS26Router ics26Router = ICS26Router(address(routerProxy));
        ICS20Transfer ics20Transfer = ICS20Transfer(address(transferProxy));

        // Wire Transfer app
        ics26Router.addIBCApp(ICS20Lib.DEFAULT_PORT_ID, address(ics20Transfer));


        // Deploy a test ERC20
        TestERC20 erc20 = new TestERC20();

        // Mint some tokens
        erc20.mint(e2eFaucet, type(uint256).max);

        vm.stopBroadcast();

        string memory json = "json";
        json.serialize("ics07Tendermint", Strings.toHexString(address(ics07Tendermint)));
        json.serialize("ics26Router", Strings.toHexString(address(ics26Router)));
        json.serialize("ics20Transfer", Strings.toHexString(address(ics20Transfer)));
        string memory finalJson = json.serialize("erc20", Strings.toHexString(address(erc20)));

        return finalJson;
    }
}
