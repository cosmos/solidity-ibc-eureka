package main

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"net/http"
	"strconv"
	"testing"
	"time"

	ibchost "github.com/cosmos/ibc-go/v8/modules/core/24-host"

	eth2client "github.com/attestantio/go-eth2-client"
	"github.com/attestantio/go-eth2-client/api"
	apiv1 "github.com/attestantio/go-eth2-client/api/v1"
	ethttp "github.com/attestantio/go-eth2-client/http"
	"github.com/attestantio/go-eth2-client/spec/phase0"
	"github.com/ethereum/go-ethereum/accounts/abi/bind"
	"github.com/ethereum/go-ethereum/common"
	"github.com/ethereum/go-ethereum/crypto"
	"github.com/ethereum/go-ethereum/ethclient"
	"github.com/rs/zerolog"
	ethereumligthclient "github.com/srdtrk/solidity-ibc-eureka/e2e/v8/types/ethereumlightclient"
	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/types/ics26router"
	"github.com/stretchr/testify/require"
)

type BeaconAPIClient struct {
	ctx    context.Context
	cancel context.CancelFunc

	client eth2client.Service
	url    string
}

type Spec struct {
	SecondsPerSlot               time.Duration `json:"SECONDS_PER_SLOT"`
	SlotsPerEpoch                uint64        `json:"SLOTS_PER_EPOCH"`
	EpochsPerSyncCommitteePeriod uint64        `json:"EPOCHS_PER_SYNC_COMMITTEE_PERIOD"`

	// Fork Parameters
	GenesisForkVersion   phase0.Version `json:"GENESIS_FORK_VERSION"`
	GenesisSlot          uint64         `json:"GENESIS_SLOT"`
	AltairForkVersion    phase0.Version `json:"ALTAIR_FORK_VERSION"`
	AltairForkEpoch      uint64         `json:"ALTAIR_FORK_EPOCH"`
	BellatrixForkVersion phase0.Version `json:"BELLATRIX_FORK_VERSION"`
	BellatrixForkEpoch   uint64         `json:"BELLATRIX_FORK_EPOCH"`
	CapellaForkVersion   phase0.Version `json:"CAPELLA_FORK_VERSION"`
	CapellaForkEpoch     uint64         `json:"CAPELLA_FORK_EPOCH"`
	DenebForkVersion     phase0.Version `json:"DENEB_FORK_VERSION"`
	DenebForkEpoch       uint64         `json:"DENEB_FORK_EPOCH"`
}

type Bootstrap struct {
	Data struct {
		Header               BootstrapHeader `json:"header"`
		CurrentSyncCommittee SyncCommittee   `json:"current_sync_committee"`
	} `json:"data"`
}

type BootstrapHeader struct {
	Beacon struct {
		Slot uint64 `json:"slot,string"`
	} `json:"beacon"`
	Execution struct {
		Timestamp   uint64 `json:"timestamp,string"`
		StateRoot   string `json:"state_root"`
		BlockNumber uint64 `json:"block_number,string"`
	} `json:"execution"`
}

type SyncCommittee struct {
	AggregatePubkey string `json:"aggregate_pubkey"`
}

func (s Spec) ToForkParameters() *ethereumligthclient.ForkParameters {
	return &ethereumligthclient.ForkParameters{
		GenesisForkVersion: s.GenesisForkVersion[:],
		GenesisSlot:        s.GenesisSlot,
		Altair: &ethereumligthclient.Fork{
			Version: s.AltairForkVersion[:],
			Epoch:   s.AltairForkEpoch,
		},
		Bellatrix: &ethereumligthclient.Fork{
			Version: s.BellatrixForkVersion[:],
			Epoch:   s.BellatrixForkEpoch,
		},
		Capella: &ethereumligthclient.Fork{
			Version: s.CapellaForkVersion[:],
			Epoch:   s.CapellaForkEpoch,
		},
		Deneb: &ethereumligthclient.Fork{
			Version: s.DenebForkVersion[:],
			Epoch:   s.DenebForkEpoch,
		},
	}
}

func (s Spec) Period() uint64 {
	return s.EpochsPerSyncCommitteePeriod * s.SlotsPerEpoch
}

func (b BeaconAPIClient) Close() {
	b.cancel()
}

func NewBeaconAPIClient(beaconAPIAddress string) BeaconAPIClient {
	ctx, cancel := context.WithCancel(context.Background())
	client, err := ethttp.New(ctx,
		// WithAddress supplies the address of the beacon node, as a URL.
		ethttp.WithAddress(beaconAPIAddress),
		// LogLevel supplies the level of logging to carry out.
		ethttp.WithLogLevel(zerolog.WarnLevel),
	)
	if err != nil {
		panic(err)
	}

	return BeaconAPIClient{
		ctx:    ctx,
		cancel: cancel,
		client: client,
		url:    beaconAPIAddress,
	}
}

// TODO: Add errors
func (b BeaconAPIClient) GetHeader(block string) *apiv1.BeaconBlockHeader {
	if provider, isProvider := b.client.(eth2client.BeaconBlockHeadersProvider); isProvider {
		headerResponse, err := provider.BeaconBlockHeader(b.ctx, &api.BeaconBlockHeaderOpts{
			Block: block,
		})

		if err != nil {
			// Errors may be API errors, in which case they will have more detail
			// about the failure.
			var apiErr *api.Error
			if errors.As(err, &apiErr) {
				switch apiErr.StatusCode {
				case 404:
					panic("genesis not found")
				case 503:
					panic("node is syncing")
				}
			}
			panic(err)
		}
		return headerResponse.Data
	}

	panic("no provider for block header")
}

// TODO: Add errors
func (b BeaconAPIClient) GetBootstrap(finalizedRoot phase0.Root) Bootstrap {
	finalizedRootStr := finalizedRoot.String()
	url := fmt.Sprintf("%s/eth/v1/beacon/light_client/bootstrap/%s", b.url, finalizedRootStr)
	req, err := http.NewRequest("GET", url, nil)
	if err != nil {
		panic(err)
	}
	req.Header.Set("Accept", "application/json")

	resp, err := http.DefaultClient.Do(req)
	if err != nil {
		panic(err)
	}
	defer resp.Body.Close()

	body, err := io.ReadAll(resp.Body)
	if err != nil {
		panic(err)
	}

	if resp.StatusCode != 200 {
		panic(fmt.Sprintf("Get bootstrap (%s) failed with status code: %d, body: %s", url, resp.StatusCode, body))
	}

	var bootstrap Bootstrap
	if err := json.Unmarshal(body, &bootstrap); err != nil {
		panic(err)
	}

	return bootstrap
}

type LightClientUpdate struct {
	Data struct {
		NextSyncCommittee SyncCommittee `json:"next_sync_committee"`
	} `json:"data"`
}

type LightClientUpdatesResponse []LightClientUpdate

// TODO: Add errors
func (b BeaconAPIClient) GetLightClientUpdates(startPeriod uint64, count uint64) LightClientUpdatesResponse {
	url := fmt.Sprintf("%s/eth/v1/beacon/light_client/updates?start_period=%d&count=%d", b.url, startPeriod, count)
	req, err := http.NewRequest("GET", url, nil)
	if err != nil {
		panic(err)
	}
	req.Header.Set("Accept", "application/json")

	resp, err := http.DefaultClient.Do(req)
	if err != nil {
		panic(err)
	}
	defer resp.Body.Close()

	body, err := io.ReadAll(resp.Body)
	if err != nil {
		panic(err)
	}

	var lightClientUpdatesResponse LightClientUpdatesResponse
	if err := json.Unmarshal(body, &lightClientUpdatesResponse); err != nil {
		panic(err)
	}

	return lightClientUpdatesResponse
}

// TODO: Add errors
func (b BeaconAPIClient) GetGenesis() *apiv1.Genesis {
	if provider, isProvider := b.client.(eth2client.GenesisProvider); isProvider {
		genesisResponse, err := provider.Genesis(b.ctx, &api.GenesisOpts{})
		if err != nil {
			// Errors may be API errors, in which case they will have more detail
			// about the failure.
			var apiErr *api.Error
			if errors.As(err, &apiErr) {
				switch apiErr.StatusCode {
				case 404:
					panic("genesis not found")
				case 503:
					panic("node is syncing")
				}
			}
			panic(err)
		}
		return genesisResponse.Data
	}

	panic("No provider for genesis!")
}

// TODO: Add errors
func (b BeaconAPIClient) GetSpec() Spec {
	if provider, isProvider := b.client.(eth2client.SpecProvider); isProvider {
		specResponse, err := provider.Spec(b.ctx, &api.SpecOpts{})
		if err != nil {
			// Errors may be API errors, in which case they will have more detail
			// about the failure.
			var apiErr *api.Error
			if errors.As(err, &apiErr) {
				switch apiErr.StatusCode {
				case 404:
					panic("spec not found")
				case 503:
					panic("node is syncing")
				}
			}
			panic(err)
		}

		specJsonBz, err := json.Marshal(specResponse.Data)
		if err != nil {
			panic(err)
		}
		var spec Spec
		if err := json.Unmarshal(specJsonBz, &spec); err != nil {
			panic(err)
		}

		return spec
	}

	panic("no provider for spec!")
}

type EthGetProofResponse struct {
	StorageHash  string `json:"storageHash"`
	StorageProof []struct {
		Key   string   `json:"key"`
		Proof []string `json:"proof"`
		Value string   `json:"value"`
	} `json:"storageProof"`
}

// TODO: Add errors
func GetProof(ethClient *ethclient.Client, address string, storageKeys []string, blockHex string) EthGetProofResponse {
	var proofResponse EthGetProofResponse
	if err := ethClient.Client().Call(&proofResponse, "eth_getProof", address, storageKeys, blockHex); err != nil {
		panic(err)
	}

	return proofResponse
}

// From https://medium.com/@zhuytt4/verify-the-owner-of-safe-wallet-with-eth-getproof-7edc450504ff
func getStorageKey(path string) common.Hash {
	// Storage slot for the balances mapping is typically 0
	storageSlot := common.Hex2Bytes("0x0")

	pathHash := crypto.Keccak256([]byte(path))

	// zero pad both to 32 bytes
	paddedSlot := common.LeftPadBytes(storageSlot, 32)

	fmt.Println("path", path)
	fmt.Println("Paddedslot", common.Bytes2Hex(paddedSlot))
	fmt.Println("PathHash", common.Bytes2Hex(pathHash))

	// keccak256(h(k) . slot)
	return crypto.Keccak256Hash(pathHash, paddedSlot)
}

func TestBeacon(t *testing.T) {
	// ethClient, err := ethclient.Dial(ethRPC)
	ethClient, err := ethclient.Dial("")
	require.NoError(t, err)
	var blockNumberHex string
	err = ethClient.Client().Call(&blockNumberHex, "eth_blockNumber")
	require.NoError(t, err)

	ics26RouterContract, err := ics26router.NewContract(common.HexToAddress(ics26RouterAddress), ethClient)
	require.NoError(t, err)

	time.Sleep(10 * time.Second) // Just to give time to settle, some calls might fail otherwise

	//beaconAPIClient := NewBeaconAPIClient(beaconRPC)
	beaconAPIClient := NewBeaconAPIClient("")
	genesis := beaconAPIClient.GetGenesis()
	require.NotEmpty(t, genesis)

	spec := beaconAPIClient.GetSpec()
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

	blockNumber, err := strconv.ParseInt(blockNumberHex, 0, 0)
	fmt.Printf("Block: %d\n", blockNumber)
	require.NoError(t, err)
	header := beaconAPIClient.GetHeader(strconv.Itoa(int(blockNumber)))
	bootstrap := beaconAPIClient.GetBootstrap(header.Root)
	require.NotEmpty(t, bootstrap.Data.CurrentSyncCommittee)
	require.NotEmpty(t, bootstrap.Data.Header.Beacon.Slot)
	require.NotEmpty(t, bootstrap.Data.Header.Execution.StateRoot)
	require.NotEmpty(t, bootstrap.Data.Header.Execution.BlockNumber)
	require.NotEmpty(t, bootstrap.Data.Header.Execution.Timestamp)

	// eth_getProof
	packetCommitmentPath := ibchost.PacketCommitmentPath("testport", "test-channel-0", 1)

	// "{\"ack raw\":\"test-ack\",\"channelId\":\"test-channel-1\",\"commitement hex\":\"0x6100b0115958fd2814360ce18f99b63e26266265f2a258a4191c11231c098c27\",\"erc20\":\
	// "0x1c3b44601bb528c1cf0812397a70b9330864e996\",\"ics02Client\":\"0x9d86dbccdf537f0a0baf43160d2ef1570d84e358\",\"ics20Transfer\":\"0x2d93c2a44e7b33abfaa3f0c3353c7dfe266736d5\
	// ",\"ics26Router\":\"0xc3536f63ab92bc7902db5d57926c80f933121bca\",\"path hex\":\"0x04314c66dd5927303c5b1c010b29d3044d619ac94572d759d2c69e81da573842\",\"portId\":\"testport\"
	// ,\"sequence\":\"1\"}"

	pathHash := crypto.Keccak256([]byte(packetCommitmentPath))
	var pathHash32 [32]byte
	copy(pathHash32[:], pathHash)

	committment, err := ics26RouterContract.GetCommitment(&bind.CallOpts{}, pathHash32)
	require.NoError(t, err)
	fmt.Println("pathHash", common.Bytes2Hex(pathHash))
	fmt.Println("committment", common.Bytes2Hex(committment[:]))

	packetStorageKey := getStorageKey(packetCommitmentPath)
	storageKeys := []string{packetStorageKey.Hex()}

	proofResp := GetProof(ethClient, ics26RouterAddress, storageKeys, "latest")
	fmt.Printf("ProofResp: %+v\n", proofResp)
	require.NotEmpty(t, proofResp.StorageHash)
	require.Len(t, proofResp.StorageProof, 1)
	require.NotEmpty(t, proofResp.StorageProof[0].Key)
	require.NotEmpty(t, proofResp.StorageProof[0].Proof)
	require.NotEmpty(t, proofResp.StorageProof[0].Value)
	require.NotEqual(t, "0x0", proofResp.StorageProof[0].Value)

	fmt.Printf("Block number: %d, period: %d\n", blockNumber, spec.Period())
	currentPeriod := uint64(blockNumber) / spec.Period()
	clientUpdates := beaconAPIClient.GetLightClientUpdates(currentPeriod, 1)
	require.Len(t, clientUpdates, 1)
	require.NotEmpty(t, clientUpdates[0].Data.NextSyncCommittee)

	t.Cleanup(func() {
		beaconAPIClient.Close()
	})
}
