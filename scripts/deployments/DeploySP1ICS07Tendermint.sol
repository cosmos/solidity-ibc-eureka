// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.28;

// solhint-disable custom-errors,gas-custom-errors

import { Deployments } from "../helpers/Deployments.sol";
import { SP1ICS07Tendermint } from "../../contracts/light-clients/SP1ICS07Tendermint.sol";
import { stdJson } from "forge-std/StdJson.sol";
import { SP1Verifier as SP1VerifierPlonk } from "@sp1-contracts/v4.0.0-rc.3/SP1VerifierPlonk.sol";
import { SP1Verifier as SP1VerifierGroth16 } from "@sp1-contracts/v4.0.0-rc.3/SP1VerifierGroth16.sol";
import { SP1MockVerifier } from "@sp1-contracts/SP1MockVerifier.sol";
import { IICS07TendermintMsgs } from "../../contracts/light-clients/msgs/IICS07TendermintMsgs.sol";
import { Strings } from "@openzeppelin-contracts/utils/Strings.sol";

abstract contract DeploySP1ICS07Tendermint {
    using stdJson for string;

    function deploySP1ICS07Tendermint(Deployments.SP1ICS07TendermintDeployment memory deployment)
        public
        returns (
            SP1ICS07Tendermint,
            IICS07TendermintMsgs.ConsensusState memory,
            IICS07TendermintMsgs.ClientState memory
        )
    {
        IICS07TendermintMsgs.ConsensusState memory trustedConsensusState =
            abi.decode(deployment.trustedConsensusState, (IICS07TendermintMsgs.ConsensusState));
        IICS07TendermintMsgs.ClientState memory trustedClientState =
            abi.decode(deployment.trustedClientState, (IICS07TendermintMsgs.ClientState));

        address verifier = address(0);

        if (keccak256(bytes(deployment.verifier)) == keccak256(bytes("mock"))) {
            verifier = address(new SP1MockVerifier());
        } else if (bytes(deployment.verifier).length > 0) {
            (bool success, address verifierAddr) = Strings.tryParseAddress(deployment.verifier);
            require(success, string.concat("Invalid verifier address: ", deployment.verifier));
            verifier = verifierAddr;
        } else if (trustedClientState.zkAlgorithm == IICS07TendermintMsgs.SupportedZkAlgorithm.Plonk) {
            verifier = address(new SP1VerifierPlonk());
        } else if (trustedClientState.zkAlgorithm == IICS07TendermintMsgs.SupportedZkAlgorithm.Groth16) {
            verifier = address(new SP1VerifierGroth16());
        } else {
            revert("Unsupported zk algorithm");
        }

        // Deploy the SP1 ICS07 Tendermint light client
        SP1ICS07Tendermint ics07Tendermint = new SP1ICS07Tendermint(
            deployment.updateClientVkey,
            deployment.membershipVkey,
            deployment.ucAndMembershipVkey,
            deployment.misbehaviourVkey,
            verifier,
            deployment.trustedClientState,
            keccak256(abi.encode(trustedConsensusState))
        );

        return (ics07Tendermint, trustedConsensusState, trustedClientState);
    }
}
