package chainconfig

import (
	"bytes"
	"context"
	"encoding/json"
	"fmt"

	sdk "github.com/cosmos/cosmos-sdk/types"
	authtypes "github.com/cosmos/cosmos-sdk/x/auth/types"
	genutiltypes "github.com/cosmos/cosmos-sdk/x/genutil/types"
	govtypes "github.com/cosmos/cosmos-sdk/x/gov/types"
	govv1 "github.com/cosmos/cosmos-sdk/x/gov/types/v1"

	"github.com/cosmos/interchaintest/v11/chain/cosmos"
	"github.com/cosmos/interchaintest/v11/ibc"

	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/testvalues"
)

// poaValidatorKeyName is the keyring name used for the PoA validator operator key.
const poaValidatorKeyName = "validator"

// poaGenesisValidator carries the data discovered while bootstrapping a PoA
// chain (in PreGenesis) over to the genesis injection step (in ModifyGenesis).
//
// The two steps run at different points in interchaintest's chain startup and
// communicate through a shared pointer captured by both closures.
type poaGenesisValidator struct {
	// consensusPubKey is the node's CometBFT consensus public key, as the JSON
	// Any emitted by `<bin> comet show-validator`
	// (e.g. {"@type":"/cosmos.crypto.ed25519.PubKey","key":"..."}).
	consensusPubKey json.RawMessage
	// operatorAddress is the bech32 account address of the validator operator key.
	operatorAddress string
}

// wfchainPreGenesis returns a PreGenesis hook that bootstraps the single PoA
// validator. Because we set SkipGenTx, interchaintest does not create the
// validator key or its genesis account, so we do that here and additionally
// capture the node's consensus public key for genesis injection.
func wfchainPreGenesis(out *poaGenesisValidator) func(ibc.Chain) error {
	return func(chain ibc.Chain) error {
		ctx := context.Background()

		cosmosChain, ok := chain.(*cosmos.CosmosChain)
		if !ok {
			return fmt.Errorf("expected *cosmos.CosmosChain, got %T", chain)
		}
		node := cosmosChain.GetNode()

		// Create the validator operator key and fund it in genesis.
		//
		// Use a standard secp256k1 key rather than the chain's default
		// (eth_secp256k1): this key signs the governance txs that register IFT
		// bridges (sandbox-ledger's PoA module restricts governance to validators),
		// and interchaintest decodes those txs host-side with the stock cosmos-sdk
		// codec, which cannot resolve the eth_secp256k1 pubkey type.
		if _, _, err := node.Exec(ctx, []string{
			cosmosChain.Config().Bin, "keys", "add", poaValidatorKeyName,
			"--key-type", "secp256k1",
			"--coin-type", cosmosChain.Config().CoinType,
			"--keyring-backend", "test",
			"--home", node.HomeDir(),
			"--output", "json",
		}, cosmosChain.Config().Env); err != nil {
			return fmt.Errorf("failed to create poa validator key: %w", err)
		}
		operatorAddr, err := node.AccountKeyBech32(ctx, poaValidatorKeyName)
		if err != nil {
			return fmt.Errorf("failed to get poa validator address: %w", err)
		}
		genesisCoin := sdk.NewInt64Coin(cosmosChain.Config().Denom, 1_000_000_000_000)
		if err := node.AddGenesisAccount(ctx, operatorAddr, []sdk.Coin{genesisCoin}); err != nil {
			return fmt.Errorf("failed to add poa validator genesis account: %w", err)
		}

		// Read the node's CometBFT consensus public key (local; no running node needed).
		stdout, _, err := node.ExecBin(ctx, "comet", "show-validator")
		if err != nil {
			return fmt.Errorf("failed to read consensus pubkey: %w", err)
		}

		out.consensusPubKey = json.RawMessage(bytes.TrimSpace(stdout))
		out.operatorAddress = operatorAddr
		return nil
	}
}

func defaultModifyGenesis() func(ibc.ChainConfig, []byte) ([]byte, error) {
	return func(chainConfig ibc.ChainConfig, genBz []byte) ([]byte, error) {
		appGenesis, err := genutiltypes.AppGenesisFromReader(bytes.NewReader(genBz))
		if err != nil {
			return nil, fmt.Errorf("failed to unmarshal genesis bytes: %w", err)
		}

		var appState genutiltypes.AppMap
		if err := json.Unmarshal(appGenesis.AppState, &appState); err != nil {
			return nil, fmt.Errorf("failed to unmarshal app state: %w", err)
		}

		// modify the gov v1 app state
		govGenBz, err := modifyGovV1AppState(chainConfig, appState[govtypes.ModuleName])
		if err != nil {
			return nil, fmt.Errorf("failed to modify gov v1 app state: %w", err)
		}

		appState[govtypes.ModuleName] = govGenBz

		// marshal the app state
		appGenesis.AppState, err = json.Marshal(appState)
		if err != nil {
			return nil, fmt.Errorf("failed to marshal app state: %w", err)
		}

		res, err := json.MarshalIndent(appGenesis, "", "  ")
		if err != nil {
			return nil, fmt.Errorf("failed to marshal app genesis: %w", err)
		}

		return res, nil
	}
}

func wfchainModifyGenesis(poaVal *poaGenesisValidator) func(ibc.ChainConfig, []byte) ([]byte, error) {
	return func(chainConfig ibc.ChainConfig, genBz []byte) ([]byte, error) {
		appGenesis, err := genutiltypes.AppGenesisFromReader(bytes.NewReader(genBz))
		if err != nil {
			return nil, fmt.Errorf("failed to unmarshal genesis bytes: %w", err)
		}

		var appState genutiltypes.AppMap
		if err := json.Unmarshal(appGenesis.AppState, &appState); err != nil {
			return nil, fmt.Errorf("failed to unmarshal app state: %w", err)
		}

		// modify the gov v1 app state
		govGenBz, err := modifyGovV1AppState(chainConfig, appState[govtypes.ModuleName])
		if err != nil {
			return nil, fmt.Errorf("failed to modify gov v1 app state: %w", err)
		}
		appState[govtypes.ModuleName] = govGenBz

		// modify the IFT module state to set authority to gov module
		iftGenBz, err := modifyIFTAppState(chainConfig, appState["ift"])
		if err != nil {
			return nil, fmt.Errorf("failed to modify ift app state: %w", err)
		}
		appState["ift"] = iftGenBz

		// seed the PoA validator set with the node's consensus key (discovered in PreGenesis)
		poaGenBz, err := modifyPoAAppState(appState["poa"], poaVal)
		if err != nil {
			return nil, fmt.Errorf("failed to modify poa app state: %w", err)
		}
		appState["poa"] = poaGenBz

		// disable the EIP-1559 base fee so the test faucet (and tests) can submit
		// zero-fee transactions
		feemarketGenBz, err := modifyFeemarketAppState(appState["feemarket"])
		if err != nil {
			return nil, fmt.Errorf("failed to modify feemarket app state: %w", err)
		}
		appState["feemarket"] = feemarketGenBz

		// marshal the app state
		appGenesis.AppState, err = json.Marshal(appState)
		if err != nil {
			return nil, fmt.Errorf("failed to marshal app state: %w", err)
		}

		res, err := json.MarshalIndent(appGenesis, "", "  ")
		if err != nil {
			return nil, fmt.Errorf("failed to marshal app genesis: %w", err)
		}

		return res, nil
	}
}

// modifyIFTAppState sets the IFT module authority to governance module
func modifyIFTAppState(chainConfig ibc.ChainConfig, iftAppState []byte) ([]byte, error) {
	var iftGenesis map[string]interface{}

	if len(iftAppState) == 0 {
		iftGenesis = make(map[string]interface{})
	} else {
		if err := json.Unmarshal(iftAppState, &iftGenesis); err != nil {
			return nil, fmt.Errorf("failed to unmarshal ift genesis: %w", err)
		}
	}

	// Get or create params
	params, ok := iftGenesis["params"].(map[string]interface{})
	if !ok {
		params = make(map[string]interface{})
	}

	// Set authority to governance module address with chain's bech32 prefix
	govModuleAddr := authtypes.NewModuleAddress(govtypes.ModuleName)
	bech32Addr, err := sdk.Bech32ifyAddressBytes(chainConfig.Bech32Prefix, govModuleAddr)
	if err != nil {
		return nil, fmt.Errorf("failed to create gov module address: %w", err)
	}

	params["authority"] = bech32Addr
	iftGenesis["params"] = params

	return json.Marshal(iftGenesis)
}

// modifyPoAAppState seeds the PoA module's validator set with the chain's single
// validator. The enterprise PoA module requires genesis to contain at least one
// validator with positive power (and does not derive validators from gentxs), so
// we inject an entry built from the node's CometBFT consensus key and operator
// address captured during PreGenesis.
func modifyPoAAppState(poaAppState []byte, poaVal *poaGenesisValidator) ([]byte, error) {
	if poaVal == nil || len(poaVal.consensusPubKey) == 0 || poaVal.operatorAddress == "" {
		return nil, fmt.Errorf("poa validator info was not populated during PreGenesis")
	}

	var poaGenesis map[string]interface{}
	if len(poaAppState) == 0 {
		poaGenesis = make(map[string]interface{})
	} else if err := json.Unmarshal(poaAppState, &poaGenesis); err != nil {
		return nil, fmt.Errorf("failed to unmarshal poa genesis: %w", err)
	}

	var pubKey interface{}
	if err := json.Unmarshal(poaVal.consensusPubKey, &pubKey); err != nil {
		return nil, fmt.Errorf("failed to unmarshal consensus pubkey: %w", err)
	}

	poaGenesis["validators"] = []interface{}{
		map[string]interface{}{
			"pub_key": pubKey,
			// int64 power; emitted as a string for proto JSON compatibility.
			"power": "1",
			"metadata": map[string]interface{}{
				"moniker":          poaValidatorKeyName,
				"operator_address": poaVal.operatorAddress,
				"description":      "",
			},
		},
	}

	return json.Marshal(poaGenesis)
}

// modifyFeemarketAppState disables the cosmos/evm feemarket EIP-1559 base fee so
// that zero-fee transactions are accepted. interchaintest funds test users at the
// chain's configured gas price (0stake); without this the feemarket ante rejects
// those transactions with "gas prices too low ... insufficient fee".
func modifyFeemarketAppState(feemarketAppState []byte) ([]byte, error) {
	var feemarketGenesis map[string]interface{}
	if len(feemarketAppState) == 0 {
		feemarketGenesis = make(map[string]interface{})
	} else if err := json.Unmarshal(feemarketAppState, &feemarketGenesis); err != nil {
		return nil, fmt.Errorf("failed to unmarshal feemarket genesis: %w", err)
	}

	params, ok := feemarketGenesis["params"].(map[string]interface{})
	if !ok {
		params = make(map[string]interface{})
	}
	params["no_base_fee"] = true
	params["base_fee"] = "0"
	params["min_gas_price"] = "0"
	feemarketGenesis["params"] = params

	return json.Marshal(feemarketGenesis)
}

// modifyGovV1AppState takes the existing gov app state and marshals it to a govv1 GenesisState.
func modifyGovV1AppState(chainConfig ibc.ChainConfig, govAppState []byte) ([]byte, error) {
	cdc := SDKEncodingConfig().Codec

	govGenesisState := &govv1.GenesisState{}
	if err := cdc.UnmarshalJSON(govAppState, govGenesisState); err != nil {
		return nil, fmt.Errorf("failed to unmarshal genesis bytes into gov genesis state: %w", err)
	}

	if govGenesisState.Params == nil {
		govGenesisState.Params = &govv1.Params{}
	}

	govGenesisState.Params.MinDeposit = sdk.NewCoins(sdk.NewCoin(chainConfig.Denom, govv1.DefaultMinDepositTokens))
	govGenesisState.Params.MaxDepositPeriod = &testvalues.MaxDepositPeriod
	govGenesisState.Params.VotingPeriod = &testvalues.VotingPeriod

	// govGenBz := MustProtoMarshalJSON(govGenesisState)

	govGenBz, err := cdc.MarshalJSON(govGenesisState)
	if err != nil {
		return nil, fmt.Errorf("failed to marshal gov genesis state: %w", err)
	}

	return govGenBz, nil
}
