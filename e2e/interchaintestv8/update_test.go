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

func (s *FastSuite) updateEthClient(ctx context.Context, minimumUpdateTo int64, simdRelayerUser ibc.Wallet) {
	eth, simd := s.ChainA, s.ChainB

	// Wait until we have a block number greater than the minimum update to
	var updateTo int64
	var err error
	err = testutil.WaitForCondition(5*time.Minute, 5*time.Second, func() (bool, error) {
		_, updateTo, err = eth.EthAPI.GetBlockNumber()
		s.Require().NoError(err)

		return updateTo > minimumUpdateTo, nil
	})
	s.Require().NoError(err)

	_, unionConsensusState := s.GetUnionConsensusState(ctx, s.unionClientID, clienttypes.Height{
		RevisionNumber: 0,
		RevisionHeight: s.lastUnionUpdate,
	})
	spec, err := eth.BeaconAPIClient.GetSpec()
	s.Require().NoError(err)

	trustedPeriod := unionConsensusState.Slot / spec.Period()

	var finalityUpdate ethereum.FinalityUpdateJSONResponse
	var targetPeriod uint64
	err = testutil.WaitForCondition(8*time.Minute, 5*time.Second, func() (bool, error) {
		finalityUpdate, err = eth.BeaconAPIClient.GetFinalityUpdate()
		s.Require().NoError(err)

		targetPeriod = finalityUpdate.Data.AttestedHeader.Beacon.Slot / spec.Period()

		return finalityUpdate.Data.FinalizedHeader.Beacon.Slot > uint64(updateTo) &&
				targetPeriod >= trustedPeriod,
			nil
	})
	s.Require().NoError(err)

	_, unionClientState := s.GetUnionClientState(ctx, s.unionClientID)
	// Wait until computed slot is greater than all of the updates signature slots
	err = testutil.WaitForCondition(8*time.Minute, 5*time.Second, func() (bool, error) {
		lightClientUpdates, err := eth.BeaconAPIClient.GetLightClientUpdates(trustedPeriod+1, targetPeriod-trustedPeriod)
		s.Require().NoError(err)

		computedSlot := (uint64(time.Now().Unix())-unionClientState.GenesisTime)/
			uint64(unionClientState.SecondsPerSlot) + spec.GenesisSlot

		for _, update := range lightClientUpdates {
			if computedSlot < update.Data.SignatureSlot {
				return false, nil
			}
		}

		return len(lightClientUpdates) >= int(targetPeriod-trustedPeriod), nil
	})
	s.Require().NoError(err)

	var lightClientUpdates ethereum.LightClientUpdatesResponse
	if trustedPeriod < targetPeriod {
		lightClientUpdates, err = eth.BeaconAPIClient.GetLightClientUpdates(trustedPeriod+1, targetPeriod-trustedPeriod)
		s.Require().NoError(err)
	} else {
		lightClientUpdates = []ethereum.LightClientUpdateJSON{}
	}

	headers := []ethereumligthclient.Header{}
	trustedSlot := unionConsensusState.Slot
	oldTrustedSlot := trustedSlot
	lastUpdateBlockNumber := trustedSlot
	var prevPubAggKey string
	for _, update := range lightClientUpdates {

		previousPeriod := uint64(1)
		if update.Data.AttestedHeader.Beacon.Slot/spec.Period() > 1 {
			previousPeriod = update.Data.AttestedHeader.Beacon.Slot / spec.Period()
		}
		previousPeriod -= 1

		executionHeight, err := eth.BeaconAPIClient.GetExecutionHeight(strconv.Itoa(int(update.Data.AttestedHeader.Beacon.Slot)))
		s.Require().NoError(err)
		executionHeightHex := fmt.Sprintf("0x%x", executionHeight)
		proofResp, err := eth.EthAPI.GetProof(s.contractAddresses.Ics26Router, []string{}, executionHeightHex)
		s.Require().NoError(err)
		s.Require().NotEmpty(proofResp.AccountProof)

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

		previousLightClientUpdate := previousLightClientUpdates[0]

		if previousLightClientUpdate.Data.NextSyncCommittee.AggregatePubkey == prevPubAggKey {
			continue
		}

		var nextSyncCommitteePubkeys [][]byte
		for _, pubkey := range previousLightClientUpdate.Data.NextSyncCommittee.Pubkeys {
			nextSyncCommitteePubkeys = append(nextSyncCommitteePubkeys, ethcommon.FromHex(pubkey))
		}

		consensusUpdate := update.ToLightClientUpdate()
		header := ethereumligthclient.Header{
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
			},
			AccountUpdate: &accountUpdate,
		}
		headers = append(headers, header)
		logHeader("Adding new header", header)

		lastUpdateBlockNumber = update.Data.AttestedHeader.Beacon.Slot
		oldTrustedSlot = update.Data.AttestedHeader.Beacon.Slot
		prevPubAggKey = previousLightClientUpdate.Data.NextSyncCommittee.AggregatePubkey
	}

	if trustedPeriod >= targetPeriod {
		headers = []ethereumligthclient.Header{}
	}

	previousPeriod := (finalityUpdate.Data.AttestedHeader.Beacon.Slot / spec.Period())
	if previousPeriod != 0 {
		previousPeriod -= 1
	}
	executionHeight, err := eth.BeaconAPIClient.GetExecutionHeight(fmt.Sprintf("%d", finalityUpdate.Data.AttestedHeader.Beacon.Slot))
	s.Require().NoError(err)
	executionHeightHex := fmt.Sprintf("0x%x", executionHeight)
	proofResp, err := eth.EthAPI.GetProof(s.contractAddresses.Ics26Router, []string{}, executionHeightHex)
	s.Require().NoError(err)
	s.Require().NotEmpty(proofResp.AccountProof)

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
	previousLightClientUpdate := previousPeriodLightClientUpdate[len(previousPeriodLightClientUpdate)-1]

	currentSyncCommitteePubkeys := [][]byte{}
	for _, pubkey := range previousLightClientUpdate.Data.NextSyncCommittee.Pubkeys {
		currentSyncCommitteePubkeys = append(currentSyncCommitteePubkeys, ethcommon.FromHex(pubkey))
	}

	hasFinalityUpdate := lastUpdateBlockNumber < finalityUpdate.Data.AttestedHeader.Beacon.Slot

	if hasFinalityUpdate {
		consensusUpdate := finalityUpdate.ToLightClientUpdate()
		header := ethereumligthclient.Header{
			ConsensusUpdate: &consensusUpdate,
			TrustedSyncCommittee: &ethereumligthclient.TrustedSyncCommittee{
				TrustedHeight: &clienttypes.Height{
					RevisionNumber: 0,
					RevisionHeight: unionConsensusState.Slot,
				},
				CurrentSyncCommittee: &ethereumligthclient.SyncCommittee{
					Pubkeys:         currentSyncCommitteePubkeys,
					AggregatePubkey: ethcommon.FromHex(previousLightClientUpdate.Data.NextSyncCommittee.AggregatePubkey),
				},
			},
			AccountUpdate: &accountUpdate,
		}

		logHeader("We have finality update", header)
		headers = append(headers, header)

	}

	wasmClientState, _ := s.GetUnionClientState(ctx, s.unionClientID)
	_, unionConsensusState = s.GetUnionConsensusState(ctx, s.unionClientID, wasmClientState.LatestHeight)

	for _, header := range headers {
		logHeader("Updating eth light client", header)
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

		s.lastUnionUpdate = header.ConsensusUpdate.AttestedHeader.Beacon.Slot
		fmt.Println("Updated eth light client to block number", s.lastUnionUpdate)
		time.Sleep(10 * time.Second)

		if header.ConsensusUpdate.AttestedHeader.Beacon.Slot >= uint64(updateTo) {
			fmt.Println("Updated past target block number, skipping any further updates")
			break
		}
	}

}

func logHeader(prefix string, header ethereumligthclient.Header) {
	fmt.Printf("%s: header height: %d, trusted height: %d, signature slot: %d, finalized slot: %d, finalized execution block: %d\n",
		prefix,
		header.ConsensusUpdate.AttestedHeader.Beacon.Slot,
		header.TrustedSyncCommittee.TrustedHeight.RevisionHeight,
		header.ConsensusUpdate.SignatureSlot,
		header.ConsensusUpdate.FinalizedHeader.Beacon.Slot,
		header.ConsensusUpdate.FinalizedHeader.Execution.BlockNumber)

}
