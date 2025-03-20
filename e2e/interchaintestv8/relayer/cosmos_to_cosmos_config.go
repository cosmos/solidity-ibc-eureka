package relayer

// CosmosToCosmosConfigInfo is a struct that holds the configuration information for the Cosmos to Cosmos config template
type CosmosToCosmosConfigInfo struct {
	// Chain A chain identifier
	ChainAID string
	// Chain B chain identifier
	ChainBID string
	// ChainA Tendermint RPC URL
	ChainATmRPC string
	// ChainB Tendermint RPC URL
	ChainBTmRPC string
	// ChainA Submitter address
	ChainAUser string
	// ChainB Submitter address
	ChainBUser string
}

func CreateCosmosCosmosModules(
	configInfo CosmosToCosmosConfigInfo,
) []ModuleConfig {
	return []ModuleConfig{
		{
			Name:     ModuleCosmosToCosmos,
			SrcChain: configInfo.ChainAID,
			DstChain: configInfo.ChainBID,
			Config: CosmosToCosmosModuleConfig{
				SrcRpcUrl:     configInfo.ChainATmRPC,
				TargetRpcUrl:  configInfo.ChainBTmRPC,
				SignerAddress: configInfo.ChainBUser,
			},
		},
		{
			Name:     ModuleCosmosToCosmos,
			SrcChain: configInfo.ChainBID,
			DstChain: configInfo.ChainAID,
			Config: CosmosToCosmosModuleConfig{
				SrcRpcUrl:     configInfo.ChainBTmRPC,
				TargetRpcUrl:  configInfo.ChainATmRPC,
				SignerAddress: configInfo.ChainAUser,
			},
		},
	}
}
