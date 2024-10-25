package ethereum

import (
	"context"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"time"

	eth2client "github.com/attestantio/go-eth2-client"
	"github.com/attestantio/go-eth2-client/api"
	apiv1 "github.com/attestantio/go-eth2-client/api/v1"
	ethttp "github.com/attestantio/go-eth2-client/http"
	"github.com/attestantio/go-eth2-client/spec/phase0"
	"github.com/rs/zerolog"

	ethereumlightclient "github.com/srdtrk/solidity-ibc-eureka/e2e/v8/types/ethereumlightclient"
)

type BeaconAPIClient struct {
	ctx    context.Context
	cancel context.CancelFunc

	client eth2client.Service
	url    string

	Retries   int
	RetryWait time.Duration
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
	Pubkeys         []string `json:"pubkeys"`
	AggregatePubkey string   `json:"aggregate_pubkey"`
}

type LightClientUpdatesResponse []LightClientUpdateJSON

//	pub struct UnboundedLightClientFinalityUpdate {
//	    /// Header attested to by the sync committee
//	    pub attested_header: UnboundedLightClientHeader,
//	    /// Finalized header corresponding to `attested_header.state_root`
//	    pub finalized_header: UnboundedLightClientHeader,
//	    pub finality_branch: [H256; floorlog2(FINALIZED_ROOT_INDEX)],
//	    /// Sync committee aggregate signature
//	    pub sync_aggregate: UnboundedSyncAggregate,
//	    /// Slot at which the aggregate signature was created (untrusted)
//	    #[serde(with = "::serde_utils::string")]
//	    pub signature_slot: u64,
//	}

func (s Spec) ToForkParameters() *ethereumlightclient.ForkParameters {
	return &ethereumlightclient.ForkParameters{
		GenesisForkVersion: s.GenesisForkVersion[:],
		GenesisSlot:        s.GenesisSlot,
		Altair: &ethereumlightclient.Fork{
			Version: s.AltairForkVersion[:],
			Epoch:   s.AltairForkEpoch,
		},
		Bellatrix: &ethereumlightclient.Fork{
			Version: s.BellatrixForkVersion[:],
			Epoch:   s.BellatrixForkEpoch,
		},
		Capella: &ethereumlightclient.Fork{
			Version: s.CapellaForkVersion[:],
			Epoch:   s.CapellaForkEpoch,
		},
		Deneb: &ethereumlightclient.Fork{
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
		ctx:       ctx,
		cancel:    cancel,
		client:    client,
		url:       beaconAPIAddress,
		Retries:   6,
		RetryWait: 10 * time.Second,
	}
}

func retry[T any](retries int, waitTime time.Duration, fn func() (T, error)) (T, error) {
	var err error
	var result T
	for i := 0; i < retries; i++ {
		result, err = fn()
		if err == nil {
			return result, nil
		}

		fmt.Printf("Retrying for %T: %s in %f seconds\n", result, err, waitTime.Seconds())
		time.Sleep(waitTime)
	}
	return result, err
}

func (b BeaconAPIClient) GetHeader(blockID string) (*apiv1.BeaconBlockHeader, error) {
	return retry(b.Retries, b.RetryWait, func() (*apiv1.BeaconBlockHeader, error) {
		headerResponse, err := b.client.(eth2client.BeaconBlockHeadersProvider).BeaconBlockHeader(b.ctx, &api.BeaconBlockHeaderOpts{
			Block: blockID,
		})
		if err != nil {
			return nil, err
		}

		return headerResponse.Data, nil
	})
}

func (b BeaconAPIClient) GetBootstrap(finalizedRoot phase0.Root) (Bootstrap, error) {
	return retry(b.Retries, b.RetryWait, func() (Bootstrap, error) {
		finalizedRootStr := finalizedRoot.String()
		url := fmt.Sprintf("%s/eth/v1/beacon/light_client/bootstrap/%s", b.url, finalizedRootStr)
		req, err := http.NewRequest("GET", url, nil)
		if err != nil {
			return Bootstrap{}, err
		}
		req.Header.Set("Accept", "application/json")
		resp, err := http.DefaultClient.Do(req)
		if err != nil {
			return Bootstrap{}, err
		}
		defer resp.Body.Close()

		body, err := io.ReadAll(resp.Body)
		if err != nil {
			return Bootstrap{}, err
		}

		if resp.StatusCode != 200 {
			return Bootstrap{}, fmt.Errorf("Get bootstrap (%s) failed with status code: %d, body: %s", url, resp.StatusCode, body)
		}

		var bootstrap Bootstrap
		if err := json.Unmarshal(body, &bootstrap); err != nil {
			return Bootstrap{}, err
		}

		return bootstrap, nil
	})
}

func (b BeaconAPIClient) GetLightClientUpdates(startPeriod uint64, count uint64) (LightClientUpdatesResponse, error) {
	return retry(b.Retries, b.RetryWait, func() (LightClientUpdatesResponse, error) {
		url := fmt.Sprintf("%s/eth/v1/beacon/light_client/updates?start_period=%d&count=%d", b.url, startPeriod, count)
		fmt.Println("light_client_updates url:", url)
		req, err := http.NewRequest("GET", url, nil)
		if err != nil {
			return LightClientUpdatesResponse{}, err
		}
		req.Header.Set("Accept", "application/json")

		resp, err := http.DefaultClient.Do(req)
		if err != nil {
			return LightClientUpdatesResponse{}, err
		}
		defer resp.Body.Close()

		body, err := io.ReadAll(resp.Body)
		if err != nil {
			return LightClientUpdatesResponse{}, err
		}

		var lightClientUpdatesResponse LightClientUpdatesResponse
		if err := json.Unmarshal(body, &lightClientUpdatesResponse); err != nil {
			return LightClientUpdatesResponse{}, err
		}

		return lightClientUpdatesResponse, nil
	})
}

func (b BeaconAPIClient) GetGenesis() (*apiv1.Genesis, error) {
	return retry(b.Retries, b.RetryWait, func() (*apiv1.Genesis, error) {
		genesisResponse, err := b.client.(eth2client.GenesisProvider).Genesis(b.ctx, &api.GenesisOpts{})
		if err != nil {
			return nil, err
		}
		return genesisResponse.Data, nil
	})
}

func (b BeaconAPIClient) GetSpec() (Spec, error) {
	return retry(b.Retries, b.RetryWait, func() (Spec, error) {
		specResponse, err := b.client.(eth2client.SpecProvider).Spec(b.ctx, &api.SpecOpts{})
		if err != nil {
			return Spec{}, err
		}

		specJsonBz, err := json.Marshal(specResponse.Data)
		if err != nil {
			return Spec{}, err
		}
		var spec Spec
		if err := json.Unmarshal(specJsonBz, &spec); err != nil {
			return Spec{}, err
		}

		return spec, nil
	})
}

func (b BeaconAPIClient) GetFinalityUpdate() (FinalityUpdateJSONResponse, error) {
	return retry(b.Retries, b.RetryWait, func() (FinalityUpdateJSONResponse, error) {
		url := fmt.Sprintf("%s/eth/v1/beacon/light_client/finality_update", b.url)
		req, err := http.NewRequest("GET", url, nil)
		if err != nil {
			return FinalityUpdateJSONResponse{}, err
		}
		req.Header.Set("Accept", "application/json")

		resp, err := http.DefaultClient.Do(req)
		if err != nil {
			return FinalityUpdateJSONResponse{}, err
		}
		defer resp.Body.Close()

		body, err := io.ReadAll(resp.Body)
		if err != nil {
			return FinalityUpdateJSONResponse{}, err
		}

		var finalityUpdate FinalityUpdateJSONResponse
		if err := json.Unmarshal(body, &finalityUpdate); err != nil {
			return FinalityUpdateJSONResponse{}, err
		}

		return finalityUpdate, nil
	})
}

func (b BeaconAPIClient) GetBeaconBlocks(blockID string) (BeaconBlocksResponseJSON, error) {
	return retry(b.Retries, b.RetryWait, func() (BeaconBlocksResponseJSON, error) {
		url := fmt.Sprintf("%s/eth/v1/beacon/blocks/finalized", b.url)
		req, err := http.NewRequest("GET", url, nil)
		if err != nil {
			return BeaconBlocksResponseJSON{}, err
		}

		req.Header.Set("Accept", "application/json")
		resp, err := http.DefaultClient.Do(req)
		if err != nil {
			return BeaconBlocksResponseJSON{}, err
		}

		defer resp.Body.Close()

		body, err := io.ReadAll(resp.Body)
		if err != nil {
			return BeaconBlocksResponseJSON{}, err
		}

		if resp.StatusCode != 200 {
			return BeaconBlocksResponseJSON{}, fmt.Errorf("Get execution height (%s) failed with status code: %d, body: %s", url, resp.StatusCode, body)
		}

		var beaconBlocksResponse BeaconBlocksResponseJSON
		if err := json.Unmarshal(body, &beaconBlocksResponse); err != nil {
			return BeaconBlocksResponseJSON{}, err
		}

		return beaconBlocksResponse, nil
	})
}

func (b BeaconAPIClient) GetFinalizedBlocks() (BeaconBlocksResponseJSON, error) {
	return retry(b.Retries, b.RetryWait, func() (BeaconBlocksResponseJSON, error) {
		resp, err := b.GetBeaconBlocks("finalized")
		if err != nil {
			return BeaconBlocksResponseJSON{}, err
		}

		if !resp.Finalized {
			return BeaconBlocksResponseJSON{}, fmt.Errorf("Block is not finalized")
		}

		return resp, nil
	})
}

func (b BeaconAPIClient) GetExecutionHeight(blockID string) (uint64, error) {
	return retry(b.Retries, b.RetryWait, func() (uint64, error) {
		resp, err := b.GetBeaconBlocks(blockID)
		if err != nil {
			return 0, err
		}

		if blockID == "finalized" && !resp.Finalized {
			return 0, fmt.Errorf("Block is not finalized")
		}
		fmt.Printf("block number %d, finalized: %t\n", resp.Data.Message.Body.ExecutionPayload.BlockNumber, resp.Finalized)

		return resp.Data.Message.Body.ExecutionPayload.BlockNumber, nil
	})
}
