package testvalues

import (
	"time"

	"cosmossdk.io/math"

	ibctm "github.com/cosmos/ibc-go/v8/modules/light-clients/07-tendermint"

	"github.com/strangelove-ventures/interchaintest/v8/chain/ethereum"
)

const (
	// StartingTokenAmount is the amount of tokens to give to each user at the start of the test.
	StartingTokenAmount int64 = 10_000_000_000

	// EnvKeyTendermintRPC Tendermint RPC URL.
	EnvKeyTendermintRPC = "TENDERMINT_RPC_URL"
	// EnvKeyEthRPC Ethereum RPC URL.
	EnvKeyEthRPC = "RPC_URL"
	// EnvKeyPrivateKey Private key used to submit transactions by the operator.
	EnvKeyPrivateKey = "PRIVATE_KEY"
	// EnvKeySp1Prover The prover type (local|network|mock).
	EnvKeySp1Prover = "SP1_PROVER"
	// EnvKeySp1PrivateKey Private key for the prover network.
	EnvKeySp1PrivateKey = "SP1_PRIVATE_KEY"

	// EnvKeySP1ICS07TendermintContractAddress Address of the SP1ICS07Tendermint contract.
	EnvKeySP1ICS07TendermintContractAddress = "SP1_ICS07_TENDERMINT_CONTRACT_ADDRESS"
	// EnvKeyICS02ClientContractAddress Address of the ICS02Client contract.
	EnvKeyICS02ClientContractAddress = "ICS02_CLIENT_CONTRACT_ADDRESS"
	// EnvKeyICS26RouterContractAddress Address of the ICS26Router contract.
	EnvKeyICS26RouterContractAddress = "ICS26_ROUTER_CONTRACT_ADDRESS"
	// EnvKeyICS20TransferContractAddress Address of the ICS20Transfer contract.
	EnvKeyICS20TransferContractAddress = "ICS20_TRANSFER_CONTRACT_ADDRESS"
	// EnvKeyERC20ContractAddress Address of the ERC20 contract.
	EnvKeyERC20ContractAddress = "ERC20_CONTRACT_ADDRESS"
)

var (
	// MaxDepositPeriod Maximum period to deposit on a proposal.
	// This value overrides the default value in the gov module using the `modifyGovV1AppState` function.
	MaxDepositPeriod = time.Second * 10
	// VotingPeriod Duration of the voting period.
	// This value overrides the default value in the gov module using the `modifyGovV1AppState` function.
	VotingPeriod = time.Second * 30

	// StartingEthBalance is the amount of ETH to give to each user at the start of the test.
	StartingEthBalance = math.NewInt(2 * ethereum.ETHER)

	// DefaultTrustLevel is the trust level used by the SP1ICS07Tendermint contract.
	DefaultTrustLevel = ibctm.Fraction{Numerator: 2, Denominator: 3}.ToTendermint()

	// DefaultTrustPeriod is the trust period used by the SP1ICS07Tendermint contract.
	DefaultTrustPeriod = 1209669
)
