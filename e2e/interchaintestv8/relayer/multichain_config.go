package relayer

// MultichainConfigInfo is a struct that holds the configuration information for the multichain config template
type MultichainConfigInfo struct {
	// Chain A chain identifier
	ChainAID string
	// Chain B chain identifier
	ChainBID string
	// Ethereum chain identifier
	EthChainID string
	// Chain A tendermint RPC URL
	ChainATmRPC string
	// Chain B tendermint RPC URL
	ChainBTmRPC string
	// Chain A signer address
	ChainASignerAddress string
	// Chain B signer address
	ChainBSignerAddress string
	// ICS26 Router address
	ICS26Address string
	// Ethereum RPC URL
	EthRPC string
	// Ethereum Beacon API URL
	BeaconAPI string
	// SP1 config
	SP1Config SP1ProverConfig
	// Whether we use the mock client in the cosmos chains
	MockWasmClient bool
}

func CreateMultichainModules(
	configInfo MultichainConfigInfo,
) []ModuleConfig {
	var modules []ModuleConfig
	modules = append(modules, CreateEthCosmosModules(
		EthCosmosConfigInfo{
			EthChainID:     configInfo.EthChainID,
			CosmosChainID:  configInfo.ChainAID,
			TmRPC:          configInfo.ChainATmRPC,
			ICS26Address:   configInfo.ICS26Address,
			EthRPC:         configInfo.EthRPC,
			BeaconAPI:      configInfo.BeaconAPI,
			SP1Config:      configInfo.SP1Config,
			SignerAddress:  configInfo.ChainASignerAddress,
			MockWasmClient: configInfo.MockWasmClient,
		},
	)...)

	modules = append(modules, CreateEthCosmosModules(
		EthCosmosConfigInfo{
			EthChainID:     configInfo.EthChainID,
			CosmosChainID:  configInfo.ChainBID,
			TmRPC:          configInfo.ChainBTmRPC,
			ICS26Address:   configInfo.ICS26Address,
			EthRPC:         configInfo.EthRPC,
			BeaconAPI:      configInfo.BeaconAPI,
			SP1Config:      configInfo.SP1Config,
			SignerAddress:  configInfo.ChainBSignerAddress,
			MockWasmClient: configInfo.MockWasmClient,
		},
	)...)

	modules = append(modules, CreateCosmosCosmosModules(
		CosmosToCosmosConfigInfo{
			ChainAID:    configInfo.ChainAID,
			ChainBID:    configInfo.ChainBID,
			ChainATmRPC: configInfo.ChainATmRPC,
			ChainBTmRPC: configInfo.ChainBTmRPC,
			ChainAUser:  configInfo.ChainASignerAddress,
			ChainBUser:  configInfo.ChainBSignerAddress,
		},
	)...)

	return modules
}
