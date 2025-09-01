package main

import (
	"context"
	"encoding/binary"
	"os"
	"testing"
	"time"

	"github.com/cosmos/solidity-ibc-eureka/e2e/v8/cosmos"
	"github.com/cosmos/solidity-ibc-eureka/e2e/v8/e2esuite"
	"github.com/cosmos/solidity-ibc-eureka/e2e/v8/solana"
	"github.com/cosmos/solidity-ibc-eureka/e2e/v8/testvalues"
	"github.com/stretchr/testify/suite"

	solanago "github.com/gagliardetto/solana-go"

	"github.com/cosmos/solidity-ibc-eureka/packages/go-anchor/ics07tendermint"
	"github.com/cosmos/solidity-ibc-eureka/packages/go-anchor/ics26router"
)

type IbcEurekaSolanaTestSuite struct {
	e2esuite.TestSuite

	SolanaUser *solanago.Wallet
}

// TestWithIbcEurekaTestSuite is the boilerplate code that allows the test suite to be run
func TestWithIbcEurekaSolanaTestSuite(t *testing.T) {
	suite.Run(t, new(IbcEurekaSolanaTestSuite))
}

// SetupSuite calls the underlying IbcEurekaTestSuite's SetupSuite method
// and deploys the IbcEureka contract
func (s *IbcEurekaSolanaTestSuite) SetupSuite(ctx context.Context) {
	os.Setenv(testvalues.EnvKeyEthTestnetType, testvalues.EthTestnetTypeNone)
	os.Setenv(testvalues.EnvKeySolanaTestnetType, testvalues.SolanaTestnetType_Localnet)
	s.TestSuite.SetupSuite(ctx)

	simd := s.CosmosChains[0]

	var err error
	s.SolanaUser, err = s.SolanaChain.CreateAndFundWallet()
	s.Require().NoError(err)

	s.Require().True(s.Run("Deploy contracts", func() {
		_, err := s.SolanaChain.FundUser(solana.DeployerPubkey, 20*testvalues.InitialSolBalance)
		s.Require().NoError(err)

		ics07ProgramID, _, err := solana.AnchorDeploy(ctx, "../../programs/solana", "ics07_tendermint", "./solana/ics07_tendermint-keypair.json")
		s.Require().NoError(err)
		s.Require().Equal(ics07_tendermint.ProgramID, ics07ProgramID)

		// Set the program ID in the ics07_tendermint package, in case it is not matched automatically
		ics07_tendermint.ProgramID = ics07ProgramID

		ics26RouterProgramID, _, err := solana.AnchorDeploy(ctx, "../../programs/solana", "ics26_router", "./solana/ics26_router-keypair.json")
		s.Require().NoError(err)
		s.Require().Equal(ics26_router.ProgramID, ics26RouterProgramID)
	}))

	// Wait for finality
	time.Sleep(12 * time.Second)

	s.Require().True(s.Run("Initialize Contracts", func() {
		s.Require().True(s.Run("Initialize ICS07 Tendermint", func() {
			var (
				initClientState    ics07_tendermint.ClientState
				initConsensusState ics07_tendermint.ConsensusState
			)
			s.Require().True(s.Run("Get initial client and consensus states", func() {
				header, err := cosmos.FetchCosmosHeader(ctx, simd)
				s.Require().NoError(err)
				stakingParams, err := simd.StakingQueryParams(ctx)
				s.Require().NoError(err)

				initClientState = ics07_tendermint.ClientState{
					ChainId:               simd.Config().ChainID,
					TrustLevelNumerator:   testvalues.DefaultTrustLevel.Numerator,
					TrustLevelDenominator: testvalues.DefaultTrustLevel.Denominator,
					TrustingPeriod:        uint64(testvalues.DefaultTrustPeriod),
					UnbondingPeriod:       uint64(stakingParams.UnbondingTime.Seconds()),
					MaxClockDrift:         uint64(testvalues.DefaultMaxClockDrift),
					LatestHeight: ics07_tendermint.IbcHeight{
						RevisionNumber: 1,
						RevisionHeight: uint64(header.Height),
					},
				}

				initConsensusState = ics07_tendermint.ConsensusState{
					Timestamp:          uint64(header.Time.UnixNano()),
					Root:               [32]uint8(header.AppHash),
					NextValidatorsHash: [32]uint8(header.NextValidatorsHash),
				}
			}))

			clientStateAccount, _, err := solanago.FindProgramAddress([][]byte{[]byte("client"), []byte(simd.Config().ChainID)}, ics07_tendermint.ProgramID)
			s.Require().NoError(err)

			consensusStateSeed := [][]byte{[]byte("consensus_state"), clientStateAccount.Bytes(), uint64ToLeBytes(initClientState.LatestHeight.RevisionHeight)}

			consensusStateAccount, _, err := solanago.FindProgramAddress(consensusStateSeed, ics07_tendermint.ProgramID)
			s.Require().NoError(err)

			initInstruction, err := ics07_tendermint.NewInitializeInstruction(
				initClientState.ChainId, initClientState.LatestHeight.RevisionHeight, initClientState, initConsensusState, clientStateAccount, consensusStateAccount, s.SolanaUser.PublicKey(), solanago.SystemProgramID,
			)
			s.Require().NoError(err)

			tx, err := s.SolanaChain.NewTransactionFromInstructions(s.SolanaUser.PublicKey(), initInstruction)
			s.Require().NoError(err)

			_, err = s.SolanaChain.SignAndBroadcastTx(ctx, tx, s.SolanaUser)
			s.Require().NoError(err)
		}))

		s.Require().True(s.Run("Initialize ICS26 Router", func() {
			routerStateAccount, _, err := solanago.FindProgramAddress([][]byte{[]byte("router_state")}, ics26_router.ProgramID)
			s.Require().NoError(err)
			initInstruction, err := ics26_router.NewInitializeInstruction(s.SolanaUser.PublicKey(), routerStateAccount, s.SolanaUser.PublicKey(), solanago.SystemProgramID)
			s.Require().NoError(err)

			tx, err := s.SolanaChain.NewTransactionFromInstructions(s.SolanaUser.PublicKey(), initInstruction)
			s.Require().NoError(err)
			_, err = s.SolanaChain.SignAndBroadcastTx(ctx, tx, s.SolanaUser)
			s.Require().NoError(err)
		}))
	}))
}

func (s *IbcEurekaSolanaTestSuite) Test_Deploy() {
	ctx := context.Background()
	s.SetupSuite(ctx)

	simd := s.CosmosChains[0]

	s.Require().True(s.Run("Verify ics07-svm-tendermint", func() {
		clientStateAccount, _, err := solanago.FindProgramAddress([][]byte{[]byte("client"), []byte(simd.Config().ChainID)}, ics07_tendermint.ProgramID)
		s.Require().NoError(err)

		accountInfo, err := s.SolanaChain.RPCClient.GetAccountInfo(ctx, clientStateAccount)
		s.Require().NoError(err)

		clientState, err := ics07_tendermint.ParseAccount_ClientState(accountInfo.Value.Data.GetBinary())
		s.Require().NoError(err)

		s.Require().Equal(simd.Config().ChainID, clientState.ChainId)
		s.Require().Equal(testvalues.DefaultTrustLevel.Denominator, clientState.TrustLevelDenominator)
		s.Require().Equal(testvalues.DefaultTrustLevel.Numerator, clientState.TrustLevelNumerator)
		s.Require().Equal(uint64(testvalues.DefaultTrustPeriod), clientState.TrustingPeriod)
		s.Require().True(clientState.UnbondingPeriod > clientState.TrustingPeriod)
		s.Require().Equal(uint64(testvalues.DefaultMaxClockDrift), clientState.MaxClockDrift)
		s.Require().Equal(uint64(1), clientState.LatestHeight.RevisionNumber)
		s.Require().Equal(uint64(0), clientState.FrozenHeight.RevisionNumber)
		s.Require().Equal(uint64(0), clientState.FrozenHeight.RevisionHeight)
	}))
}

func uint64ToLeBytes(val uint64) []byte {
	b := make([]byte, 8)
	binary.LittleEndian.PutUint64(b, val)
	return b
}
