// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.28;

// solhint-disable custom-errors,gas-custom-errors

import { stdJson } from "forge-std/StdJson.sol";
import { SP1ICS07Tendermint } from "../contracts/light-clients/SP1ICS07Tendermint.sol";
import { ISP1ICS07Tendermint } from "../contracts/light-clients/SP1ICS07Tendermint.sol";
import { IICS07TendermintMsgs } from "../contracts/light-clients/msgs/IICS07TendermintMsgs.sol";
import { ICS26Router } from "../contracts/ICS26Router.sol";
import { IICS26Router } from "../contracts/interfaces/IICS26Router.sol";
import { IICS20Transfer } from "../contracts/interfaces/IICS20Transfer.sol";
import { ICS20Transfer } from "../contracts/ICS20Transfer.sol";
import { ICS20Lib } from "../contracts/utils/ICS20Lib.sol";
import { IICS07TendermintMsgs } from "../contracts/light-clients/msgs/IICS07TendermintMsgs.sol";
import { ERC1967Proxy } from "@openzeppelin-contracts/proxy/ERC1967/ERC1967Proxy.sol";
import { IBCERC20 } from "../contracts/utils/IBCERC20.sol";
import { Escrow } from "../contracts/utils/Escrow.sol";

library DeployLib {
    using stdJson for string;

    struct DeploymentConfigJson {
        address timelockAdminAddress;
        address portCustomizerAddress;
        address ics20PauserAddress;
        address permit2Address;
    }

    struct SP1ICS07TendermintGenesisJson {
        bytes trustedClientState;
        bytes trustedConsensusState;
        bytes32 updateClientVkey;
        bytes32 membershipVkey;
        bytes32 ucAndMembershipVkey;
        bytes32 misbehaviourVkey;
    }

    function deployIBCCore(DeploymentConfigJson memory deploymentConfig) public returns (IICS26Router, IICS20Transfer) {
        // Deploy IBC Eureka with proxy
        address escrowLogic = address(new Escrow());
        address ibcERC20Logic = address(new IBCERC20());
        address ics26RouterLogic = address(new ICS26Router());
        address ics20TransferLogic = address(new ICS20Transfer());

        ERC1967Proxy routerProxy = new ERC1967Proxy(
            ics26RouterLogic,
            abi.encodeWithSelector(
                ICS26Router.initialize.selector,
                deploymentConfig.timelockAdminAddress,
                deploymentConfig.portCustomizerAddress
            )
        );

        ERC1967Proxy transferProxy = new ERC1967Proxy(
            ics20TransferLogic,
            abi.encodeWithSelector(
                ICS20Transfer.initialize.selector,
                address(routerProxy),
                escrowLogic,
                ibcERC20Logic,
                deploymentConfig.ics20PauserAddress,
                deploymentConfig.permit2Address
            )
        );

        ICS26Router ics26Router = ICS26Router(address(routerProxy));
        ICS20Transfer ics20Transfer = ICS20Transfer(address(transferProxy));

        return (ics26Router, ics20Transfer);
    }

    function deployTendermintLightClient(SP1ICS07TendermintGenesisJson memory genesis, address sp1Verifier) public returns (ISP1ICS07Tendermint) {
        IICS07TendermintMsgs.ConsensusState memory trustedConsensusState =
            abi.decode(genesis.trustedConsensusState, (IICS07TendermintMsgs.ConsensusState));

        // Deploy the SP1 ICS07 Tendermint light client
        return new SP1ICS07Tendermint(
            genesis.updateClientVkey,
            genesis.membershipVkey,
            genesis.ucAndMembershipVkey,
            genesis.misbehaviourVkey,
            sp1Verifier,
            genesis.trustedClientState,
            keccak256(abi.encode(trustedConsensusState))
        );
    }

    function loadDeploymentConfigFromJson(string memory deploymentConfigJson) internal pure returns (DeploymentConfigJson memory) {
        address timelockAdminAddress = deploymentConfigJson.readAddress(".timelockAdminAddress");
        address portCustomizerAddress = deploymentConfigJson.readAddress(".portCustomizerAddress");
        address ics20PauserAddress = deploymentConfigJson.readAddress(".ics20PauserAddress");
        address permit2Address = deploymentConfigJson.readAddress(".permit2Address");

        return DeploymentConfigJson(
            timelockAdminAddress,
            portCustomizerAddress,
            ics20PauserAddress,
            permit2Address
        );
    }

    function loadTendermintGenesisFromJson(string memory json) public pure returns (SP1ICS07TendermintGenesisJson memory) {
        bytes memory trustedClientState = json.readBytes(".trustedClientState");
        bytes memory trustedConsensusState = json.readBytes(".trustedConsensusState");
        bytes32 updateClientVkey = json.readBytes32(".updateClientVkey");
        bytes32 membershipVkey = json.readBytes32(".membershipVkey");
        bytes32 ucAndMembershipVkey = json.readBytes32(".ucAndMembershipVkey");
        bytes32 misbehaviourVkey = json.readBytes32(".misbehaviourVkey");

        SP1ICS07TendermintGenesisJson memory fixture = SP1ICS07TendermintGenesisJson({
            trustedClientState: trustedClientState,
            trustedConsensusState: trustedConsensusState,
            updateClientVkey: updateClientVkey,
            membershipVkey: membershipVkey,
            ucAndMembershipVkey: ucAndMembershipVkey,
            misbehaviourVkey: misbehaviourVkey
        });

        return fixture;
    }



}
