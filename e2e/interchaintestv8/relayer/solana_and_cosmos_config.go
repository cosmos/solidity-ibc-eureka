// Package relayer provides configuration for the IBC relayer service.
// This file defines the configuration structure for Solana <-> Cosmos relaying.
//
// Implementation Status:
// - Configuration structure: COMPLETE
// - Module definitions: COMPLETE  
// - Integration with test suite: COMPLETE
// - Relayer binary support: PENDING (requires implementation in actual relayer)
//
// The Solana relayer modules will handle:
// 1. solana_to_cosmos: Parse Solana transactions, generate proofs, relay to Cosmos
// 2. cosmos_to_solana: Parse Cosmos transactions, generate Solana instructions, relay to Solana
//
// Key differences from Ethereum:
// - No SP1 proofs needed (Solana uses native account/state proofs)
// - Transaction format is different (Solana uses instructions)
// - Light client implementation differs (Anchor programs vs Solidity contracts)
package relayer

// Module names for Solana <-> Cosmos relaying
const (
	ModuleSolanaToCosmos = "solana-to-cosmos"
	ModuleCosmosToSolana = "cosmos-to-solana"
)

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
	// Solana wallet keypair path (for cosmos-to-solana)
	SolanaWalletPath string
}

// SolanaToCosmosModuleConfig represents the configuration for solana_to_cosmos module
// This must match the expected structure in packages/relayer/modules/solana-to-cosmos/src/lib.rs
type SolanaToCosmosModuleConfig struct {
	// Solana RPC URL
	SolanaRpcUrl string `json:"solana_rpc_url"`
	// Target tendermint RPC URL (must be "target_rpc_url" not "tm_rpc_url")
	TargetRpcUrl string `json:"target_rpc_url"`
	// Signer address for submitting to Cosmos
	SignerAddress string `json:"signer_address"`
	// Solana ICS26 router program ID (must be "solana_ics26_program_id")
	SolanaIcs26ProgramId string `json:"solana_ics26_program_id"`
}

// CosmosToSolanaModuleConfig represents the configuration for cosmos_to_solana module
// This must match the expected structure in packages/relayer/modules/cosmos-to-solana/src/lib.rs
type CosmosToSolanaModuleConfig struct {
	// Source tendermint RPC URL (must be "source_rpc_url")
	SourceRpcUrl string `json:"source_rpc_url"`
	// Solana RPC URL
	SolanaRpcUrl string `json:"solana_rpc_url"`
	// Solana ICS26 router program ID (must be "solana_ics26_program_id")
	SolanaIcs26ProgramId string `json:"solana_ics26_program_id"`
	// Solana ICS07 Tendermint light client program ID (must be "solana_ics07_program_id")
	SolanaIcs07ProgramId string `json:"solana_ics07_program_id"`
	// Solana wallet keypair path for signing transactions
	SolanaWalletPath string `json:"solana_wallet_path"`
}

// CreateSolanaCosmosModules creates the module configurations for Solana <-> Cosmos relaying
func CreateSolanaCosmosModules(configInfo SolanaCosmosConfigInfo) []ModuleConfig {
	return []ModuleConfig{
		{
			Name:     ModuleSolanaToCosmos,
			SrcChain: configInfo.SolanaChainID,
			DstChain: configInfo.CosmosChainID,
			Config: SolanaToCosmosModuleConfig{
				SolanaRpcUrl:         configInfo.SolanaRPC,
				TargetRpcUrl:         configInfo.TmRPC,
				SignerAddress:        configInfo.CosmosSignerAddress,
				SolanaIcs26ProgramId: configInfo.ICS26RouterProgramID,
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
				SolanaWalletPath:     configInfo.SolanaWalletPath,
			},
		},
	}
}

// CreateMixedChainModules creates module configurations for Ethereum, Solana, and Cosmos chains
// This allows testing all three chains together
func CreateMixedChainModules(
	ethCosmosConfig EthCosmosConfigInfo,
	solanaCosmosConfig SolanaCosmosConfigInfo,
) []ModuleConfig {
	modules := []ModuleConfig{}
	
	// Add Ethereum <-> Cosmos modules
	modules = append(modules, CreateEthCosmosModules(ethCosmosConfig)...)
	
	// Add Solana <-> Cosmos modules
	modules = append(modules, CreateSolanaCosmosModules(solanaCosmosConfig)...)
	
	return modules
}