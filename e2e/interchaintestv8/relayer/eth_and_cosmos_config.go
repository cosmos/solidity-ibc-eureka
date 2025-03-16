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
	SP1Config SP1Config
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
			Name:     ModuleEthToCosmos,
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
				Sp1Config:    configInfo.SP1Config,
			},
		},
	}
}
