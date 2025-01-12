package chainconfig

import (
	"bytes"
	"encoding/json"
	"fmt"

	govtypes "cosmossdk.io/x/gov/types"
	govv1 "cosmossdk.io/x/gov/types/v1"

	sdk "github.com/cosmos/cosmos-sdk/types"
	genutiltypes "github.com/cosmos/cosmos-sdk/x/genutil/types"

	"github.com/strangelove-ventures/interchaintest/v9/ibc"

	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/testvalues"
)

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

// modifyGovV1AppState takes the existing gov app state and marshals it to a govv1 GenesisState.
func modifyGovV1AppState(chainConfig ibc.ChainConfig, govAppState []byte) ([]byte, error) {
	cdc := Codec()

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
