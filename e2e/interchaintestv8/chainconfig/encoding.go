package chainconfig

import (
	"github.com/cosmos/gogoproto/proto"

	txsigning "cosmossdk.io/x/tx/signing"
	upgradetypes "cosmossdk.io/x/upgrade/types"

	"github.com/cosmos/cosmos-sdk/codec"
	"github.com/cosmos/cosmos-sdk/codec/address"
	codectypes "github.com/cosmos/cosmos-sdk/codec/types"
	cryptocodec "github.com/cosmos/cosmos-sdk/crypto/codec"
	sdk "github.com/cosmos/cosmos-sdk/types"
	sdktestutil "github.com/cosmos/cosmos-sdk/types/module/testutil"
	txtypes "github.com/cosmos/cosmos-sdk/types/tx"
	authtx "github.com/cosmos/cosmos-sdk/x/auth/tx"
	authtypes "github.com/cosmos/cosmos-sdk/x/auth/types"
	"github.com/cosmos/cosmos-sdk/x/authz"
	banktypes "github.com/cosmos/cosmos-sdk/x/bank/types"
	consensustypes "github.com/cosmos/cosmos-sdk/x/consensus/types"
	distrtypes "github.com/cosmos/cosmos-sdk/x/distribution/types"
	govv1 "github.com/cosmos/cosmos-sdk/x/gov/types/v1"
	govv1beta1 "github.com/cosmos/cosmos-sdk/x/gov/types/v1beta1"
	grouptypes "github.com/cosmos/cosmos-sdk/x/group"
	minttypes "github.com/cosmos/cosmos-sdk/x/mint/types"
	proposaltypes "github.com/cosmos/cosmos-sdk/x/params/types/proposal"
	slashingtypes "github.com/cosmos/cosmos-sdk/x/slashing/types"
	stakingtypes "github.com/cosmos/cosmos-sdk/x/staking/types"

	icacontrollertypes "github.com/cosmos/ibc-go/v8/modules/apps/27-interchain-accounts/controller/types"
	icahosttypes "github.com/cosmos/ibc-go/v8/modules/apps/27-interchain-accounts/host/types"
	feetypes "github.com/cosmos/ibc-go/v8/modules/apps/29-fee/types"
	transfertypes "github.com/cosmos/ibc-go/v8/modules/apps/transfer/types"
	v7migrations "github.com/cosmos/ibc-go/v8/modules/core/02-client/migrations/v7"
	clienttypes "github.com/cosmos/ibc-go/v8/modules/core/02-client/types"
	connectiontypes "github.com/cosmos/ibc-go/v8/modules/core/03-connection/types"
	channeltypes "github.com/cosmos/ibc-go/v8/modules/core/04-channel/types"
	mock "github.com/cosmos/ibc-go/v8/modules/light-clients/00-mock"
	solomachine "github.com/cosmos/ibc-go/v8/modules/light-clients/06-solomachine"
	ibctmtypes "github.com/cosmos/ibc-go/v8/modules/light-clients/07-tendermint"
	localhost "github.com/cosmos/ibc-go/v8/modules/light-clients/09-localhost"

	wasmtypes "github.com/CosmWasm/wasmd/x/wasm/types"
)

// CosmosEncodingConfig returns the global E2E encoding config for simd.
func CosmosEncodingConfig() *sdktestutil.TestEncodingConfig {
	return encodingConfig("cosmos")
}

// EncodingConfig returns the global E2E encoding config.
// It includes CosmosSDK, IBC, and Wasm messages
func encodingConfig(bech32Prefix string) *sdktestutil.TestEncodingConfig {
	amino := codec.NewLegacyAmino()
	interfaceRegistry, err := codectypes.NewInterfaceRegistryWithOptions(codectypes.InterfaceRegistryOptions{
		ProtoFiles: proto.HybridResolver,
		SigningOptions: txsigning.Options{
			AddressCodec: address.Bech32Codec{
				Bech32Prefix: bech32Prefix,
			},
			ValidatorAddressCodec: address.Bech32Codec{
				Bech32Prefix: bech32Prefix + sdk.PrefixValidator + sdk.PrefixOperator,
			},
		},
	})
	if err != nil {
		panic(err)
	}

	// ibc types
	icacontrollertypes.RegisterInterfaces(interfaceRegistry)
	icahosttypes.RegisterInterfaces(interfaceRegistry)
	feetypes.RegisterInterfaces(interfaceRegistry)
	transfertypes.RegisterInterfaces(interfaceRegistry)
	v7migrations.RegisterInterfaces(interfaceRegistry)
	clienttypes.RegisterInterfaces(interfaceRegistry)
	connectiontypes.RegisterInterfaces(interfaceRegistry)
	channeltypes.RegisterInterfaces(interfaceRegistry)
	solomachine.RegisterInterfaces(interfaceRegistry)
	ibctmtypes.RegisterInterfaces(interfaceRegistry)
	localhost.RegisterInterfaces(interfaceRegistry)
	mock.RegisterInterfaces(interfaceRegistry)

	// sdk types
	upgradetypes.RegisterInterfaces(interfaceRegistry)
	banktypes.RegisterInterfaces(interfaceRegistry)
	govv1beta1.RegisterInterfaces(interfaceRegistry)
	govv1.RegisterInterfaces(interfaceRegistry)
	authtypes.RegisterInterfaces(interfaceRegistry)
	cryptocodec.RegisterInterfaces(interfaceRegistry)
	grouptypes.RegisterInterfaces(interfaceRegistry)
	proposaltypes.RegisterInterfaces(interfaceRegistry)
	authz.RegisterInterfaces(interfaceRegistry)
	txtypes.RegisterInterfaces(interfaceRegistry)
	stakingtypes.RegisterInterfaces(interfaceRegistry)
	minttypes.RegisterInterfaces(interfaceRegistry)
	distrtypes.RegisterInterfaces(interfaceRegistry)
	slashingtypes.RegisterInterfaces(interfaceRegistry)
	consensustypes.RegisterInterfaces(interfaceRegistry)

	// custom module types
	wasmtypes.RegisterInterfaces(interfaceRegistry)

	cdc := codec.NewProtoCodec(interfaceRegistry)

	cfg := &sdktestutil.TestEncodingConfig{
		InterfaceRegistry: interfaceRegistry,
		Codec:             cdc,
		TxConfig:          authtx.NewTxConfig(cdc, authtx.DefaultSignModes),
		Amino:             amino,
	}

	return cfg
}
