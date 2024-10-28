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

	if updateTo <= targetBlockNumber {
		time.Sleep(30 * time.Second)

		_, updateTo, err = eth.EthAPI.GetBlockNumber()
		s.Require().NoError(err)
		s.Require().Greater(updateTo, targetBlockNumber)
	}

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

		lightClientUpdates, err := eth.BeaconAPIClient.GetLightClientUpdates(trustedPeriod+1, targetPeriod-trustedPeriod)
		s.Require().NoError(err)

		return len(lightClientUpdates) >= int(targetPeriod-trustedPeriod) &&
				finalityUpdate.Data.FinalizedHeader.Beacon.Slot > uint64(updateTo) &&
				targetPeriod >= trustedPeriod,
			nil
	})
	s.Require().NoError(err)

	var lightClientUpdates ethereum.LightClientUpdatesResponse
	if trustedPeriod < targetPeriod {
		lightClientUpdates, err = eth.BeaconAPIClient.GetLightClientUpdates(trustedPeriod+1, targetPeriod-trustedPeriod)
		s.Require().NoError(err)
	} else {
		lightClientUpdates = []ethereum.LightClientUpdateJSON{}
	}

	newHeaders := []ethereumligthclient.Header{}
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

	consensusUpdate := finalityUpdate.ToLightClientUpdate()
	oldestHeader := ethereumligthclient.Header{
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

	doesNotHaveFinalityUpdate := lastUpdateBlockNumber >= finalityUpdate.Data.AttestedHeader.Beacon.Slot
	var headers []ethereumligthclient.Header
	headers = append(headers, newHeaders...)

	// TODO: Flip this thing, no need for all this negativity
	if !doesNotHaveFinalityUpdate {
		headers = append(headers, oldestHeader)

	}

	wasmClientState, _ := s.GetUnionClientState(ctx, s.unionClientID)
	_, unionConsensusState = s.GetUnionConsensusState(ctx, s.unionClientID, wasmClientState.LatestHeight)

	for _, header := range headers {
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
