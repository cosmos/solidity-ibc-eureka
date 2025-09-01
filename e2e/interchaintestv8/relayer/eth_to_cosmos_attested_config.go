package relayer

// AttestorConfig represents the attestor configuration section
type AttestorConfig struct {
	AttestorQueryTimeoutMs int      `json:"attestor_query_timeout_ms"`
	QuorumThreshold        int      `json:"quorum_threshold"`
	AttestorEndpoints      []string `json:"attestor_endpoints"`
}

// CacheConfig represents the cache configuration section
type CacheConfig struct {
	StateCacheMaxEntries  int `json:"state_cache_max_entries"`
	PacketCacheMaxEntries int `json:"packet_cache_max_entries"`
}

// AggregatorConfig represents the full aggregator configuration
type AggregatorConfig struct {
	Attestor AttestorConfig `json:"attestor"`
	Cache    CacheConfig    `json:"cache"`
}

// DefaultAggregatorConfig returns a config with sensible defaults
func DefaultAggregatorConfig() AggregatorConfig {
	return AggregatorConfig{
		Attestor: AttestorConfig{
			AttestorQueryTimeoutMs: 5000,
			QuorumThreshold:        1,
			AttestorEndpoints:      []string{"http://127.0.0.1:9000"},
		},
		Cache: CacheConfig{
			StateCacheMaxEntries:  100000,
			PacketCacheMaxEntries: 100000,
		},
	}
}

// EthToCosmosAttestedConfigInfo is a struct that holds the configuration information for the Attested to Cosmos config template
type EthToCosmosAttestedConfigInfo struct {
	AttestedChainID     string
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
				AttestedChainId:  configInfo.AttestedChainID,
				AggregatorConfig: DefaultAggregatorConfig(),
				AttestedRpcUrl:   configInfo.AttestedRpcUrl,
				Ics26Address:     configInfo.Ics26Address,
				TmRpcUrl:         configInfo.TmRpcUrl,
				SignerAddress:    configInfo.CosmosSignerAddress,
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
