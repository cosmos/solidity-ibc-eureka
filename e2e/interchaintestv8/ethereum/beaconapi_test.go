package ethereum_test

import (
	"context"
	"fmt"
	"testing"

	"github.com/ethereum/go-ethereum/common"
	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/ethereum"
	"github.com/stretchr/testify/require"
)

func TestBeacon(t *testing.T) {
	ctx := context.Background()

	eth, err := ethereum.SpinUpEthereum(ctx)
	require.NoError(t, err)
	t.Cleanup(func() {
		eth.Destroy(ctx)
	})

	_, blockNumber, err := eth.EthAPI.GetBlockNumber()
	require.NoError(t, err)

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
	fmt.Println("GenesisForkVersion", common.Bytes2Hex(forkParams.GenesisForkVersion))
	require.NotEmpty(t, forkParams.Altair.Version)
	require.NotEmpty(t, forkParams.Bellatrix.Version)
	require.NotEmpty(t, forkParams.Capella.Version)
	require.NotEmpty(t, forkParams.Deneb.Version)

	header, err := eth.BeaconAPIClient.GetHeader(blockNumber)
	require.NoError(t, err)
	bootstrap, err := eth.BeaconAPIClient.GetBootstrap(header.Root)
	require.NoError(t, err)
	require.NotEmpty(t, bootstrap.Data.CurrentSyncCommittee)
	require.NotEmpty(t, bootstrap.Data.Header.Beacon.Slot)
	require.NotEmpty(t, bootstrap.Data.Header.Execution.StateRoot)
	require.NotEmpty(t, bootstrap.Data.Header.Execution.BlockNumber)
	require.NotEmpty(t, bootstrap.Data.Header.Execution.Timestamp)
}
