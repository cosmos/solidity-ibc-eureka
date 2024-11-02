package e2esuite

import (
	"context"
	"encoding/hex"
	"fmt"
	"os"
	"strconv"
	"time"

	ethcommon "github.com/ethereum/go-ethereum/common"

	ibcwasmtypes "github.com/cosmos/ibc-go/modules/light-clients/08-wasm/types"
	clienttypes "github.com/cosmos/ibc-go/v8/modules/core/02-client/types"
	mock "github.com/cosmos/ibc-go/v8/modules/light-clients/00-mock"
	ibctesting "github.com/cosmos/ibc-go/v8/testing"

	"github.com/strangelove-ventures/interchaintest/v8/ibc"
	"github.com/strangelove-ventures/interchaintest/v8/testutil"

	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/ethereum"
	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/testvalues"
	ethereumligthclient "github.com/srdtrk/solidity-ibc-eureka/e2e/v8/types/ethereumlightclient"
)

func (s *TestSuite) CreateEthereumLightClient(ctx context.Context, simdRelayerUser ibc.Wallet, ibcContractAddress string) {
	switch s.ethTestnetType {
	case testvalues.EthTestnetTypePoW:
		s.createDummyLightClient(ctx, simdRelayerUser)
	case testvalues.EthTestnetTypePoS:
		s.createUnionLightClient(ctx, simdRelayerUser, ibcContractAddress)
	default:
		panic(fmt.Sprintf("Unrecognized Ethereum testnet type: %v", s.ethTestnetType))
	}
}

func (s *TestSuite) UpdateEthClient(ctx context.Context, ibcContractAddress string, minimumUpdateTo int64, simdRelayerUser ibc.Wallet) {
	if s.ethTestnetType != testvalues.EthTestnetTypePoS {
		return
	}

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
	fmt.Printf("Updating eth light client to at least block number %d (with minimum requested: %d)\n", updateTo, minimumUpdateTo)

	_, unionConsensusState := s.GetUnionConsensusState(ctx, simd, s.EthereumLightClientID, clienttypes.Height{
		RevisionNumber: 0,
		RevisionHeight: s.LastEtheruemLightClientUpdate,
	})
	spec, err := eth.BeaconAPIClient.GetSpec()
	s.Require().NoError(err)

	trustedPeriod := unionConsensusState.Slot / spec.Period()

	var finalityUpdate ethereum.FinalityUpdateJSONResponse
	var targetPeriod uint64
	// Now we need to wait for finalization and light client update availability to be after updateTo
	err = testutil.WaitForCondition(8*time.Minute, 5*time.Second, func() (bool, error) {
		finalityUpdate, err = eth.BeaconAPIClient.GetFinalityUpdate()
		s.Require().NoError(err)

		targetPeriod = finalityUpdate.Data.AttestedHeader.Beacon.Slot / spec.Period()

		lightClientUpdates, err := eth.BeaconAPIClient.GetLightClientUpdates(trustedPeriod+1, targetPeriod-trustedPeriod)
		s.Require().NoError(err)
		var highestUpdateSlot uint64
		for _, update := range lightClientUpdates {
			if update.Data.AttestedHeader.Beacon.Slot > highestUpdateSlot {
				highestUpdateSlot = update.Data.AttestedHeader.Beacon.Slot
			}
		}

		return finalityUpdate.Data.FinalizedHeader.Beacon.Slot > uint64(updateTo) &&
				targetPeriod > trustedPeriod &&
				highestUpdateSlot > uint64(updateTo),
			nil
	})
	s.Require().NoError(err)

	_, unionClientState := s.GetUnionClientState(ctx, simd, s.EthereumLightClientID)
	// Wait until computed slot is greater than all of the updates signature slots
	err = testutil.WaitForCondition(8*time.Minute, 5*time.Second, func() (bool, error) {
		lightClientUpdates, err := eth.BeaconAPIClient.GetLightClientUpdates(trustedPeriod+1, targetPeriod-trustedPeriod)
		s.Require().NoError(err)

		computedSlot := (uint64(time.Now().Unix())-unionClientState.GenesisTime)/
			unionClientState.SecondsPerSlot + spec.GenesisSlot

		for _, update := range lightClientUpdates {
			if computedSlot < update.Data.SignatureSlot {
				return false, nil
			}
		}

		return len(lightClientUpdates) >= int(targetPeriod-trustedPeriod), nil
	})
	s.Require().NoError(err)

	s.Require().Greater(targetPeriod, trustedPeriod)

	lightClientUpdates, err := eth.BeaconAPIClient.GetLightClientUpdates(trustedPeriod+1, targetPeriod-trustedPeriod)
	s.Require().NoError(err)

	headers := []ethereumligthclient.Header{}
	trustedSlot := unionConsensusState.Slot
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
		proofResp, err := eth.EthAPI.GetProof(ibcContractAddress, []string{}, executionHeightHex)
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
					RevisionHeight: trustedSlot,
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

		trustedSlot = update.Data.AttestedHeader.Beacon.Slot
		prevPubAggKey = previousLightClientUpdate.Data.NextSyncCommittee.AggregatePubkey
	}

	if trustedPeriod >= targetPeriod {
		headers = []ethereumligthclient.Header{}
	}

	wasmClientState, _ := s.GetUnionClientState(ctx, simd, s.EthereumLightClientID)
	_, unionConsensusState = s.GetUnionConsensusState(ctx, simd, s.EthereumLightClientID, wasmClientState.LatestHeight)

	for _, header := range headers {
		logHeader("Updating eth light client", header)
		headerBz := simd.Config().EncodingConfig.Codec.MustMarshal(&header)
		wasmHeader := ibcwasmtypes.ClientMessage{
			Data: headerBz,
		}

		wasmHeaderAny, err := clienttypes.PackClientMessage(&wasmHeader)
		s.Require().NoError(err)
		_, err = s.BroadcastMessages(ctx, simd, simdRelayerUser, 200_000, &clienttypes.MsgUpdateClient{
			ClientId:      s.EthereumLightClientID,
			ClientMessage: wasmHeaderAny,
			Signer:        simdRelayerUser.FormattedAddress(),
		})
		s.Require().NoError(err)

		s.LastEtheruemLightClientUpdate = header.ConsensusUpdate.AttestedHeader.Beacon.Slot
		fmt.Println("Updated eth light client to block number", s.LastEtheruemLightClientUpdate)
		time.Sleep(10 * time.Second)

		if s.LastEtheruemLightClientUpdate > uint64(updateTo) {
			fmt.Println("Updated past target block number, skipping any further updates")
			break
		}
	}

	s.Require().Greater(s.LastEtheruemLightClientUpdate, uint64(minimumUpdateTo))
}

func (s *TestSuite) createUnionLightClient(ctx context.Context, simdRelayerUser ibc.Wallet, ibcContractAddress string) {
	eth, simd := s.ChainA, s.ChainB

	file, err := os.Open("e2e/interchaintestv8/wasm/ethereum_light_client_minimal.wasm.gz")
	s.Require().NoError(err)

	unionClientChecksum := s.PushNewWasmClientProposal(ctx, simd, simdRelayerUser, file)
	s.Require().NotEmpty(unionClientChecksum, "checksum was empty but should not have been")

	genesis, err := eth.BeaconAPIClient.GetGenesis()
	s.Require().NoError(err)
	spec, err := eth.BeaconAPIClient.GetSpec()
	s.Require().NoError(err)

	executionHeight, err := eth.BeaconAPIClient.GetExecutionHeight("finalized")
	s.Require().NoError(err)
	executionNumberHex := fmt.Sprintf("0x%x", executionHeight)

	ibcCommitmentSlot := ethereum.HexToBeBytes(testvalues.IbcCommitmentSlotHex)

	ethClientState := ethereumligthclient.ClientState{
		ChainId:                      eth.ChainID.String(),
		GenesisValidatorsRoot:        genesis.GenesisValidatorsRoot[:],
		MinSyncCommitteeParticipants: 0,
		GenesisTime:                  uint64(genesis.GenesisTime.Unix()),
		ForkParameters:               spec.ToForkParameters(),
		SecondsPerSlot:               uint64(spec.SecondsPerSlot.Seconds()),
		SlotsPerEpoch:                spec.SlotsPerEpoch,
		EpochsPerSyncCommitteePeriod: spec.EpochsPerSyncCommitteePeriod,
		LatestSlot:                   executionHeight,
		FrozenHeight: &clienttypes.Height{
			RevisionNumber: 0,
			RevisionHeight: 0,
		},
		IbcCommitmentSlot:  ibcCommitmentSlot,
		IbcContractAddress: ethcommon.FromHex(ibcContractAddress),
	}

	ethClientStateBz := simd.Config().EncodingConfig.Codec.MustMarshal(&ethClientState)
	wasmClientChecksum, err := hex.DecodeString(unionClientChecksum)
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
	stateRoot := ethereum.HexToBeBytes(bootstrap.Data.Header.Execution.StateRoot)

	currentPeriod := executionHeight / spec.Period()
	clientUpdates, err := eth.BeaconAPIClient.GetLightClientUpdates(currentPeriod, 1)
	s.Require().NoError(err)
	s.Require().NotEmpty(clientUpdates)

	s.LastEtheruemLightClientUpdate = bootstrap.Data.Header.Beacon.Slot
	ethConsensusState := ethereumligthclient.ConsensusState{
		Slot:                 bootstrap.Data.Header.Beacon.Slot,
		StateRoot:            stateRoot,
		StorageRoot:          ethereum.HexToBeBytes(proofOfIBCContract.StorageHash),
		Timestamp:            timestamp,
		CurrentSyncCommittee: ethcommon.FromHex(bootstrap.Data.CurrentSyncCommittee.AggregatePubkey),
		NextSyncCommittee:    ethcommon.FromHex(clientUpdates[0].Data.NextSyncCommittee.AggregatePubkey),
	}

	ethConsensusStateBz := simd.Config().EncodingConfig.Codec.MustMarshal(&ethConsensusState)
	consensusState := ibcwasmtypes.ConsensusState{
		Data: ethConsensusStateBz,
	}
	consensusStateAny, err := clienttypes.PackConsensusState(&consensusState)
	s.Require().NoError(err)

	res, err := s.BroadcastMessages(ctx, simd, simdRelayerUser, 200_000, &clienttypes.MsgCreateClient{
		ClientState:      clientStateAny,
		ConsensusState:   consensusStateAny,
		Signer:           simdRelayerUser.FormattedAddress(),
		CounterpartyId:   "",
		MerklePathPrefix: nil,
	})
	s.Require().NoError(err)

	s.EthereumLightClientID, err = ibctesting.ParseClientIDFromEvents(res.Events)
	s.Require().NoError(err)
	s.Require().Equal("08-wasm-0", s.EthereumLightClientID)
}

func (s *TestSuite) createDummyLightClient(ctx context.Context, simdRelayerUser ibc.Wallet) {
	eth, simd := s.ChainA, s.ChainB

	ethHeight, err := eth.Height()
	s.Require().NoError(err)
	s.Require().NotZero(ethHeight)

	clientState := mock.ClientState{
		LatestHeight: clienttypes.NewHeight(1, uint64(ethHeight)),
	}
	clientStateAny, err := clienttypes.PackClientState(&clientState)
	s.Require().NoError(err)
	consensusState := mock.ConsensusState{
		Timestamp: uint64(time.Now().UnixNano()),
	}
	consensusStateAny, err := clienttypes.PackConsensusState(&consensusState)
	s.Require().NoError(err)

	res, err := s.BroadcastMessages(ctx, simd, simdRelayerUser, 200_000, &clienttypes.MsgCreateClient{
		ClientState:      clientStateAny,
		ConsensusState:   consensusStateAny,
		Signer:           simdRelayerUser.FormattedAddress(),
		CounterpartyId:   "",
		MerklePathPrefix: nil,
	})
	s.Require().NoError(err)

	s.EthereumLightClientID, err = ibctesting.ParseClientIDFromEvents(res.Events)
	s.Require().NoError(err)
	s.Require().Equal("00-mock-0", s.EthereumLightClientID)
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
