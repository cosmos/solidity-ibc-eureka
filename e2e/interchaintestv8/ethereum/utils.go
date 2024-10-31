package ethereum

import (
	"encoding/json"
	"fmt"
	"math/big"
	"regexp"
	"strings"

	ethcommon "github.com/ethereum/go-ethereum/common"
	"github.com/ethereum/go-ethereum/crypto"

	clienttypes "github.com/cosmos/ibc-go/v8/modules/core/02-client/types"

	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/testvalues"
	ethereumligthclient "github.com/srdtrk/solidity-ibc-eureka/e2e/v8/types/ethereumlightclient"
)

type ForgeScriptReturnValues struct {
	InternalType string `json:"internal_type"`
	Value        string `json:"value"`
}

type ForgeDeployOutput struct {
	Returns map[string]ForgeScriptReturnValues `json:"returns"`
}

type DeployedContracts struct {
	Ics07Tendermint string `json:"ics07Tendermint"`
	Ics02Client     string `json:"ics02Client"`
	Ics26Router     string `json:"ics26Router"`
	Ics20Transfer   string `json:"ics20Transfer"`
	Erc20           string `json:"erc20"`
	Escrow          string `json:"escrow"`
	IbcStore        string `json:"ibcstore"`
}

func GetEthContractsFromDeployOutput(stdout string) (DeployedContracts, error) {
	// Remove everything above the JSON part
	cutOff := "== Return =="
	cutoffIndex := strings.Index(stdout, cutOff)
	stdout = stdout[cutoffIndex+len(cutOff):]

	// Extract the JSON part using regex
	re := regexp.MustCompile(`\{.*\}`)
	jsonPart := re.FindString(stdout)

	jsonPart = strings.ReplaceAll(jsonPart, `\"`, `"`)
	jsonPart = strings.Trim(jsonPart, `"`)

	var embeddedContracts DeployedContracts
	err := json.Unmarshal([]byte(jsonPart), &embeddedContracts)
	if err != nil {
		return DeployedContracts{}, err
	}

	if embeddedContracts.Erc20 == "" ||
		embeddedContracts.Ics02Client == "" ||
		embeddedContracts.Ics07Tendermint == "" ||
		embeddedContracts.Ics20Transfer == "" ||
		embeddedContracts.Ics26Router == "" ||
		embeddedContracts.Escrow == "" ||
		embeddedContracts.IbcStore == "" {

		return DeployedContracts{}, fmt.Errorf("one or more contracts missing: %+v", embeddedContracts)
	}

	return embeddedContracts, nil
}

// From https://medium.com/@zhuytt4/verify-the-owner-of-safe-wallet-with-eth-getproof-7edc450504ff
func GetCommitmentsStorageKey(path string) ethcommon.Hash {
	commitmentStorageSlot := ethcommon.FromHex(testvalues.IbcCommitmentSlotHex)

	pathHash := crypto.Keccak256([]byte(path))

	// zero pad both to 32 bytes
	paddedSlot := ethcommon.LeftPadBytes(commitmentStorageSlot, 32)

	// keccak256(h(k) . slot)
	return crypto.Keccak256Hash(pathHash, paddedSlot)
}

// Utility method to get JSON in a format that can be used in the Union unit tests: https://github.com/unionlabs/union/tree/main/light-clients/ethereum-light-client/src/test
func GetUnionClientStateUnitTestJSON(
	ethClientState ethereumligthclient.ClientState,
	spec Spec,
	ics26RouterAddress string,
	clientChecksum string,
	latestHeight clienttypes.Height,
) string {
	return fmt.Sprintf(`{
  "data": {
    "chain_id": "%s",
    "genesis_validators_root": "0x%s",
    "min_sync_committee_participants": 0,
    "genesis_time": %d,
    "fork_parameters": {
      "genesis_fork_version": "%s",
      "genesis_slot": %d,
      "altair": {
        "version": "0x%s",
        "epoch": %d
      },
      "bellatrix": {
        "version": "0x%s",
        "epoch": %d
      },
      "capella": {
        "version": "0x%s",
        "epoch": %d
      },
      "deneb": {
        "version": "0x%s",
        "epoch": %d
      }
    },
    "seconds_per_slot": %d,
    "slots_per_epoch": %d,
    "epochs_per_sync_committee_period": %d,
    "latest_slot": %d,
    "frozen_height": {
      "revision_number": 0,
      "revision_height": 0
    },
    "ibc_commitment_slot": "0",
    "ibc_contract_address": "%s"
  },
  "checksum": "%s",
  "latest_height": {
    "revision_number": %d,
    "revision_height": %d
  }
}\n`,
		ethClientState.ChainId,
		ethcommon.Bytes2Hex(ethClientState.GenesisValidatorsRoot),
		ethClientState.GenesisTime,
		ethcommon.Bytes2Hex(spec.GenesisForkVersion[:]),
		spec.GenesisSlot,
		ethcommon.Bytes2Hex(spec.AltairForkVersion[:]),
		spec.AltairForkEpoch,
		ethcommon.Bytes2Hex(spec.BellatrixForkVersion[:]),
		spec.BellatrixForkEpoch,
		ethcommon.Bytes2Hex(spec.CapellaForkVersion[:]),
		spec.CapellaForkEpoch,
		ethcommon.Bytes2Hex(spec.DenebForkVersion[:]),
		spec.DenebForkEpoch,
		ethClientState.SecondsPerSlot,
		ethClientState.SlotsPerEpoch,
		ethClientState.EpochsPerSyncCommitteePeriod,
		ethClientState.LatestSlot,
		ics26RouterAddress,
		clientChecksum,
		latestHeight.RevisionNumber,
		latestHeight.RevisionHeight,
	)
}

// Utility method to get JSON in a format that can be used in the Union unit tests: https://github.com/unionlabs/union/tree/main/light-clients/ethereum-light-client/src/test
func GetUnionConsensusStateUnitTestJSON(
	ethConsensusState ethereumligthclient.ConsensusState,
	bootstrap Bootstrap,
	proofResp EthGetProofResponse,
	timestamp uint64,
	clientUpdate LightClientUpdateJSON,
) string {
	return fmt.Sprintf(`{
  "data": {
    "slot": %d,
    "state_root": "%s",
    "storage_root": "%s",
    "timestamp": %d,
    "current_sync_committee": "%s",
    "next_sync_committee": "%s"
  }
}\n`,
		ethConsensusState.Slot,
		bootstrap.Data.Header.Execution.StateRoot,
		proofResp.StorageHash,
		timestamp,
		bootstrap.Data.CurrentSyncCommittee.AggregatePubkey,
		clientUpdate.Data.NextSyncCommittee.AggregatePubkey,
	)
}

func HexToBeBytes(hex string) []byte {
	bz := ethcommon.FromHex(hex)
	if len(bz) == 32 {
		return bz
	}
	if len(bz) > 32 {
		panic("TOO BIG!")
	}
	beBytes := make([]byte, 32)
	copy(beBytes[32-len(bz):32], bz)
	return beBytes
}

func BigIntToBeBytes(n *big.Int) [32]byte {
	bytes := n.Bytes()
	var beBytes [32]byte
	copy(beBytes[32-len(bytes):], bytes)
	return beBytes
}
