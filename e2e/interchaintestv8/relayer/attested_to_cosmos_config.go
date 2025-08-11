package relayer

// AttestedToCosmosConfigInfo is a struct that holds the configuration information for the Attested to Cosmos config template
type AttestedToCosmosConfigInfo struct {
	// Attested chain identifier
	AttestedChainID string
	// Cosmos chain identifier
	CosmosChainID string
	// Aggregator service URL for fetching attestations
	AggregatorUrl string
	// Cosmos chain Tendermint RPC URL
	CosmosTmRPC string
	// Cosmos chain submitter address
	CosmosSignerAddress string
}

func CreateAttestedToCosmosModules(
	configInfo AttestedToCosmosConfigInfo,
) []ModuleConfig {
	return []ModuleConfig{
		{
			Name:     ModuleAttestedToCosmos,
			SrcChain: configInfo.AttestedChainID,
			DstChain: configInfo.CosmosChainID,
			Config: AttestedToCosmosModuleConfig{
				AggregatorUrl:   configInfo.AggregatorUrl,
				TmRpcUrl:        configInfo.CosmosTmRPC,
				SignerAddress:   configInfo.CosmosSignerAddress,
				AttestedChainId: configInfo.AttestedChainID,
			},
		},
	}
}