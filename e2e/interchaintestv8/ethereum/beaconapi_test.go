package ethereum_test

import (
	"context"
	"testing"
	"time"

	"github.com/stretchr/testify/require"

	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/ethereum"
)

func TestBeacon(t *testing.T) {
	ctx := context.Background()

	eth, err := ethereum.SpinUpEthereum(ctx)
	require.NoError(t, err)
	t.Cleanup(func() {
		eth.Destroy(ctx)
	})

	blockNumberHex, blockNumber, err := eth.EthAPI.GetBlockNumber()
	require.NoError(t, err)

	time.Sleep(30 * time.Second)

	genesis, err := eth.BeaconAPIClient.GetGenesis()
	require.NoError(t, err)
	require.NotEmpty(t, genesis)

	spec, err := eth.BeaconAPIClient.GetSpec()
	require.NoError(t, err)
	forkParams := spec.ToForkParameters()

	require.NotEmpty(t, spec.SecondsPerSlot)
	require.NotEmpty(t, spec.SlotsPerEpoch)
	require.NotEmpty(t, spec.EpochsPerSyncCommitteePeriod)

	require.NotEmpty(t, forkParams.GenesisForkVersion)
	require.NotEmpty(t, forkParams.Altair.Version)
	require.NotEmpty(t, forkParams.Bellatrix.Version)
	require.NotEmpty(t, forkParams.Capella.Version)
	require.NotEmpty(t, forkParams.Deneb.Version)

	header, err := eth.BeaconAPIClient.GetHeader("")
	require.NoError(t, err)

	bootstrap, err := eth.BeaconAPIClient.GetBootstrap(header.Root)
	require.NoError(t, err)
	require.NotEmpty(t, bootstrap.Data.CurrentSyncCommittee)
	require.NotEmpty(t, bootstrap.Data.Header.Execution.StateRoot)

	finalityUpdate, err := eth.BeaconAPIClient.GetFinalityUpdate()
	require.NoError(t, err)
	require.NotEmpty(t, finalityUpdate)

	executionHeight, err := eth.BeaconAPIClient.GetExecutionHeight(blockNumberHex)
	require.NoError(t, err)
	require.NotEmpty(t, executionHeight)

	currentPeriod := uint64(blockNumber) / spec.Period()
	clientUpdates, err := eth.BeaconAPIClient.GetLightClientUpdates(currentPeriod, 1)
	require.NoError(t, err)
	require.NotEmpty(t, clientUpdates)
}
