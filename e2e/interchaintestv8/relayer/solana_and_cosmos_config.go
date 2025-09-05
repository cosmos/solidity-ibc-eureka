package relayer

// SolanaCosmosConfigInfo holds the configuration information for Solana <-> Cosmos relaying
type SolanaCosmosConfigInfo struct {
	// Solana chain identifier (e.g., "solana-localnet", "solana-devnet", "solana-mainnet")
	SolanaChainID string
	// Cosmos chain identifier
	CosmosChainID string
	// Solana RPC URL
	SolanaRPC string
	// Tendermint RPC URL for Cosmos chain
	TmRPC string
	// ICS07 Tendermint program ID on Solana
	ICS07ProgramID string
	// ICS26 Router program ID on Solana
	ICS26RouterProgramID string
	// Signer address for Cosmos transactions
	CosmosSignerAddress string
	// Solana fee payer address (for cosmos-to-solana)
	SolanaFeePayer string
	// Whether we use the mock WASM client in Cosmos (for Solana->Cosmos)
	MockWasmClient bool
	// Whether we use the mock Solana light client (for Cosmos->Solana)
	MockSolanaClient bool
}

type SolanaToCosmosModuleConfig struct {
	// Solana chain ID
	SolanaChainId string `json:"solana_chain_id"`
	// Solana RPC URL
	SolanaRpcUrl string `json:"solana_rpc_url"`
	// Target tendermint RPC URL (must be "target_rpc_url" not "tm_rpc_url")
	TargetRpcUrl string `json:"target_rpc_url"`
	// Signer address for submitting to Cosmos
	SignerAddress string `json:"signer_address"`
	// Solana ICS26 router program ID (must be "solana_ics26_program_id")
	SolanaIcs26ProgramId string `json:"solana_ics26_program_id"`
	// Whether to use mock WASM client on Cosmos for testing
	MockWasmClient bool `json:"mock_wasm_client"`
	// Whether to use mock Solana light client updates for testing
	MockSolanaClient bool `json:"mock_solana_client"`
}

type CosmosToSolanaModuleConfig struct {
	// Source tendermint RPC URL (must be "source_rpc_url")
	SourceRpcUrl string `json:"source_rpc_url"`
	// Solana RPC URL
	SolanaRpcUrl string `json:"solana_rpc_url"`
	// Solana ICS26 router program ID (must be "solana_ics26_program_id")
	SolanaIcs26ProgramId string `json:"solana_ics26_program_id"`
	// Solana ICS07 Tendermint light client program ID (must be "solana_ics07_program_id")
	SolanaIcs07ProgramId string `json:"solana_ics07_program_id"`
	// Solana fee payer address for unsigned transactions
	SolanaFeePayer string `json:"solana_fee_payer"`
	// Whether to use mock WASM client on Cosmos for testing
	MockWasmClient bool `json:"mock_wasm_client"`
	// Whether to use mock Solana light client updates for testing
	MockSolanaClient bool `json:"mock_solana_client"`
}

func CreateSolanaCosmosModules(configInfo SolanaCosmosConfigInfo) []ModuleConfig {
	return []ModuleConfig{
		{
			Name:     ModuleSolanaToCosmos,
			SrcChain: configInfo.SolanaChainID,
			DstChain: configInfo.CosmosChainID,
			Config: SolanaToCosmosModuleConfig{
				SolanaChainId:        configInfo.SolanaChainID,
				SolanaRpcUrl:         configInfo.SolanaRPC,
				TargetRpcUrl:         configInfo.TmRPC,
				SignerAddress:        configInfo.CosmosSignerAddress,
				SolanaIcs26ProgramId: configInfo.ICS26RouterProgramID,
				MockWasmClient:       configInfo.MockWasmClient,
				MockSolanaClient:     configInfo.MockSolanaClient,
			},
		},
		{
			Name:     ModuleCosmosToSolana,
			SrcChain: configInfo.CosmosChainID,
			DstChain: configInfo.SolanaChainID,
			Config: CosmosToSolanaModuleConfig{
				SourceRpcUrl:         configInfo.TmRPC,
				SolanaRpcUrl:         configInfo.SolanaRPC,
				SolanaIcs26ProgramId: configInfo.ICS26RouterProgramID,
				SolanaIcs07ProgramId: configInfo.ICS07ProgramID,
				SolanaFeePayer:       configInfo.SolanaFeePayer,
				MockWasmClient:       configInfo.MockWasmClient,
				MockSolanaClient:     configInfo.MockSolanaClient,
			},
		},
	}
}
