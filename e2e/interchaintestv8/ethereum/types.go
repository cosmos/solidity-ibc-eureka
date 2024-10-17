package ethereum

import (
	"math/big"

	ethcommon "github.com/ethereum/go-ethereum/common"
	ethereumlightclient "github.com/srdtrk/solidity-ibc-eureka/e2e/v8/types/ethereumlightclient"
)

type BeaconJSON struct {
	Slot          uint64 `json:"slot,string"`
	ProposerIndex uint64 `json:"proposer_index,string"`
	ParentRoot    string `json:"parent_root"`
	StateRoot     string `json:"state_root"`
	BodyRoot      string `json:"body_root"`
}

type ExecutionJSON struct {
	ParentHash       string `json:"parent_hash"`
	FeeRecipient     string `json:"fee_recipient"`
	StateRoot        string `json:"state_root"`
	ReceiptsRoot     string `json:"receipts_root"`
	LogsBloom        string `json:"logs_bloom"`
	PrevRandao       string `json:"prev_randao"`
	BlockNumber      uint64 `json:"block_number,string"`
	GasLimit         uint64 `json:"gas_limit,string"`
	GasUsed          uint64 `json:"gas_used,string"`
	Timestamp        uint64 `json:"timestamp,string"`
	ExtraData        string `json:"extra_data"`
	BaseFeePerGas    uint64 `json:"base_fee_per_gas,string"`
	BlockHash        string `json:"block_hash"`
	TransactionsRoot string `json:"transactions_root"`
	WithdrawalsRoot  string `json:"withdrawals_root"`
	BlobGasUsed      uint64 `json:"blob_gas_used,string"`
	ExcessBlobGas    uint64 `json:"excess_blob_gas,string"`
}

type FinalityUpdateJSONResponse struct {
	Version string `json:"version"`
	Data    struct {
		AttestedHeader struct {
			Beacon          BeaconJSON    `json:"beacon"`
			Execution       ExecutionJSON `json:"execution"`
			ExecutionBranch []string      `json:"execution_branch"`
		} `json:"attested_header"`
		FinalizedHeader struct {
			Beacon          BeaconJSON    `json:"beacon"`
			Execution       ExecutionJSON `json:"execution"`
			ExecutionBranch []string      `json:"execution_branch"`
		} `json:"finalized_header"`
		FinalityBranch []string `json:"finality_branch"`
		SyncAggregate  struct {
			SyncCommitteeBits      string `json:"sync_committee_bits"`
			SyncCommitteeSignature string `json:"sync_committee_signature"`
		} `json:"sync_aggregate"`
		SignatureSlot uint64 `json:"signature_slot,string"`
	} `json:"data"`
}

type BeaconBlocksResponseJSON struct {
	ExecutionOptimistic bool `json:"execution_optimistic"`
	Finalized           bool `json:"finalized"`
	Data                struct {
		Message struct {
			Slot          string `json:"slot"`
			ProposerIndex string `json:"proposer_index"`
			ParentRoot    string `json:"parent_root"`
			StateRoot     string `json:"state_root"`
			Body          struct {
				RandaoReveal string `json:"randao_reveal"`
				Eth1Data     struct {
					DepositRoot  string `json:"deposit_root"`
					DepositCount string `json:"deposit_count"`
					BlockHash    string `json:"block_hash"`
				} `json:"eth1_data"`
				Graffiti          string `json:"graffiti"`
				ProposerSlashings []any  `json:"proposer_slashings"`
				AttesterSlashings []any  `json:"attester_slashings"`
				Attestations      []any  `json:"attestations"`
				Deposits          []any  `json:"deposits"`
				VoluntaryExits    []any  `json:"voluntary_exits"`
				SyncAggregate     struct {
					SyncCommitteeBits      string `json:"sync_committee_bits"`
					SyncCommitteeSignature string `json:"sync_committee_signature"`
				} `json:"sync_aggregate"`
				ExecutionPayload struct {
					ParentHash    string `json:"parent_hash"`
					FeeRecipient  string `json:"fee_recipient"`
					StateRoot     string `json:"state_root"`
					ReceiptsRoot  string `json:"receipts_root"`
					LogsBloom     string `json:"logs_bloom"`
					PrevRandao    string `json:"prev_randao"`
					BlockNumber   uint64 `json:"block_number,string"`
					GasLimit      string `json:"gas_limit"`
					GasUsed       string `json:"gas_used"`
					Timestamp     string `json:"timestamp"`
					ExtraData     string `json:"extra_data"`
					BaseFeePerGas string `json:"base_fee_per_gas"`
					BlockHash     string `json:"block_hash"`
					Transactions  []any  `json:"transactions"`
					Withdrawals   []any  `json:"withdrawals"`
					BlobGasUsed   string `json:"blob_gas_used"`
					ExcessBlobGas string `json:"excess_blob_gas"`
				} `json:"execution_payload"`
				BlsToExecutionChanges []any `json:"bls_to_execution_changes"`
				BlobKzgCommitments    []any `json:"blob_kzg_commitments"`
			} `json:"body"`
		} `json:"message"`
		Signature string `json:"signature"`
	} `json:"data"`
}

type LightClientUpdateJSON struct {
	Data struct {
		AttestedHeader struct {
			Beacon          BeaconJSON    `json:"beacon"`
			Execution       ExecutionJSON `json:"execution"`
			ExecutionBranch []string      `json:"execution_branch"`
		} `json:"attested_header"`
		NextSyncCommittee       SyncCommittee `json:"next_sync_committee"`
		NextSyncCommitteeBranch []string      `json:"next_sync_committee_branch"`
		FinalizedHeader         struct {
			Beacon          BeaconJSON    `json:"beacon"`
			Execution       ExecutionJSON `json:"execution"`
			ExecutionBranch []string      `json:"execution_branch"`
		} `json:"finalized_header"`
		FinalityBranch []string `json:"finality_branch"`
		SyncAggregate  struct {
			SyncCommitteeBits      string `json:"sync_committee_bits"`
			SyncCommitteeSignature string `json:"sync_committee_signature"`
		} `json:"sync_aggregate"`
		SignatureSlot uint64 `json:"signature_slot,string"`
	} `json:"data"`
}

func (l LightClientUpdateJSON) ToLightClientUpdate() ethereumlightclient.LightClientUpdate {
	attestedHeaderBeacon := l.Data.AttestedHeader.Beacon.ToBeaconBlockHeader()
	attestedHeaderExecution := l.Data.AttestedHeader.Execution.ToExecutionPayloadHeader()
	var attestedheaderExecutionBranch [][]byte
	for _, branch := range l.Data.AttestedHeader.ExecutionBranch {
		attestedheaderExecutionBranch = append(attestedheaderExecutionBranch, ethcommon.FromHex(branch))
	}

	var nextSyncCommitteePubkeys [][]byte
	for _, pubkey := range l.Data.NextSyncCommittee.Pubkeys {
		nextSyncCommitteePubkeys = append(nextSyncCommitteePubkeys, ethcommon.FromHex(pubkey))
	}
	nextSyncCommitteeAggregatePubkey := ethcommon.FromHex(l.Data.NextSyncCommittee.AggregatePubkey)

	finalizedHeaderBeacon := l.Data.FinalizedHeader.Beacon.ToBeaconBlockHeader()
	finalizedHeaderExecution := l.Data.FinalizedHeader.Execution.ToExecutionPayloadHeader()

	var nextSyncCommitteeBranch [][]byte
	for _, branch := range l.Data.NextSyncCommitteeBranch {
		nextSyncCommitteeBranch = append(nextSyncCommitteeBranch, ethcommon.FromHex(branch))
	}

	var finalityBranch [][]byte
	for _, branch := range l.Data.FinalityBranch {
		finalityBranch = append(finalityBranch, ethcommon.FromHex(branch))
	}

	return ethereumlightclient.LightClientUpdate{
		AttestedHeader: &ethereumlightclient.LightClientHeader{
			Beacon:          &attestedHeaderBeacon,
			Execution:       &attestedHeaderExecution,
			ExecutionBranch: attestedheaderExecutionBranch,
		},
		NextSyncCommittee: &ethereumlightclient.SyncCommittee{
			Pubkeys:         nextSyncCommitteePubkeys,
			AggregatePubkey: nextSyncCommitteeAggregatePubkey,
		},
		NextSyncCommitteeBranch: nextSyncCommitteeBranch,
		FinalizedHeader: &ethereumlightclient.LightClientHeader{
			Beacon:          &finalizedHeaderBeacon,
			Execution:       &finalizedHeaderExecution,
			ExecutionBranch: attestedheaderExecutionBranch,
		},
		FinalityBranch: finalityBranch,
		SyncAggregate: &ethereumlightclient.SyncAggregate{
			SyncCommitteeBits:      ethcommon.FromHex(l.Data.SyncAggregate.SyncCommitteeBits),
			SyncCommitteeSignature: ethcommon.FromHex(l.Data.SyncAggregate.SyncCommitteeSignature),
		},
		SignatureSlot: l.Data.SignatureSlot,
	}
}

func (b BeaconJSON) ToBeaconBlockHeader() ethereumlightclient.BeaconBlockHeader {
	return ethereumlightclient.BeaconBlockHeader{
		Slot:          b.Slot,
		ProposerIndex: b.ProposerIndex,
		ParentRoot:    ethcommon.FromHex(b.ParentRoot),
		StateRoot:     ethcommon.FromHex(b.StateRoot),
		BodyRoot:      ethcommon.FromHex(b.BodyRoot),
	}
}

func (e ExecutionJSON) ToExecutionPayloadHeader() ethereumlightclient.ExecutionPayloadHeader {
	baseFeePerGasBE := BigIntToBeBytes(big.NewInt(int64(e.BaseFeePerGas)))

	return ethereumlightclient.ExecutionPayloadHeader{
		ParentHash:       ethcommon.FromHex(e.ParentHash),
		FeeRecipient:     ethcommon.FromHex(e.FeeRecipient),
		StateRoot:        ethcommon.FromHex(e.StateRoot),
		ReceiptsRoot:     ethcommon.FromHex(e.ReceiptsRoot),
		LogsBloom:        ethcommon.FromHex(e.LogsBloom),
		PrevRandao:       ethcommon.FromHex(e.PrevRandao),
		BlockNumber:      e.BlockNumber,
		GasLimit:         e.GasLimit,
		GasUsed:          e.GasUsed,
		Timestamp:        e.Timestamp,
		ExtraData:        ethcommon.FromHex(e.ExtraData),
		BaseFeePerGas:    baseFeePerGasBE[:],
		BlockHash:        ethcommon.FromHex(e.BlockHash),
		TransactionsRoot: ethcommon.FromHex(e.TransactionsRoot),
		WithdrawalsRoot:  ethcommon.FromHex(e.WithdrawalsRoot),
		BlobGasUsed:      e.BlobGasUsed,
		ExcessBlobGas:    e.ExcessBlobGas,
	}
}

func (f *FinalityUpdateJSONResponse) ToLightClientUpdate() ethereumlightclient.LightClientUpdate {
	attestedHeaderBeacon := f.Data.AttestedHeader.Beacon.ToBeaconBlockHeader()
	attestedHeaderExecution := f.Data.AttestedHeader.Execution.ToExecutionPayloadHeader()
	var attestedheaderExecutionBranch [][]byte
	for _, branch := range f.Data.AttestedHeader.ExecutionBranch {
		attestedheaderExecutionBranch = append(attestedheaderExecutionBranch, ethcommon.FromHex(branch))
	}

	finalizedHeaderBeacon := f.Data.FinalizedHeader.Beacon.ToBeaconBlockHeader()
	finalizedHeaderExecution := f.Data.FinalizedHeader.Execution.ToExecutionPayloadHeader()
	var finalizedheaderExecutionBranch [][]byte
	for _, branch := range f.Data.FinalizedHeader.ExecutionBranch {
		finalizedheaderExecutionBranch = append(finalizedheaderExecutionBranch, ethcommon.FromHex(branch))
	}

	var finalityBranch [][]byte
	for _, branch := range f.Data.FinalityBranch {
		finalityBranch = append(finalityBranch, ethcommon.FromHex(branch))
	}

	return ethereumlightclient.LightClientUpdate{
		AttestedHeader: &ethereumlightclient.LightClientHeader{
			Beacon:          &attestedHeaderBeacon,
			Execution:       &attestedHeaderExecution,
			ExecutionBranch: attestedheaderExecutionBranch,
		},
		// TODO: DO WE NEED THESE? NOT SURE WHERE TO GET THEM?
		//
		// NextSyncCommittee: &ethereumlightclient.SyncCommittee{
		// 	Pubkeys:         [][]byte{},
		// 	AggregatePubkey: []byte{},
		// },
		// NextSyncCommitteeBranch: [][]byte{},
		FinalizedHeader: &ethereumlightclient.LightClientHeader{
			Beacon:          &finalizedHeaderBeacon,
			Execution:       &finalizedHeaderExecution,
			ExecutionBranch: finalizedheaderExecutionBranch,
		},
		FinalityBranch: finalityBranch,
		SyncAggregate: &ethereumlightclient.SyncAggregate{
			SyncCommitteeBits:      ethcommon.FromHex(f.Data.SyncAggregate.SyncCommitteeBits),
			SyncCommitteeSignature: ethcommon.FromHex(f.Data.SyncAggregate.SyncCommitteeSignature),
		},
		SignatureSlot: f.Data.SignatureSlot,
	}

}
