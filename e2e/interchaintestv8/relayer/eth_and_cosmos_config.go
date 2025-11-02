package relayer

// EthCosmosConfigInfo is a struct that holds the configuration information for the Eth to Cosmos config template
type EthCosmosConfigInfo struct {
	// Ethereum chain identifier
	EthChainID string
	// Cosmos chain identifier
	CosmosChainID string
	// Tendermint RPC URL
	TmRPC string
	// ICS26 Router address
	ICS26Address string
	// Ethereum RPC URL
	EthRPC string
	// Ethereum Beacon API URL
	BeaconAPI string
	// SP1 config
	SP1Config SP1ProverConfig
	// Signer address cosmos
	SignerAddress string
	// Whether we use the mock client in Cosmos
	MockWasmClient bool
}

func CreateEthCosmosModules(
	configInfo EthCosmosConfigInfo,
) []ModuleConfig {
	return []ModuleConfig{
		{
			Name:     ModuleEthToCosmosCompat,
			SrcChain: configInfo.EthChainID,
			DstChain: configInfo.CosmosChainID,
			Config: ethToCosmosConfig{
				TmRpcUrl:        configInfo.TmRPC,
				Ics26Address:    configInfo.ICS26Address,
				EthRpcUrl:       configInfo.EthRPC,
				EthBeaconApiUrl: configInfo.BeaconAPI,
				SignerAddress:   configInfo.SignerAddress,
				Mock:            configInfo.MockWasmClient,
			},
		},
		{
			Name:     ModuleCosmosToEth,
			SrcChain: configInfo.CosmosChainID,
			DstChain: configInfo.EthChainID,
			Config: CosmosToEthModuleConfig{
				TmRpcUrl:     configInfo.TmRPC,
				Ics26Address: configInfo.ICS26Address,
				EthRpcUrl:    configInfo.EthRPC,
				Sp1Prover:    configInfo.SP1Config,
				Sp1Programs: SP1ProgramPaths{
					UpdateClient:              "./programs/sp1-programs/target/elf-compilation/riscv32im-succinct-zkvm-elf/release/sp1-ics07-tendermint-update-client",
					Membership:                "./programs/sp1-programs/target/elf-compilation/riscv32im-succinct-zkvm-elf/release/sp1-ics07-tendermint-membership",
					UpdateClientAndMembership: "./programs/sp1-programs/target/elf-compilation/riscv32im-succinct-zkvm-elf/release/sp1-ics07-tendermint-uc-and-membership",
					Misbehaviour:              "./programs/sp1-programs//target/elf-compilation/riscv32im-succinct-zkvm-elf/release/sp1-ics07-tendermint-misbehaviour",
				},
			},
		},
	}
}
