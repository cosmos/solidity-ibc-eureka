package main

import (
	"context"
	"fmt"
	"strconv"
	"time"

	ethcommon "github.com/ethereum/go-ethereum/common"

	ibcwasmtypes "github.com/cosmos/ibc-go/modules/light-clients/08-wasm/types"
	clienttypes "github.com/cosmos/ibc-go/v8/modules/core/02-client/types"

	"github.com/strangelove-ventures/interchaintest/v8/ibc"
	"github.com/strangelove-ventures/interchaintest/v8/testutil"

	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/ethereum"
	ethereumligthclient "github.com/srdtrk/solidity-ibc-eureka/e2e/v8/types/ethereumlightclient"
)

func (s *FastSuite) updateEthClient(ctx context.Context, targetBlockNumber int64, simdRelayerUser ibc.Wallet) {
	eth, simd := s.ChainA, s.ChainB
	_, updateTo, err := eth.EthAPI.GetBlockNumber()
	s.Require().NoError(err)
	s.LogVisualizerMessage(fmt.Sprintf("first updateTo: %d", updateTo))
	s.LogVisualizerMessage(fmt.Sprintf("recvBlockNumber: %d", targetBlockNumber))

	if updateTo <= targetBlockNumber {
		time.Sleep(30 * time.Second)

		_, updateTo, err = eth.EthAPI.GetBlockNumber()
		s.Require().NoError(err)
		s.Require().Greater(updateTo, targetBlockNumber)
	}

	wasmClientStateDoNotUseMe, _ := s.GetUnionClientState(ctx, s.unionClientID)
	s.LogVisualizerMessage(fmt.Sprintf("wasmClientStateDoNotUseMe latest height: %+v", wasmClientStateDoNotUseMe.LatestHeight.RevisionHeight))
	_, unionConsensusState := s.GetUnionConsensusState(ctx, s.unionClientID, clienttypes.Height{
		RevisionNumber: 0,
		RevisionHeight: s.lastUnionUpdate,
	})
	s.LogVisualizerMessage(fmt.Sprintf("trusted slot (union cons slot): %d", unionConsensusState.Slot))
	spec, err := eth.BeaconAPIClient.GetSpec()
	s.Require().NoError(err)

	trustedPeriod := unionConsensusState.Slot / spec.Period()
	s.LogVisualizerMessage(fmt.Sprintf("spec period: %d", spec.Period()))
	s.LogVisualizerMessage(fmt.Sprintf("trusted period: %d", trustedPeriod))

	var finalityUpdate ethereum.FinalityUpdateJSONResponse
	var targetPeriod uint64
	err = testutil.WaitForCondition(8*time.Minute, 5*time.Second, func() (bool, error) {
		finalityUpdate, err = eth.BeaconAPIClient.GetFinalityUpdate()
		s.Require().NoError(err)
		targetPeriod = finalityUpdate.Data.AttestedHeader.Beacon.Slot / spec.Period()

		s.LogVisualizerMessage(fmt.Sprintf("Waiting for finality update and target period. updateTo: %d finality update slot: %d, target period: %d", updateTo, finalityUpdate.Data.FinalizedHeader.Beacon.Slot, targetPeriod))

		lightClientUpdates, err := eth.BeaconAPIClient.GetLightClientUpdates(trustedPeriod+1, targetPeriod-trustedPeriod)
		s.Require().NoError(err)

		// for _, update := range lightClientUpdates {
		// 	blocksResp, err := eth.BeaconAPIClient.GetBeaconBlocks(strconv.Itoa(int(update.Data.AttestedHeader.Beacon.Slot)))
		// 	s.Require().NoError(err)
		// 	if !blocksResp.Finalized {
		// 		return false, nil
		// 	}
		// }

		s.LogVisualizerMessage(fmt.Sprintf("len(lightClientUpdates): %d, targetPeriod-trustedPeriod: %d", len(lightClientUpdates), (targetPeriod - trustedPeriod)))
		s.LogVisualizerMessage(fmt.Sprintf("finalityUpdate.Data.FinalizedHeader.Beacon.Slot: %d, updateTo: %d", finalityUpdate.Data.FinalizedHeader.Beacon.Slot, updateTo))
		s.LogVisualizerMessage(fmt.Sprintf("targetPeriod: %d, trustedPeriod: %d", targetPeriod, trustedPeriod))

		return len(lightClientUpdates) >= int(targetPeriod-trustedPeriod) &&
				finalityUpdate.Data.FinalizedHeader.Beacon.Slot > uint64(updateTo) &&
				targetPeriod >= trustedPeriod,
			nil
	})
	s.Require().NoError(err)

	s.LogVisualizerMessage(fmt.Sprintf("targetPeriod: %d", targetPeriod))
	s.LogVisualizerMessage(fmt.Sprintf("trustedPeriod: %d", trustedPeriod))

	var lightClientUpdates ethereum.LightClientUpdatesResponse
	if trustedPeriod < targetPeriod {
		lightClientUpdates, err = eth.BeaconAPIClient.GetLightClientUpdates(trustedPeriod+1, targetPeriod-trustedPeriod)
		s.Require().NoError(err)
		s.LogVisualizerMessage(fmt.Sprintf("Num light client updates for header updates: %d", len(lightClientUpdates)))
		for _, update := range lightClientUpdates {
			s.LogVisualizerMessage(fmt.Sprintf("light client update for header update slot: %d", update.Data.AttestedHeader.Beacon.Slot))
		}
	} else {
		s.LogVisualizerMessage("No light client updates for header updates")
		lightClientUpdates = []ethereum.LightClientUpdateJSON{}
	}

	newHeaders := []ethereumligthclient.Header{}
	trustedSlot := unionConsensusState.Slot
	oldTrustedSlot := trustedSlot
	lastUpdateBlockNumber := trustedSlot
	var prevPubAggKey string
	for _, update := range lightClientUpdates {
		s.LogVisualizerMessage(fmt.Sprintf("old trusted slot: %d", oldTrustedSlot))

		previousPeriod := uint64(1)
		if update.Data.AttestedHeader.Beacon.Slot/spec.Period() > 1 {
			previousPeriod = update.Data.AttestedHeader.Beacon.Slot / spec.Period()
		}
		previousPeriod -= 1
		s.LogVisualizerMessage(fmt.Sprintf("previous period: %d", previousPeriod))

		executionHeight, err := eth.BeaconAPIClient.GetExecutionHeight(strconv.Itoa(int(update.Data.AttestedHeader.Beacon.Slot)))
		s.Require().NoError(err)
		executionHeightHex := fmt.Sprintf("0x%x", executionHeight)
		s.LogVisualizerMessage(fmt.Sprintf("Execution height: %d", executionHeight))
		proofResp, err := eth.EthAPI.GetProof(s.contractAddresses.Ics26Router, []string{}, executionHeightHex)
		s.Require().NoError(err)
		s.Require().NotEmpty(proofResp.AccountProof)
		s.LogVisualizerMessage(fmt.Sprintf("final update: proof resp: %+v", proofResp))

		var proofBz [][]byte
		for _, proofStr := range proofResp.AccountProof {
			proofBz = append(proofBz, ethcommon.FromHex(proofStr))
		}
		accountUpdate := ethereumligthclient.AccountUpdate{
			AccountProof: &ethereumligthclient.AccountProof{
				StorageRoot: ethereum.HexToBeBytes(proofResp.StorageHash),
				Proof:       proofBz,
			},
		}

		previousLightClientUpdates, err := eth.BeaconAPIClient.GetLightClientUpdates(previousPeriod, 1)
		s.Require().NoError(err)
		s.LogVisualizerMessage(fmt.Sprintf("Num previous light client updates: %d", len(previousLightClientUpdates)))
		for _, previousLightClientUpdate := range previousLightClientUpdates {
			s.LogVisualizerMessage(fmt.Sprintf("prev light client update slot: %d", previousLightClientUpdate.Data.AttestedHeader.Beacon.Slot))
			s.LogVisualizerMessage(fmt.Sprintf("prev light client update next sync c aggpubkey: %s", previousLightClientUpdate.Data.NextSyncCommittee.AggregatePubkey))
		}

		// previousLightClientUpdate := previousLightClientUpdates[len(previousLightClientUpdates)-1]
		previousLightClientUpdate := previousLightClientUpdates[0]
		s.LogVisualizerMessage(fmt.Sprintf("prev light client update slot: %d", previousLightClientUpdate.Data.AttestedHeader.Beacon.Slot))

		if previousLightClientUpdate.Data.NextSyncCommittee.AggregatePubkey == prevPubAggKey {
			s.LogVisualizerMessage(fmt.Sprintf("found previous light client update with same aggpubkey: %s", previousLightClientUpdate.Data.NextSyncCommittee.AggregatePubkey))
			continue
		}

		var nextSyncCommitteePubkeys [][]byte
		for _, pubkey := range previousLightClientUpdate.Data.NextSyncCommittee.Pubkeys {
			nextSyncCommitteePubkeys = append(nextSyncCommitteePubkeys, ethcommon.FromHex(pubkey))
		}

		consensusUpdate := update.ToLightClientUpdate()
		newHeaders = append(newHeaders, ethereumligthclient.Header{
			ConsensusUpdate: &consensusUpdate,
			TrustedSyncCommittee: &ethereumligthclient.TrustedSyncCommittee{
				TrustedHeight: &clienttypes.Height{
					RevisionNumber: 0,
					RevisionHeight: oldTrustedSlot,
				},
				NextSyncCommittee: &ethereumligthclient.SyncCommittee{
					Pubkeys:         nextSyncCommitteePubkeys,
					AggregatePubkey: ethcommon.FromHex(previousLightClientUpdate.Data.NextSyncCommittee.AggregatePubkey),
				},
				// CurrentSyncCommittee: &ethereumligthclient.SyncCommittee{},
			},
			AccountUpdate: &accountUpdate,
		})

		lastUpdateBlockNumber = update.Data.AttestedHeader.Beacon.Slot
		oldTrustedSlot = update.Data.AttestedHeader.Beacon.Slot
		prevPubAggKey = previousLightClientUpdate.Data.NextSyncCommittee.AggregatePubkey
	}

	if trustedPeriod >= targetPeriod {
		newHeaders = []ethereumligthclient.Header{}
	}

	s.LogVisualizerMessage(fmt.Sprintf("final update: finality update slot: %d, spec period: %d", finalityUpdate.Data.AttestedHeader.Beacon.Slot, spec.Period()))
	previousPeriod := (finalityUpdate.Data.AttestedHeader.Beacon.Slot / spec.Period())
	if previousPeriod != 0 {
		previousPeriod -= 1
	}
	s.LogVisualizerMessage(fmt.Sprintf("final update: previous period: %d", previousPeriod))
	executionHeight, err := eth.BeaconAPIClient.GetExecutionHeight(fmt.Sprintf("%d", finalityUpdate.Data.AttestedHeader.Beacon.Slot))
	s.Require().NoError(err)
	s.LogVisualizerMessage(fmt.Sprintf("final update: execution height: %d", executionHeight))
	executionHeightHex := fmt.Sprintf("0x%x", executionHeight)
	proofResp, err := eth.EthAPI.GetProof(s.contractAddresses.Ics26Router, []string{}, executionHeightHex)
	s.Require().NoError(err)
	s.Require().NotEmpty(proofResp.AccountProof)
	s.LogVisualizerMessage(fmt.Sprintf("final update: proof resp: %+v", proofResp))

	var proofBz [][]byte
	for _, proofStr := range proofResp.AccountProof {
		proofBz = append(proofBz, ethcommon.FromHex(proofStr))
	}
	accountUpdate := ethereumligthclient.AccountUpdate{
		AccountProof: &ethereumligthclient.AccountProof{
			StorageRoot: ethereum.HexToBeBytes(proofResp.StorageHash),
			Proof:       proofBz,
		},
	}

	previousPeriodLightClientUpdate, err := eth.BeaconAPIClient.GetLightClientUpdates(previousPeriod, 1)
	s.Require().NoError(err)
	s.LogVisualizerMessage(fmt.Sprintf("final update: Num previous light client updates: %d", len(previousPeriodLightClientUpdate)))
	// var previousLightClientUpdate ethereum.LightClientUpdateJSON
	for _, update := range previousPeriodLightClientUpdate {
		s.LogVisualizerMessage(fmt.Sprintf("final update: prev light client update slot: %d", update.Data.AttestedHeader.Beacon.Slot))
		s.LogVisualizerMessage(fmt.Sprintf("final update: prev light client update next sync c aggpubkey: %s", update.Data.NextSyncCommittee.AggregatePubkey))
		// if update.Data.NextSyncCommittee.AggregatePubkey == finalityUpdate.Data {
		// 	s.LogVisualizerMessage(fmt.Sprintf("final update: found previous light client update with same aggpubkey: %s", update.Data.NextSyncCommittee.AggregatePubkey))
		// 	previousLightClientUpdate = update
		// }
	}
	previousLightClientUpdate := previousPeriodLightClientUpdate[len(previousPeriodLightClientUpdate)-1]
	s.LogVisualizerMessage(fmt.Sprintf("final update: prev light client update slot: %d", previousLightClientUpdate.Data.AttestedHeader.Beacon.Slot))

	currentSyncCommitteePubkeys := [][]byte{}
	for _, pubkey := range previousLightClientUpdate.Data.NextSyncCommittee.Pubkeys {
		currentSyncCommitteePubkeys = append(currentSyncCommitteePubkeys, ethcommon.FromHex(pubkey))
	}

	consensusUpdate := finalityUpdate.ToLightClientUpdate()
	oldestHeader := ethereumligthclient.Header{
		ConsensusUpdate: &consensusUpdate,
		TrustedSyncCommittee: &ethereumligthclient.TrustedSyncCommittee{
			TrustedHeight: &clienttypes.Height{
				RevisionNumber: 0,
				RevisionHeight: unionConsensusState.Slot,
			},
			// NextSyncCommittee: &ethereumligthclient.SyncCommittee{
			// 	Pubkeys:         nextSyncCommitteePubkeys,
			// 	AggregatePubkey: ethcommon.FromHex(previousLightClientUpdate.Data.NextSyncCommittee.AggregatePubkey),
			// },
			CurrentSyncCommittee: &ethereumligthclient.SyncCommittee{
				Pubkeys:         currentSyncCommitteePubkeys,
				AggregatePubkey: ethcommon.FromHex(previousLightClientUpdate.Data.NextSyncCommittee.AggregatePubkey),
			},
		},
		AccountUpdate: &accountUpdate,
	}

	//		        let does_not_have_finality_update = last_update_block_number >= update_to.revision_height;
	doesNotHaveFinalityUpdate := lastUpdateBlockNumber >= finalityUpdate.Data.AttestedHeader.Beacon.Slot
	var headers []ethereumligthclient.Header
	headers = append(headers, newHeaders...)

	if doesNotHaveFinalityUpdate {
		s.LogVisualizerMessage(fmt.Sprintf("does not have finality update: lastUpdateBlockNumber: %d, finalityUpdate slot: %d", lastUpdateBlockNumber, finalityUpdate.Data.AttestedHeader.Beacon.Slot))
	} else {
		s.LogVisualizerMessage(fmt.Sprintf("has finality update: lastUpdateBlockNumber: %d, finalityUpdate slot: %d", lastUpdateBlockNumber, finalityUpdate.Data.AttestedHeader.Beacon.Slot))
		headers = append(headers, oldestHeader)

	}

	s.LogVisualizerMessage(fmt.Sprintf("Num headers: %d", len(headers)))

	// #[error(
	//     "(update_signature_slot > update_attested_slot >= update_finalized_slot) must hold, \
	//     found: ({update_signature_slot} > {update_attested_slot} >= {update_finalized_slot})"
	// )]

	// current_slot >= update.signature_slot
	// && update.signature_slot > update_attested_slot
	// && update_attested_slot >= update_finalized_slot,

	wasmClientState, unionClientState := s.GetUnionClientState(ctx, s.unionClientID)
	_, unionConsensusState = s.GetUnionConsensusState(ctx, s.unionClientID, wasmClientState.LatestHeight)
	s.LogVisualizerMessage(fmt.Sprintf("submitting header to client with wasm latest height: %d", wasmClientState.LatestHeight.RevisionHeight))
	s.LogVisualizerMessage(fmt.Sprintf("submitting header to client with union latest height: %d", unionClientState.LatestSlot))
	s.LogVisualizerMessage(fmt.Sprintf("submitting header to client with union cons height: %d", unionConsensusState.Slot))
	s.LogVisualizerMessage(fmt.Sprintf("submitting header to client with union current cons pub agg key: %s", ethcommon.Bytes2Hex(unionConsensusState.CurrentSyncCommittee)))
	s.LogVisualizerMessage(fmt.Sprintf("submitting header to client with union next cons pub agg key: %s", ethcommon.Bytes2Hex(unionConsensusState.CurrentSyncCommittee)))

	s.LogVisualizerMessage("loop headers")
	for _, header := range headers {
		s.LogVisualizerMessage(fmt.Sprintf("submittiong header slot: %d", header.ConsensusUpdate.AttestedHeader.Beacon.Slot))
		s.LogVisualizerMessage(fmt.Sprintf("submitting header with trusted slot: %d", header.TrustedSyncCommittee.TrustedHeight.RevisionHeight))
		if header.TrustedSyncCommittee.CurrentSyncCommittee != nil {
			s.LogVisualizerMessage(fmt.Sprintf("submitting header with current sync committee: %s", ethcommon.Bytes2Hex(header.TrustedSyncCommittee.CurrentSyncCommittee.AggregatePubkey)))
		}
		if header.TrustedSyncCommittee.NextSyncCommittee != nil {
			s.LogVisualizerMessage(fmt.Sprintf("submitting header with next sync committee: %s", ethcommon.Bytes2Hex(header.TrustedSyncCommittee.NextSyncCommittee.AggregatePubkey)))
		}
		s.LogVisualizerMessage(fmt.Sprintf("submitting header with signature slot: %d", header.ConsensusUpdate.SignatureSlot))
		s.LogVisualizerMessage(fmt.Sprintf("submitting header with attested slot: %d", header.ConsensusUpdate.AttestedHeader.Beacon.Slot))
		s.LogVisualizerMessage(fmt.Sprintf("submitting header with finalized slot: %d", header.ConsensusUpdate.FinalizedHeader.Beacon.Slot))
		s.LogVisualizerMessage(fmt.Sprintf("submitting header with account update storage root: %s", ethcommon.Bytes2Hex(header.AccountUpdate.AccountProof.StorageRoot)))
		s.LogVisualizerMessage(fmt.Sprintf("submitting header with exec state root: %s", ethcommon.Bytes2Hex(header.ConsensusUpdate.AttestedHeader.Execution.StateRoot)))
	}

	for _, header := range headers {
		s.LogVisualizerMessage(fmt.Sprintf("submitting header slot: %d", header.ConsensusUpdate.AttestedHeader.Beacon.Slot))
		headerBz := simd.Config().EncodingConfig.Codec.MustMarshal(&header)
		wasmHeader := ibcwasmtypes.ClientMessage{
			Data: headerBz,
		}

		wasmHeaderAny, err := clienttypes.PackClientMessage(&wasmHeader)
		s.Require().NoError(err)
		_, err = s.BroadcastMessages(ctx, simd, simdRelayerUser, 200_000, &clienttypes.MsgUpdateClient{
			ClientId:      s.unionClientID,
			ClientMessage: wasmHeaderAny,
			Signer:        simdRelayerUser.FormattedAddress(),
		})
		s.Require().NoError(err)
		s.LogVisualizerMessage("OH MY FUCKING GOD, YES!!!!!")
		s.lastUnionUpdate = header.ConsensusUpdate.AttestedHeader.Beacon.Slot
		time.Sleep(10 * time.Second)

		if header.ConsensusUpdate.AttestedHeader.Beacon.Slot >= uint64(updateTo) {
			s.LogVisualizerMessage("we have updated past updateTo! we should be able to prove now!")
			break
		}
	}

}
