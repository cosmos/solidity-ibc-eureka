package e2esuite

import (
	"context"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"os"
	"strconv"
	"time"

	ethcommon "github.com/ethereum/go-ethereum/common"

	ibcwasmtypes "github.com/cosmos/ibc-go/modules/light-clients/08-wasm/types"
	clienttypes "github.com/cosmos/ibc-go/v9/modules/core/02-client/types"
	ibctesting "github.com/cosmos/ibc-go/v9/testing"

	"github.com/strangelove-ventures/interchaintest/v8/ibc"
	"github.com/strangelove-ventures/interchaintest/v8/testutil"

	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/ethereum"
	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/testvalues"
	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/types"
	ethereumtypes "github.com/srdtrk/solidity-ibc-eureka/e2e/v8/types/ethereum"
)

func (s *TestSuite) CreateEthereumLightClient(ctx context.Context, simdRelayerUser ibc.Wallet, ibcContractAddress string, rustFixtureGenerator *types.RustFixtureGenerator) {
	switch s.ethTestnetType {
	case testvalues.EthTestnetTypePoW:
		s.createDummyLightClient(ctx, simdRelayerUser)
	case testvalues.EthTestnetTypePoS:
		s.createEthereumLightClient(ctx, simdRelayerUser, ibcContractAddress, rustFixtureGenerator)
	default:
		panic(fmt.Sprintf("Unrecognized Ethereum testnet type: %v", s.ethTestnetType))
	}
}

func (s *TestSuite) UpdateEthClient(ctx context.Context, ibcContractAddress string, minimumUpdateTo uint64, simdRelayerUser ibc.Wallet, rustFixtureGenerator *types.RustFixtureGenerator) {
	if s.ethTestnetType != testvalues.EthTestnetTypePoS {
		return
	}

	eth, simd := s.EthChain, s.CosmosChains[0]

	// Wait until we have a block number greater than the minimum update to
	var updateTo uint64
	var err error
	err = testutil.WaitForCondition(5*time.Minute, 5*time.Second, func() (bool, error) {
		_, updateTo, err = eth.EthAPI.GetBlockNumber()
		s.Require().NoError(err)

		return updateTo > minimumUpdateTo, nil
	})
	s.Require().NoError(err)
	fmt.Printf("Updating eth light client to at least block number %d (with minimum requested: %d)\n", updateTo, minimumUpdateTo)

	_, ethereumConsensusState := s.GetEthereumConsensusState(ctx, simd, s.EthereumLightClientID, clienttypes.Height{
		RevisionNumber: 0,
		RevisionHeight: s.LastEtheruemLightClientUpdate,
	})
	spec, err := eth.BeaconAPIClient.GetSpec()
	s.Require().NoError(err)

	trustedPeriod := ethereumConsensusState.Slot / spec.Period()

	var finalityUpdate ethereum.FinalityUpdateJSONResponse
	var targetPeriod uint64
	// Now we need to wait for finalization and light client update availability to be after updateTo
	err = testutil.WaitForCondition(8*time.Minute, 5*time.Second, func() (bool, error) {
		finalityUpdate, err = eth.BeaconAPIClient.GetFinalityUpdate()
		s.Require().NoError(err)

		targetPeriod = finalityUpdate.Data.AttestedHeader.Beacon.GetSlot() / spec.Period()

		lightClientUpdates, err := eth.BeaconAPIClient.GetLightClientUpdates(trustedPeriod+1, targetPeriod-trustedPeriod)
		s.Require().NoError(err)
		var highestUpdateSlot uint64
		for _, update := range lightClientUpdates {
			if update.Data.AttestedHeader.Beacon.GetSlot() > highestUpdateSlot {
				highestUpdateSlot = update.Data.AttestedHeader.Beacon.GetSlot()
			}
		}

		return finalityUpdate.Data.FinalizedHeader.Beacon.GetSlot() > updateTo &&
				targetPeriod > trustedPeriod &&
				highestUpdateSlot > updateTo,
			nil
	})
	s.Require().NoError(err)

	_, ethereumClientState := s.GetEthereumClientState(ctx, simd, s.EthereumLightClientID)
	// Wait until computed slot is greater than all of the updates signature slots
	err = testutil.WaitForCondition(8*time.Minute, 5*time.Second, func() (bool, error) {
		lightClientUpdates, err := eth.BeaconAPIClient.GetLightClientUpdates(trustedPeriod+1, targetPeriod-trustedPeriod)
		s.Require().NoError(err)

		computedSlot := (uint64(time.Now().Unix())-ethereumClientState.GenesisTime)/
			ethereumClientState.SecondsPerSlot + spec.GenesisSlot

		for _, update := range lightClientUpdates {
			if computedSlot < update.Data.GetSignatureSlot() {
				return false, nil
			}
		}

		return len(lightClientUpdates) >= int(targetPeriod-trustedPeriod), nil
	})
	s.Require().NoError(err)

	s.Require().Greater(targetPeriod, trustedPeriod)

	lightClientUpdates, err := eth.BeaconAPIClient.GetLightClientUpdates(trustedPeriod+1, targetPeriod-trustedPeriod)
	s.Require().NoError(err)

	headers := []ethereumtypes.Header{}
	trustedSlot := ethereumConsensusState.Slot
	var prevPubAggKey string
	for _, update := range lightClientUpdates {

		previousPeriod := uint64(1)
		if update.Data.AttestedHeader.Beacon.GetSlot()/spec.Period() > 1 {
			previousPeriod = update.Data.AttestedHeader.Beacon.GetSlot() / spec.Period()
		}
		previousPeriod -= 1

		executionHeight, err := eth.BeaconAPIClient.GetExecutionHeight(strconv.Itoa(int(update.Data.AttestedHeader.Beacon.GetSlot())))
		s.Require().NoError(err)
		executionHeightHex := fmt.Sprintf("0x%x", executionHeight)
		proofResp, err := eth.EthAPI.GetProof(ibcContractAddress, []string{}, executionHeightHex)
		s.Require().NoError(err)
		s.Require().NotEmpty(proofResp.AccountProof)

		accountUpdate := ethereumtypes.AccountUpdate{
			AccountProof: ethereumtypes.AccountProof{
				StorageRoot: proofResp.StorageHash,
				Proof:       proofResp.AccountProof,
			},
		}

		previousLightClientUpdates, err := eth.BeaconAPIClient.GetLightClientUpdates(previousPeriod, 1)
		s.Require().NoError(err)

		previousLightClientUpdate := previousLightClientUpdates[0]

		if previousLightClientUpdate.Data.NextSyncCommittee.AggregatePubkey == prevPubAggKey {
			continue
		}

		header := ethereumtypes.Header{
			ConsensusUpdate: update.Data,
			TrustedSyncCommittee: ethereumtypes.TrustedSyncCommittee{
				TrustedSlot: trustedSlot,
				SyncCommittee: ethereumtypes.ActiveSyncCommittee{
					Next: &ethereumtypes.SyncCommittee{
						Pubkeys:         previousLightClientUpdate.Data.NextSyncCommittee.Pubkeys,
						AggregatePubkey: previousLightClientUpdate.Data.NextSyncCommittee.AggregatePubkey,
					},
				},
			},
			AccountUpdate: accountUpdate,
		}
		headers = append(headers, header)

		trustedSlot = update.Data.AttestedHeader.Beacon.GetSlot()
		prevPubAggKey = previousLightClientUpdate.Data.NextSyncCommittee.AggregatePubkey
	}

	if trustedPeriod >= targetPeriod {
		headers = []ethereumtypes.Header{}
	}

	wasmClientState, ethereumClientState := s.GetEthereumClientState(ctx, simd, s.EthereumLightClientID)
	_, ethereumConsensusState = s.GetEthereumConsensusState(ctx, simd, s.EthereumLightClientID, wasmClientState.LatestHeight)

	var updatedHeaders []ethereumtypes.Header
	for _, header := range headers {
		headerBz, err := json.Marshal(header)
		s.Require().NoError(err)

		wasmHeader := ibcwasmtypes.ClientMessage{
			Data: headerBz,
		}

		wasmHeaderAny, err := clienttypes.PackClientMessage(&wasmHeader)
		s.Require().NoError(err)
		_, err = s.BroadcastMessages(ctx, simd, simdRelayerUser, 500_000, &clienttypes.MsgUpdateClient{
			ClientId:      s.EthereumLightClientID,
			ClientMessage: wasmHeaderAny,
			Signer:        simdRelayerUser.FormattedAddress(),
		})
		s.Require().NoError(err)

		s.LastEtheruemLightClientUpdate = header.ConsensusUpdate.AttestedHeader.Beacon.GetSlot()
		fmt.Println("Updated eth light client to block number", s.LastEtheruemLightClientUpdate)

		updatedHeaders = append(updatedHeaders, header)

		time.Sleep(10 * time.Second)

		if s.LastEtheruemLightClientUpdate > updateTo {
			fmt.Println("Updated past target block number, skipping any further updates")
			break
		}
	}
	rustFixtureGenerator.AddFixtureStep("updated_light_client", ethereumtypes.UpdateClient{
		ClientState:    ethereumClientState,
		ConsensusState: ethereumConsensusState,
		Updates:        updatedHeaders,
	})

	s.Require().Greater(s.LastEtheruemLightClientUpdate, minimumUpdateTo)
}

func (s *TestSuite) createEthereumLightClient(
	ctx context.Context,
	simdRelayerUser ibc.Wallet,
	ibcContractAddress string,
	rustFixtureGenerator *types.RustFixtureGenerator,
) {
	eth, simd := s.EthChain, s.CosmosChains[0]

	file, err := os.Open("e2e/interchaintestv8/wasm/cw_ics08_wasm_eth.wasm.gz")
	s.Require().NoError(err)

	etheruemClientChecksum := s.PushNewWasmClientProposal(ctx, simd, simdRelayerUser, file)
	s.Require().NotEmpty(etheruemClientChecksum, "checksum was empty but should not have been")

	genesis, err := eth.BeaconAPIClient.GetGenesis()
	s.Require().NoError(err)
	spec, err := eth.BeaconAPIClient.GetSpec()
	s.Require().NoError(err)

	executionHeight, err := eth.BeaconAPIClient.GetExecutionHeight("finalized")
	s.Require().NoError(err)
	executionNumberHex := fmt.Sprintf("0x%x", executionHeight)

	ethClientState := ethereumtypes.ClientState{
		ChainID:                      eth.ChainID.Uint64(),
		GenesisValidatorsRoot:        ethcommon.Bytes2Hex(genesis.GenesisValidatorsRoot[:]),
		MinSyncCommitteeParticipants: 32,
		GenesisTime:                  uint64(genesis.GenesisTime.Unix()),
		ForkParameters:               spec.ToForkParameters(),
		SecondsPerSlot:               uint64(spec.SecondsPerSlot.Seconds()),
		SlotsPerEpoch:                spec.SlotsPerEpoch,
		EpochsPerSyncCommitteePeriod: spec.EpochsPerSyncCommitteePeriod,
		LatestSlot:                   executionHeight,
		FrozenSlot:                   0,
		IbcCommitmentSlot:            testvalues.IbcCommitmentSlotHex,
		IbcContractAddress:           ibcContractAddress,
	}

	ethClientStateBz, err := json.Marshal(&ethClientState)
	s.Require().NoError(err)
	wasmClientChecksum, err := hex.DecodeString(etheruemClientChecksum)
	s.Require().NoError(err)
	latestHeightSlot := clienttypes.Height{
		RevisionNumber: 0,
		RevisionHeight: executionHeight,
	}
	clientState := ibcwasmtypes.ClientState{
		Data:         ethClientStateBz,
		Checksum:     wasmClientChecksum,
		LatestHeight: latestHeightSlot,
	}
	clientStateAny, err := clienttypes.PackClientState(&clientState)
	s.Require().NoError(err)

	proofOfIBCContract, err := eth.EthAPI.GetProof(ibcContractAddress, []string{}, executionNumberHex)
	s.Require().NoError(err)

	header, err := eth.BeaconAPIClient.GetHeader(strconv.Itoa(int(executionHeight)))
	s.Require().NoError(err)
	bootstrap, err := eth.BeaconAPIClient.GetBootstrap(header.Root)
	s.Require().NoError(err)

	if bootstrap.Data.Header.Beacon.Slot != executionHeight {
		s.Require().Fail(fmt.Sprintf("creating client: expected exec height %d, to equal boostrap slot %d", executionHeight, bootstrap.Data.Header.Beacon.Slot))
	}

	timestamp := bootstrap.Data.Header.Execution.Timestamp * 1_000_000_000

	currentPeriod := executionHeight / spec.Period()
	clientUpdates, err := eth.BeaconAPIClient.GetLightClientUpdates(currentPeriod, 1)
	s.Require().NoError(err)
	s.Require().NotEmpty(clientUpdates)

	s.LastEtheruemLightClientUpdate = bootstrap.Data.Header.Beacon.Slot
	ethConsensusState := ethereumtypes.ConsensusState{
		Slot:                 bootstrap.Data.Header.Beacon.Slot,
		StateRoot:            bootstrap.Data.Header.Execution.StateRoot,
		StorageRoot:          proofOfIBCContract.StorageHash,
		Timestamp:            timestamp,
		CurrentSyncCommittee: bootstrap.Data.CurrentSyncCommittee.AggregatePubkey,
		NextSyncCommittee:    clientUpdates[0].Data.NextSyncCommittee.AggregatePubkey,
	}

	ethConsensusStateBz, err := json.Marshal(&ethConsensusState)
	s.Require().NoError(err)
	consensusState := ibcwasmtypes.ConsensusState{
		Data: ethConsensusStateBz,
	}
	consensusStateAny, err := clienttypes.PackConsensusState(&consensusState)
	s.Require().NoError(err)

	res, err := s.BroadcastMessages(ctx, simd, simdRelayerUser, 200_000, &clienttypes.MsgCreateClient{
		ClientState:    clientStateAny,
		ConsensusState: consensusStateAny,
		Signer:         simdRelayerUser.FormattedAddress(),
	})
	s.Require().NoError(err)

	s.EthereumLightClientID, err = ibctesting.ParseClientIDFromEvents(res.Events)
	s.Require().NoError(err)
	s.Require().Equal("08-wasm-0", s.EthereumLightClientID)

	rustFixtureGenerator.AddFixtureStep("initial_state", ethereumtypes.InitialState{
		ClientState:    ethClientState,
		ConsensusState: ethConsensusState,
	})
}

func (s *TestSuite) createDummyLightClient(ctx context.Context, simdRelayerUser ibc.Wallet) {
	eth, simd := s.EthChain, s.CosmosChains[0]

	file, err := os.Open("e2e/interchaintestv8/wasm/wasm_dummy_light_client.wasm.gz")
	s.Require().NoError(err)

	dummyClientChecksum := s.PushNewWasmClientProposal(ctx, simd, simdRelayerUser, file)
	s.Require().NotEmpty(dummyClientChecksum, "checksum was empty but should not have been")

	_, ethHeight, err := eth.EthAPI.GetBlockNumber()
	s.Require().NoError(err)

	wasmClientChecksum, err := hex.DecodeString(dummyClientChecksum)
	s.Require().NoError(err)
	latestHeight := clienttypes.Height{
		RevisionNumber: 0,
		RevisionHeight: ethHeight,
	}
	s.Require().NoError(err)
	clientState := ibcwasmtypes.ClientState{
		Data:         []byte("doesnt matter"),
		Checksum:     wasmClientChecksum,
		LatestHeight: latestHeight,
	}
	clientStateAny, err := clienttypes.PackClientState(&clientState)
	s.Require().NoError(err)

	consensusState := ibcwasmtypes.ConsensusState{
		Data: []byte("doesnt matter"),
	}
	consensusStateAny, err := clienttypes.PackConsensusState(&consensusState)
	s.Require().NoError(err)

	res, err := s.BroadcastMessages(ctx, simd, simdRelayerUser, 200_000, &clienttypes.MsgCreateClient{
		ClientState:    clientStateAny,
		ConsensusState: consensusStateAny,
		Signer:         simdRelayerUser.FormattedAddress(),
	})
	s.Require().NoError(err)

	s.EthereumLightClientID, err = ibctesting.ParseClientIDFromEvents(res.Events)
	s.Require().NoError(err)
	s.Require().Equal("08-wasm-0", s.EthereumLightClientID)
}
