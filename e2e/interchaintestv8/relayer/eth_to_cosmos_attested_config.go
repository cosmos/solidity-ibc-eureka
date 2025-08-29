package relayer

// EthToCosmosAttestedConfigInfo is a struct that holds the configuration information for the Attested to Cosmos config template
type EthToCosmosAttestedConfigInfo struct {
	AttestedChainID     string
	AggregatorUrl       string
	AttestedRpcUrl      string
	Ics26Address        string
	TmRpcUrl            string
	CosmosSignerAddress string
	CosmosChainID       string
	SP1Config           SP1ProverConfig
}

func CreateAttestedCosmosModules(
	configInfo EthToCosmosAttestedConfigInfo,
) []ModuleConfig {
	return []ModuleConfig{
		{
			Name:     ModuleEthToCosmosAttested,
			SrcChain: configInfo.AttestedChainID,
			DstChain: configInfo.CosmosChainID,
			Config: EthToCosmosAttestedModuleConfig{
				AttestedChainId: configInfo.AttestedChainID,
				AggregatorUrl:   configInfo.AggregatorUrl,
				AttestedRpcUrl:  configInfo.AttestedRpcUrl,
				Ics26Address:    configInfo.Ics26Address,
				TmRpcUrl:        configInfo.TmRpcUrl,
				SignerAddress:   configInfo.CosmosSignerAddress,
			},
		},
		{
			Name:     ModuleCosmosToEth,
			SrcChain: configInfo.CosmosChainID,
			DstChain: configInfo.AttestedChainID,
			Config: CosmosToEthModuleConfig{
				TmRpcUrl:     configInfo.TmRpcUrl,
				Ics26Address: configInfo.Ics26Address,
				EthRpcUrl:    configInfo.AttestedRpcUrl,
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
