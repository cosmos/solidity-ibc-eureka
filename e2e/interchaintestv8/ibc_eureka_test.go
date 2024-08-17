package main

import (
	"context"
	"crypto/ecdsa"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"math/big"
	"os"
	"strconv"
	"strings"
	"testing"
	"time"

	"github.com/stretchr/testify/suite"

	ethcommon "github.com/ethereum/go-ethereum/common"
	ethtypes "github.com/ethereum/go-ethereum/core/types"
	"github.com/ethereum/go-ethereum/crypto"
	"github.com/ethereum/go-ethereum/ethclient"

	sdkmath "cosmossdk.io/math"

	sdk "github.com/cosmos/cosmos-sdk/types"
	banktypes "github.com/cosmos/cosmos-sdk/x/bank/types"

	transfertypes "github.com/cosmos/ibc-go/v8/modules/apps/transfer/types"
	clienttypes "github.com/cosmos/ibc-go/v8/modules/core/02-client/types"
	channeltypes "github.com/cosmos/ibc-go/v8/modules/core/04-channel/types"
	commitmenttypes "github.com/cosmos/ibc-go/v8/modules/core/23-commitment/types"
	ibchost "github.com/cosmos/ibc-go/v8/modules/core/24-host"
	ibcexported "github.com/cosmos/ibc-go/v8/modules/core/exported"
	mock "github.com/cosmos/ibc-go/v8/modules/light-clients/00-mock"
	ibctesting "github.com/cosmos/ibc-go/v8/testing"

	"github.com/strangelove-ventures/interchaintest/v8/chain/ethereum"
	"github.com/strangelove-ventures/interchaintest/v8/ibc"

	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/e2esuite"
	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/operator"
	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/testvalues"
	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/types"
	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/types/erc20"
	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/types/ibcerc20"
	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/types/ics02client"
	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/types/ics26router"
	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/types/sdkics20transfer"
	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/types/sp1ics07tendermint"
)

// IbcEurekaTestSuite is a suite of tests that wraps TestSuite
// and can provide additional functionality
type IbcEurekaTestSuite struct {
	e2esuite.TestSuite

	// Whether to generate fixtures for the solidity tests
	generateFixtures bool

	// The private key of a test account
	key *ecdsa.PrivateKey
	// The private key of the faucet account of interchaintest
	faucet   *ecdsa.PrivateKey
	deployer ibc.Wallet

	contractAddresses e2esuite.DeployedContracts

	sp1Ics07Contract *sp1ics07tendermint.Contract
	ics02Contract    *ics02client.Contract
	ics26Contract    *ics26router.Contract
	ics20Contract    *sdkics20transfer.Contract
	erc20Contract    *erc20.Contract

	simdClientID string
	ethClientID  string
}

// SetupSuite calls the underlying IbcEurekaTestSuite's SetupSuite method
// and deploys the IbcEureka contract
func (s *IbcEurekaTestSuite) SetupSuite(ctx context.Context) {
	s.TestSuite.SetupSuite(ctx)

	eth, simd := s.ChainA, s.ChainB

	var prover string
	s.Require().True(s.Run("Set up environment", func() {
		err := os.Chdir("../..")
		s.Require().NoError(err)

		s.key, err = crypto.GenerateKey()
		s.Require().NoError(err)
		testKeyAddress := crypto.PubkeyToAddress(s.key.PublicKey).Hex()

		s.deployer, err = eth.BuildWallet(ctx, "deployer", "")
		s.Require().NoError(err)

		operatorKey, err := crypto.GenerateKey()
		s.Require().NoError(err)
		operatorAddress := crypto.PubkeyToAddress(operatorKey.PublicKey).Hex()

		// get faucet private key from string
		s.faucet, err = crypto.HexToECDSA(testvalues.FaucetPrivateKey)
		s.Require().NoError(err)

		prover = os.Getenv(testvalues.EnvKeySp1Prover)
		switch prover {
		case "":
			prover = testvalues.EnvValueSp1Prover_Network
		case testvalues.EnvValueSp1Prover_Mock:
			s.T().Logf("Using mock prover")
		case testvalues.EnvValueSp1Prover_Network:
		default:
			s.Require().Fail("invalid prover type: %s", prover)
		}

		os.Setenv(testvalues.EnvKeyRustLog, testvalues.EnvValueRustLog_Info)
		os.Setenv(testvalues.EnvKeyEthRPC, eth.GetHostRPCAddress())
		os.Setenv(testvalues.EnvKeyTendermintRPC, simd.GetHostRPCAddress())
		os.Setenv(testvalues.EnvKeySp1Prover, prover)
		os.Setenv(testvalues.EnvKeyOperatorPrivateKey, hex.EncodeToString(crypto.FromECDSA(operatorKey)))
		// make sure that the SP1_PRIVATE_KEY is set.
		s.Require().NotEmpty(os.Getenv(testvalues.EnvKeySp1PrivateKey))
		if os.Getenv(testvalues.EnvKeyGenerateFixtures) == testvalues.EnvValueGenerateFixtures_True {
			s.generateFixtures = true
		}

		s.Require().NoError(eth.SendFunds(ctx, "faucet", ibc.WalletAmount{
			Amount:  testvalues.StartingEthBalance,
			Address: testKeyAddress,
		}))

		s.Require().NoError(eth.SendFunds(ctx, "faucet", ibc.WalletAmount{
			Amount:  testvalues.StartingEthBalance,
			Address: s.deployer.FormattedAddress(),
		}))

		s.Require().NoError(eth.SendFunds(ctx, "faucet", ibc.WalletAmount{
			Amount:  testvalues.StartingEthBalance,
			Address: operatorAddress,
		}))
	}))

	s.Require().True(s.Run("Deploy contracts", func() {
		s.Require().NoError(operator.RunGenesis(
			"--trust-level", testvalues.DefaultTrustLevel.String(),
			"--trusting-period", strconv.Itoa(testvalues.DefaultTrustPeriod),
			"-o", testvalues.Sp1GenesisFilePath,
		))

		var (
			stdout []byte
			stderr []byte
			err    error
		)
		switch prover {
		case testvalues.EnvValueSp1Prover_Mock:
			stdout, stderr, err = eth.ForgeScript(ctx, s.deployer.KeyName(), ethereum.ForgeScriptOpts{
				ContractRootDir:  ".",
				SolidityContract: "script/MockE2ETestDeploy.s.sol",
				RawOptions: []string{
					"--json",
					"--sender", s.deployer.FormattedAddress(), // This, combined with the keyname, makes msg.sender the deployer
				},
			})
			s.Require().NoError(err, fmt.Sprintf("error deploying contracts: \nstderr: %s\nstdout: %s", stderr, stdout))
		case testvalues.EnvValueSp1Prover_Network:
			stdout, stderr, err = eth.ForgeScript(ctx, s.deployer.KeyName(), ethereum.ForgeScriptOpts{
				ContractRootDir:  ".",
				SolidityContract: "script/E2ETestDeploy.s.sol",
				RawOptions: []string{
					"--json",
					"--sender", s.deployer.FormattedAddress(), // This, combined with the keyname, makes msg.sender the deployer
				},
			})
			s.Require().NoError(err, fmt.Sprintf("error deploying contracts: \nstderr: %s\nstdout: %s", stderr, stdout))
		default:
			s.Require().Fail("invalid prover type: %s", prover)
		}

		ethClient, err := ethclient.Dial(eth.GetHostRPCAddress())
		s.Require().NoError(err)

		s.contractAddresses = s.GetEthContractsFromDeployOutput(string(stdout))
		s.sp1Ics07Contract, err = sp1ics07tendermint.NewContract(ethcommon.HexToAddress(s.contractAddresses.Ics07Tendermint), ethClient)
		s.Require().NoError(err)
		s.ics02Contract, err = ics02client.NewContract(ethcommon.HexToAddress(s.contractAddresses.Ics02Client), ethClient)
		s.Require().NoError(err)
		s.ics26Contract, err = ics26router.NewContract(ethcommon.HexToAddress(s.contractAddresses.Ics26Router), ethClient)
		s.Require().NoError(err)
		s.ics20Contract, err = sdkics20transfer.NewContract(ethcommon.HexToAddress(s.contractAddresses.Ics20Transfer), ethClient)
		s.Require().NoError(err)
		s.erc20Contract, err = erc20.NewContract(ethcommon.HexToAddress(s.contractAddresses.Erc20), ethClient)
		s.Require().NoError(err)
	}))

	s.T().Cleanup(func() {
		_ = os.Remove(testvalues.Sp1GenesisFilePath)
	})

	s.Require().True(s.Run("Fund address with ERC20", func() {
		tx, err := s.erc20Contract.Transfer(s.GetTransactOpts(s.faucet), crypto.PubkeyToAddress(s.key.PublicKey), big.NewInt(testvalues.StartingERC20TokenAmount))
		s.Require().NoError(err)

		_ = s.GetTxReciept(ctx, eth, tx.Hash()) // wait for the tx to be mined
	}))

	_, simdRelayerUser := s.GetRelayerUsers(ctx)
	s.Require().True(s.Run("Add client on Cosmos side", func() {
		ethHeight, err := eth.Height(ctx)
		s.Require().NoError(err)

		clientState := mock.ClientState{
			LatestHeight: clienttypes.NewHeight(1, uint64(ethHeight)),
		}
		clientStateAny, err := clienttypes.PackClientState(&clientState)
		s.Require().NoError(err)
		consensusState := mock.ConsensusState{
			Timestamp: uint64(time.Now().UnixNano()),
		}
		consensusStateAny, err := clienttypes.PackConsensusState(&consensusState)
		s.Require().NoError(err)

		res, err := s.BroadcastMessages(ctx, simd, simdRelayerUser, 200_000, &clienttypes.MsgCreateClient{
			ClientState:      clientStateAny,
			ConsensusState:   consensusStateAny,
			Signer:           simdRelayerUser.FormattedAddress(),
			CounterpartyId:   "",
			MerklePathPrefix: nil,
		})
		s.Require().NoError(err)

		s.simdClientID, err = ibctesting.ParseClientIDFromEvents(res.Events)
		s.Require().NoError(err)
		s.Require().Equal("00-mock-0", s.simdClientID)
	}))

	s.Require().True(s.Run("Add client and counterparty on EVM", func() {
		counterpartyInfo := ics02client.IICS02ClientMsgsCounterpartyInfo{
			ClientId: s.simdClientID,
		}
		lightClientAddress := ethcommon.HexToAddress(s.contractAddresses.Ics07Tendermint)
		tx, err := s.ics02Contract.AddClient(s.GetTransactOpts(s.key), ibcexported.Tendermint, counterpartyInfo, lightClientAddress)
		s.Require().NoError(err)

		receipt := s.GetTxReciept(ctx, eth, tx.Hash())
		event, err := e2esuite.GetEvmEvent(receipt, s.ics02Contract.ParseICS02ClientAdded)
		s.Require().NoError(err)
		s.Require().Equal(ibctesting.FirstClientID, event.ClientId)
		s.Require().Equal(s.simdClientID, event.CounterpartyInfo.ClientId)
		s.ethClientID = event.ClientId
	}))

	s.Require().True(s.Run("Register counterparty on Cosmos side", func() {
		// NOTE: This is the mock client on the Cosmos side, so the prefix need not be valid
		merklePathPrefix := commitmenttypes.NewMerklePath([]byte{0x1})

		_, err := s.BroadcastMessages(ctx, simd, simdRelayerUser, 200_000, &clienttypes.MsgProvideCounterparty{
			ClientId:         s.simdClientID,
			CounterpartyId:   s.ethClientID,
			MerklePathPrefix: &merklePathPrefix,
			Signer:           simdRelayerUser.FormattedAddress(),
		})
		s.Require().NoError(err)
	}))
}

// TestWithIbcEurekaTestSuite is the boilerplate code that allows the test suite to be run
func TestWithIbcEurekaTestSuite(t *testing.T) {
	suite.Run(t, new(IbcEurekaTestSuite))
}

// TestDeploy tests the deployment of the IbcEureka contracts
func (s *IbcEurekaTestSuite) TestDeploy() {
	ctx := context.Background()

	s.SetupSuite(ctx)

	_, simd := s.ChainA, s.ChainB

	s.Require().True(s.Run("Verify deployment", func() {
		// Verify that the contracts have been deployed
		s.Require().NotNil(s.sp1Ics07Contract)
		s.Require().NotNil(s.ics02Contract)
		s.Require().NotNil(s.ics26Contract)
		s.Require().NotNil(s.ics20Contract)
		s.Require().NotNil(s.erc20Contract)

		s.Require().True(s.Run("Verify SP1 Client", func() {
			clientState, err := s.sp1Ics07Contract.GetClientState(nil)
			s.Require().NoError(err)

			stakingParams, err := simd.StakingQueryParams(ctx)
			s.Require().NoError(err)

			s.Require().Equal(simd.Config().ChainID, clientState.ChainId)
			s.Require().Equal(uint8(testvalues.DefaultTrustLevel.Numerator), clientState.TrustLevel.Numerator)
			s.Require().Equal(uint8(testvalues.DefaultTrustLevel.Denominator), clientState.TrustLevel.Denominator)
			s.Require().Equal(uint32(testvalues.DefaultTrustPeriod), clientState.TrustingPeriod)
			s.Require().Equal(uint32(stakingParams.UnbondingTime.Seconds()), clientState.UnbondingPeriod)
			s.Require().False(clientState.IsFrozen)
			s.Require().Equal(uint32(1), clientState.LatestHeight.RevisionNumber)
			s.Require().Greater(clientState.LatestHeight.RevisionHeight, uint32(0))
		}))

		s.Require().True(s.Run("Verify ICS02 Client", func() {
			owner, err := s.ics02Contract.Owner(nil)
			s.Require().NoError(err)
			s.Require().Equal(strings.ToLower(s.deployer.FormattedAddress()), strings.ToLower(owner.Hex()))

			clientAddress, err := s.ics02Contract.GetClient(nil, s.ethClientID)
			s.Require().NoError(err)
			s.Require().Equal(s.contractAddresses.Ics07Tendermint, strings.ToLower(clientAddress.Hex()))

			counterpartyInfo, err := s.ics02Contract.GetCounterparty(nil, s.ethClientID)
			s.Require().NoError(err)
			s.Require().Equal(s.simdClientID, counterpartyInfo.ClientId)
		}))

		s.Require().True(s.Run("Verify ICS26 Router", func() {
			owner, err := s.ics26Contract.Owner(nil)
			s.Require().NoError(err)
			s.Require().Equal(strings.ToLower(s.deployer.FormattedAddress()), strings.ToLower(owner.Hex()))

			transferAddress, err := s.ics26Contract.GetIBCApp(nil, transfertypes.PortID)
			s.Require().NoError(err)
			s.Require().Equal(s.contractAddresses.Ics20Transfer, strings.ToLower(transferAddress.Hex()))
		}))

		s.Require().True(s.Run("Verify ERC20 Genesis", func() {
			userBalance, err := s.erc20Contract.BalanceOf(nil, crypto.PubkeyToAddress(s.key.PublicKey))
			s.Require().NoError(err)
			s.Require().Equal(testvalues.StartingERC20TokenAmount, userBalance.Int64())
		}))
	}))
}

func (s *IbcEurekaTestSuite) TestICS20Transfer() {
	ctx := context.Background()

	s.SetupSuite(ctx)

	eth, simd := s.ChainA, s.ChainB

	ics20Address := ethcommon.HexToAddress(s.contractAddresses.Ics20Transfer)
	erc20TransferAmount := big.NewInt(testvalues.ERC20TransferAmount)
	sdkTransferAmount := big.NewInt(testvalues.SDKTransferAmount)
	userAddress := crypto.PubkeyToAddress(s.key.PublicKey)
	receiver := s.UserB

	s.Require().True(s.Run("Approve the SdkICS20Transfer.sol contract to spend the erc20 tokens", func() {
		tx, err := s.erc20Contract.Approve(s.GetTransactOpts(s.key), ics20Address, erc20TransferAmount)
		s.Require().NoError(err)
		receipt := s.GetTxReciept(ctx, eth, tx.Hash())
		s.Require().Equal(ethtypes.ReceiptStatusSuccessful, receipt.Status)

		allowance, err := s.erc20Contract.Allowance(nil, userAddress, ics20Address)
		s.Require().NoError(err)
		s.Require().Equal(erc20TransferAmount, allowance)
	}))

	var sendPacket ics26router.IICS26RouterMsgsPacket
	s.Require().True(s.Run("sendTransfer on Ethereum side", func() {
		timeout := uint64(time.Now().Add(30 * time.Minute).Unix())
		msgSendTransfer := sdkics20transfer.IICS20TransferMsgsSendTransferMsg{
			Denom:            s.contractAddresses.Erc20,
			Amount:           erc20TransferAmount,
			Receiver:         receiver.FormattedAddress(),
			SourceChannel:    s.ethClientID,
			DestPort:         transfertypes.PortID,
			TimeoutTimestamp: timeout,
			Memo:             "",
		}

		tx, err := s.ics20Contract.SendTransfer(s.GetTransactOpts(s.key), msgSendTransfer)
		s.Require().NoError(err)
		receipt := s.GetTxReciept(ctx, eth, tx.Hash())
		s.Require().Equal(ethtypes.ReceiptStatusSuccessful, receipt.Status)

		transferEvent, err := e2esuite.GetEvmEvent(receipt, s.ics20Contract.ParseICS20Transfer)
		s.Require().NoError(err)
		s.Require().Equal(s.contractAddresses.Erc20, strings.ToLower(transferEvent.Erc20Address.Hex()))
		s.Require().Equal(sdkTransferAmount, transferEvent.PacketData.Amount) // converted from erc20 amount to sdk coin amount
		s.Require().Equal(strings.ToLower(userAddress.Hex()), strings.ToLower(transferEvent.PacketData.Sender))
		s.Require().Equal(receiver.FormattedAddress(), transferEvent.PacketData.Receiver)
		s.Require().Equal("", transferEvent.PacketData.Memo)

		sendPacketEvent, err := e2esuite.GetEvmEvent(receipt, s.ics26Contract.ParseSendPacket)
		s.Require().NoError(err)
		sendPacket = sendPacketEvent.Packet
		s.Require().Equal(uint32(1), sendPacket.Sequence)
		s.Require().Equal(timeout, sendPacket.TimeoutTimestamp)
		s.Require().Equal(transfertypes.PortID, sendPacket.SourcePort)
		s.Require().Equal(s.ethClientID, sendPacket.SourceChannel)
		s.Require().Equal(transfertypes.PortID, sendPacket.DestPort)
		s.Require().Equal(s.simdClientID, sendPacket.DestChannel)
		s.Require().Equal(transfertypes.Version, sendPacket.Version)

		s.True(s.Run("Verify balances", func() {
			userBalance, err := s.erc20Contract.BalanceOf(nil, userAddress)
			s.Require().NoError(err)
			s.Require().Equal(testvalues.StartingERC20TokenAmount-testvalues.ERC20TransferAmount, userBalance.Int64())
			ics20TransferBalance, err := s.erc20Contract.BalanceOf(nil, ics20Address)
			s.Require().NoError(err)
			s.Require().Equal(erc20TransferAmount, ics20TransferBalance)
		}))
	}))

	// TODO: When using a non-mock light client on the cosmos side, the client there needs to be updated at this point

	var recvAck []byte
	var denomOnCosmos transfertypes.DenomTrace
	s.Require().True(s.Run("recvPacket on Cosmos side", func() {
		txResp, err := s.BroadcastMessages(ctx, simd, s.UserB, 200_000, &channeltypes.MsgRecvPacket{
			Packet: channeltypes.Packet{
				Sequence:           uint64(sendPacket.Sequence),
				SourcePort:         sendPacket.SourcePort,
				SourceChannel:      sendPacket.SourceChannel,
				DestinationPort:    sendPacket.DestPort,
				DestinationChannel: sendPacket.DestChannel,
				Data:               sendPacket.Data,
				TimeoutHeight:      clienttypes.Height{},
				TimeoutTimestamp:   sendPacket.TimeoutTimestamp * 1_000_000_000,
			},
			ProofCommitment: []byte("doesn't matter"),
			ProofHeight:     clienttypes.Height{},
			Signer:          s.UserB.FormattedAddress(),
		})
		s.Require().NoError(err)

		recvAck, err = ibctesting.ParseAckFromEvents(txResp.Events)
		s.Require().NoError(err)
		s.Require().NotNil(recvAck)

		s.Require().True(s.Run("Verify balances", func() {
			denomOnCosmos = transfertypes.ParseDenomTrace(
				fmt.Sprintf("%s/%s/%s", transfertypes.PortID, "00-mock-0", s.contractAddresses.Erc20),
			)

			// Check the balance of UserB
			resp, err := e2esuite.GRPCQuery[banktypes.QueryBalanceResponse](ctx, simd, &banktypes.QueryBalanceRequest{
				Address: s.UserB.FormattedAddress(),
				Denom:   denomOnCosmos.IBCDenom(),
			})
			s.Require().NoError(err)
			s.Require().NotNil(resp.Balance)
			s.Require().Equal(sdkmath.NewIntFromBigInt(sdkTransferAmount), resp.Balance.Amount)
			s.Require().Equal(denomOnCosmos.IBCDenom(), resp.Balance.Denom)

			// Check the balance of the SdkICS20Transfer.sol contract
			ics20Bal, err := s.erc20Contract.BalanceOf(nil, ics20Address)
			s.Require().NoError(err)
			s.Require().Equal(testvalues.ERC20TransferAmount, ics20Bal.Int64())

			// Check the balance of the sender
			userBalance, err := s.erc20Contract.BalanceOf(nil, userAddress)
			s.Require().NoError(err)
			s.Require().Equal(testvalues.StartingERC20TokenAmount-testvalues.ERC20TransferAmount, userBalance.Int64())
		}))
	}))

	s.Require().True(s.Run("acknowledgePacket on Ethereum side", func() {
		clientState, err := s.sp1Ics07Contract.GetClientState(nil)
		s.Require().NoError(err)

		trustedHeight := clientState.LatestHeight.RevisionHeight
		latestHeight, err := simd.Height(ctx)
		s.Require().NoError(err)

		// This will be a membership proof since the acknowledgement is written
		packetAckPath := ibchost.PacketAcknowledgementPath(sendPacket.DestPort, sendPacket.DestChannel, uint64(sendPacket.Sequence))
		proofHeight, ucAndMemProof, err := operator.UpdateClientAndMembershipProof(
			uint64(trustedHeight), uint64(latestHeight), packetAckPath,
			"--trust-level", testvalues.DefaultTrustLevel.String(),
			"--trusting-period", strconv.Itoa(testvalues.DefaultTrustPeriod),
		)
		s.Require().NoError(err)

		msg := ics26router.IICS26RouterMsgsMsgAckPacket{
			Packet:          sendPacket,
			Acknowledgement: recvAck,
			ProofAcked:      ucAndMemProof,
			ProofHeight:     *proofHeight,
		}

		tx, err := s.ics26Contract.AckPacket(s.GetTransactOpts(s.key), msg)
		s.Require().NoError(err)

		receipt := s.GetTxReciept(ctx, eth, tx.Hash())
		s.Require().Equal(ethtypes.ReceiptStatusSuccessful, receipt.Status)

		if s.generateFixtures {
			s.Require().NoError(types.GenerateAndSaveFixture("acknowledgePacket.json", s.contractAddresses.Erc20, "ackPacket", msg, sendPacket))
		}

		s.Require().True(s.Run("Verify balances", func() {
			// Check the balance of the SdkICS20Transfer.sol contract
			ics20Bal, err := s.erc20Contract.BalanceOf(nil, ics20Address)
			s.Require().NoError(err)
			s.Require().Equal(testvalues.ERC20TransferAmount, ics20Bal.Int64())

			// Check the balance of the sender
			userBalance, err := s.erc20Contract.BalanceOf(nil, userAddress)
			s.Require().NoError(err)
			s.Require().Equal(testvalues.StartingERC20TokenAmount-testvalues.ERC20TransferAmount, userBalance.Int64())
		}))
	}))

	var returnPacket channeltypes.Packet
	s.Require().True(s.Run("Transfer back", func() {
		// We need the timeout to be a whole number of seconds to be received by eth
		timeout := uint64(time.Now().Add(30*time.Minute).Unix() * 1_000_000_000)
		ibcCoin := sdk.NewCoin(denomOnCosmos.IBCDenom(), sdkmath.NewIntFromBigInt(sdkTransferAmount))

		msgTransfer := transfertypes.MsgTransfer{
			SourcePort:       transfertypes.PortID,
			SourceChannel:    s.simdClientID,
			Token:            ibcCoin,
			Sender:           s.UserB.FormattedAddress(),
			Receiver:         strings.ToLower(userAddress.Hex()),
			TimeoutHeight:    clienttypes.Height{},
			TimeoutTimestamp: timeout,
			Memo:             "",
			DestPort:         transfertypes.PortID,
			DestChannel:      s.ethClientID,
		}

		txResp, err := s.BroadcastMessages(ctx, simd, s.UserB, 200_000, &msgTransfer)
		s.Require().NoError(err)
		returnPacket, err = ibctesting.ParsePacketFromEvents(txResp.Events)
		s.Require().NoError(err)

		s.Require().Equal(uint64(1), returnPacket.Sequence)
		s.Require().Equal(transfertypes.PortID, returnPacket.SourcePort)
		s.Require().Equal(s.simdClientID, returnPacket.SourceChannel)
		s.Require().Equal(transfertypes.PortID, returnPacket.DestinationPort)
		s.Require().Equal(s.ethClientID, returnPacket.DestinationChannel)
		s.Require().Equal(clienttypes.Height{}, returnPacket.TimeoutHeight)
		s.Require().Equal(timeout, returnPacket.TimeoutTimestamp)

		var transferPacketData transfertypes.FungibleTokenPacketData
		err = json.Unmarshal(returnPacket.Data, &transferPacketData)
		s.Require().NoError(err)
		s.Require().Equal(denomOnCosmos.GetFullDenomPath(), transferPacketData.Denom)
		s.Require().Equal(sdkTransferAmount.String(), transferPacketData.Amount)
		s.Require().Equal(s.UserB.FormattedAddress(), transferPacketData.Sender)
		s.Require().Equal(strings.ToLower(userAddress.Hex()), transferPacketData.Receiver)
		s.Require().Equal("", transferPacketData.Memo)

		s.Require().True(s.Run("Verify balances", func() {
			// Check the balance of UserB
			resp, err := e2esuite.GRPCQuery[banktypes.QueryBalanceResponse](ctx, simd, &banktypes.QueryBalanceRequest{
				Address: s.UserB.FormattedAddress(),
				Denom:   denomOnCosmos.IBCDenom(),
			})
			s.Require().NoError(err)
			s.Require().NotNil(resp.Balance)
			s.Require().Equal(int64(0), resp.Balance.Amount.Int64())
			s.Require().Equal(denomOnCosmos.GetFullDenomPath(), resp.Balance.Denom)
		}))
	}))

	var returnWriteAckEvent *ics26router.ContractWriteAcknowledgement
	s.Require().True(s.Run("Receive packet on Ethereum side", func() {
		clientState, err := s.sp1Ics07Contract.GetClientState(nil)
		s.Require().NoError(err)

		trustedHeight := clientState.LatestHeight.RevisionHeight
		latestHeight, err := simd.Height(ctx)
		s.Require().NoError(err)

		packetCommitmentPath := ibchost.PacketCommitmentPath(returnPacket.SourcePort, returnPacket.SourceChannel, returnPacket.Sequence)
		proofHeight, ucAndMemProof, err := operator.UpdateClientAndMembershipProof(
			uint64(trustedHeight), uint64(latestHeight), packetCommitmentPath,
			"--trust-level", testvalues.DefaultTrustLevel.String(),
			"--trusting-period", strconv.Itoa(testvalues.DefaultTrustPeriod),
		)
		s.Require().NoError(err)

		packet := ics26router.IICS26RouterMsgsPacket{
			Sequence:         uint32(returnPacket.Sequence),
			TimeoutTimestamp: returnPacket.TimeoutTimestamp / 1_000_000_000,
			SourcePort:       returnPacket.SourcePort,
			SourceChannel:    returnPacket.SourceChannel,
			DestPort:         returnPacket.DestinationPort,
			DestChannel:      returnPacket.DestinationChannel,
			Version:          transfertypes.Version,
			Data:             returnPacket.Data,
		}
		msg := ics26router.IICS26RouterMsgsMsgRecvPacket{
			Packet:          packet,
			ProofCommitment: ucAndMemProof,
			ProofHeight:     *proofHeight,
		}

		tx, err := s.ics26Contract.RecvPacket(s.GetTransactOpts(s.key), msg)
		s.Require().NoError(err)

		receipt := s.GetTxReciept(ctx, eth, tx.Hash())
		s.Require().Equal(ethtypes.ReceiptStatusSuccessful, receipt.Status)

		if s.generateFixtures {
			s.Require().NoError(types.GenerateAndSaveFixture("receivePacket.json", s.contractAddresses.Erc20, "recvPacket", msg, packet))
		}

		returnWriteAckEvent, err = e2esuite.GetEvmEvent(receipt, s.ics26Contract.ParseWriteAcknowledgement)
		s.Require().NoError(err)

		receiveEvent, err := e2esuite.GetEvmEvent(receipt, s.ics20Contract.ParseICS20ReceiveTransfer)
		s.Require().NoError(err)
		ethReceiveData := receiveEvent.PacketData
		s.Require().Equal(denomOnCosmos.GetFullDenomPath(), ethReceiveData.Denom)
		s.Require().Equal(s.contractAddresses.Erc20, strings.ToLower(receiveEvent.Erc20Address.Hex()))
		s.Require().Equal(s.UserB.FormattedAddress(), ethReceiveData.Sender)
		s.Require().Equal(strings.ToLower(userAddress.Hex()), ethReceiveData.Receiver)
		s.Require().Equal(sdkTransferAmount, ethReceiveData.Amount) // the amount transferred the user on the evm side is converted, but the packet doesn't change
		s.Require().Equal("", ethReceiveData.Memo)

		s.True(s.Run("Verify balances", func() {
			// User balance should be back to the starting point
			userBalance, err := s.erc20Contract.BalanceOf(nil, userAddress)
			s.Require().NoError(err)
			s.Require().Equal(testvalues.StartingERC20TokenAmount, userBalance.Int64())

			ics20TransferBalance, err := s.erc20Contract.BalanceOf(nil, ics20Address)
			s.Require().NoError(err)
			s.Require().Equal(int64(0), ics20TransferBalance.Int64())
		}))
	}))

	// TODO: When using a non-mock light client on the cosmos side, the client there needs to be updated at this point

	s.Require().True(s.Run("acknowledgePacket on Cosmos side", func() {
		resp, err := e2esuite.GRPCQuery[clienttypes.QueryClientStateResponse](ctx, simd, &clienttypes.QueryClientStateRequest{
			ClientId: s.simdClientID,
		})
		s.Require().NoError(err)
		var clientState mock.ClientState
		err = simd.Config().EncodingConfig.Codec.Unmarshal(resp.ClientState.Value, &clientState)
		s.Require().NoError(err)

		txResp, err := s.BroadcastMessages(ctx, simd, s.UserB, 200_000, &channeltypes.MsgAcknowledgement{
			Packet:          returnPacket,
			Acknowledgement: returnWriteAckEvent.Acknowledgement,
			ProofAcked:      []byte("doesn't matter"), // Because mock light client
			ProofHeight:     clienttypes.Height{},
			Signer:          s.UserB.FormattedAddress(),
		})
		s.Require().NoError(err)
		s.Require().Equal(uint32(0), txResp.Code)
	}))
}

func (s *IbcEurekaTestSuite) TestICS20TransferNativeSdkCoin() {
	ctx := context.Background()

	s.SetupSuite(ctx)

	eth, simd := s.ChainA, s.ChainB

	ics20Address := ethcommon.HexToAddress(s.contractAddresses.Ics20Transfer)
	sdkTransferAmount := big.NewInt(testvalues.SDKTransferAmount)
	userAddress := crypto.PubkeyToAddress(s.key.PublicKey)
	sendMemo := "nonnativesend"

	var sendPacket channeltypes.Packet
	var transferCoin sdk.Coin
	s.Require().True(s.Run("Transfer from cosmos side", func() {
		// We need the timeout to be a whole number of seconds to be received by eth
		timeout := uint64(time.Now().Add(30*time.Minute).Unix() * 1_000_000_000)
		transferCoin = sdk.NewCoin(s.ChainB.Config().Denom, sdkmath.NewIntFromBigInt(sdkTransferAmount))

		msgTransfer := transfertypes.MsgTransfer{
			SourcePort:       transfertypes.PortID,
			SourceChannel:    s.simdClientID,
			Token:            transferCoin,
			Sender:           s.UserB.FormattedAddress(),
			Receiver:         strings.ToLower(userAddress.Hex()),
			TimeoutHeight:    clienttypes.Height{},
			TimeoutTimestamp: timeout,
			Memo:             sendMemo,
			DestPort:         transfertypes.PortID,
			DestChannel:      s.ethClientID,
		}

		txResp, err := s.BroadcastMessages(ctx, simd, s.UserB, 200_000, &msgTransfer)
		s.Require().NoError(err)

		sendPacket, err = ibctesting.ParsePacketFromEvents(txResp.Events)
		s.Require().NoError(err)

		s.Require().Equal(uint64(1), sendPacket.Sequence)
		s.Require().Equal(transfertypes.PortID, sendPacket.SourcePort)
		s.Require().Equal(s.simdClientID, sendPacket.SourceChannel)
		s.Require().Equal(transfertypes.PortID, sendPacket.DestinationPort)
		s.Require().Equal(s.ethClientID, sendPacket.DestinationChannel)
		s.Require().Equal(clienttypes.Height{}, sendPacket.TimeoutHeight)
		s.Require().Equal(timeout, sendPacket.TimeoutTimestamp)

		var transferPacketData transfertypes.FungibleTokenPacketData
		err = json.Unmarshal(sendPacket.Data, &transferPacketData)
		s.Require().NoError(err)
		s.Require().Equal(transferCoin.Denom, transferPacketData.Denom)
		s.Require().Equal(sdkTransferAmount.String(), transferPacketData.Amount)
		s.Require().Equal(s.UserB.FormattedAddress(), transferPacketData.Sender)
		s.Require().Equal(strings.ToLower(userAddress.Hex()), transferPacketData.Receiver)
		s.Require().Equal(sendMemo, transferPacketData.Memo)

		s.Require().True(s.Run("Verify balances", func() {
			// Check the balance of UserB
			resp, err := e2esuite.GRPCQuery[banktypes.QueryBalanceResponse](ctx, simd, &banktypes.QueryBalanceRequest{
				Address: s.UserB.FormattedAddress(),
				Denom:   transferCoin.Denom,
			})
			s.Require().NoError(err)
			s.Require().NotNil(resp.Balance)
			s.Require().Equal(testvalues.StartingTokenAmount-testvalues.SDKTransferAmount, resp.Balance.Amount.Int64())
		}))
	}))

	var ethReceiveAckEvent *ics26router.ContractWriteAcknowledgement
	var ethReceiveTransferPacket sdkics20transfer.ICS20LibPacketDataJSON
	var denomOnEthereum transfertypes.DenomTrace
	var ibcERC20 *ibcerc20.Contract
	var ibcERC20Address string
	s.Require().True(s.Run("Receive packet on Ethereum side", func() {
		clientState, err := s.sp1Ics07Contract.GetClientState(nil)
		s.Require().NoError(err)

		trustedHeight := clientState.LatestHeight.RevisionHeight
		latestHeight, err := simd.Height(ctx)
		s.Require().NoError(err)

		packetCommitmentPath := ibchost.PacketCommitmentPath(sendPacket.SourcePort, sendPacket.SourceChannel, sendPacket.Sequence)
		proofHeight, ucAndMemProof, err := operator.UpdateClientAndMembershipProof(
			uint64(trustedHeight), uint64(latestHeight), packetCommitmentPath,
			"--trust-level", testvalues.DefaultTrustLevel.String(),
			"--trusting-period", strconv.Itoa(testvalues.DefaultTrustPeriod),
		)
		s.Require().NoError(err)

		packet := ics26router.IICS26RouterMsgsPacket{
			Sequence:         uint32(sendPacket.Sequence),
			TimeoutTimestamp: sendPacket.TimeoutTimestamp / 1_000_000_000,
			SourcePort:       sendPacket.SourcePort,
			SourceChannel:    sendPacket.SourceChannel,
			DestPort:         sendPacket.DestinationPort,
			DestChannel:      sendPacket.DestinationChannel,
			Version:          transfertypes.Version,
			Data:             sendPacket.Data,
		}
		msg := ics26router.IICS26RouterMsgsMsgRecvPacket{
			Packet:          packet,
			ProofCommitment: ucAndMemProof,
			ProofHeight:     *proofHeight,
		}

		tx, err := s.ics26Contract.RecvPacket(s.GetTransactOpts(s.key), msg)
		s.Require().NoError(err)

		receipt := s.GetTxReciept(ctx, eth, tx.Hash())
		s.Require().Equal(ethtypes.ReceiptStatusSuccessful, receipt.Status)

		if s.generateFixtures {
			s.Require().NoError(types.GenerateAndSaveFixture("receiveNativePacket.json", s.contractAddresses.Erc20, "recvPacket", msg, packet))
		}

		ethReceiveAckEvent, err = e2esuite.GetEvmEvent(receipt, s.ics26Contract.ParseWriteAcknowledgement)
		s.Require().NoError(err)

		ethReceiveTransferEvent, err := e2esuite.GetEvmEvent(receipt, s.ics20Contract.ParseICS20ReceiveTransfer)
		s.Require().NoError(err)

		ethClient, err := ethclient.Dial(eth.GetHostRPCAddress())
		s.Require().NoError(err)
		ibcERC20, err = ibcerc20.NewContract(ethReceiveTransferEvent.Erc20Address, ethClient)
		s.Require().NoError(err)

		ibcERC20Address = strings.ToLower(ethReceiveTransferEvent.Erc20Address.Hex())

		denomOnEthereum = transfertypes.DenomTrace{
			Path:      fmt.Sprintf("%s/%s", sendPacket.DestinationPort, sendPacket.DestinationChannel),
			BaseDenom: transferCoin.Denom,
		}
		actualDenom, err := ibcERC20.Name(nil)
		s.Require().NoError(err)
		s.Require().Equal(denomOnEthereum.IBCDenom(), actualDenom)

		actualBaseDenom, err := ibcERC20.Symbol(nil)
		s.Require().NoError(err)
		s.Require().Equal(transferCoin.Denom, actualBaseDenom)

		actualFullDenom, err := ibcERC20.FullDenomPath(nil)
		s.Require().NoError(err)
		s.Require().Equal(denomOnEthereum.GetFullDenomPath(), actualFullDenom)

		ethReceiveTransferPacket = ethReceiveTransferEvent.PacketData
		s.Require().Equal(transferCoin.Denom, ethReceiveTransferPacket.Denom)
		s.Require().Equal(sdkTransferAmount, ethReceiveTransferPacket.Amount)
		s.Require().Equal(s.UserB.FormattedAddress(), ethReceiveTransferPacket.Sender)
		s.Require().Equal(strings.ToLower(userAddress.Hex()), strings.ToLower(ethReceiveTransferPacket.Receiver))
		s.Require().Equal(sendMemo, ethReceiveTransferPacket.Memo)

		s.True(s.Run("Verify balances", func() {
			userBalance, err := ibcERC20.BalanceOf(nil, userAddress)
			s.Require().NoError(err)
			s.Require().Equal(sdkTransferAmount, userBalance)

			ics20TransferBalance, err := ibcERC20.BalanceOf(nil, ics20Address)
			s.Require().NoError(err)
			s.Require().Equal(int64(0), ics20TransferBalance.Int64())
		}))
	}))

	// TODO: When using a non-mock light client on the cosmos side, the client there needs to be updated at this point

	s.Require().True(s.Run("ack back to cosmos", func() {
		resp, err := e2esuite.GRPCQuery[clienttypes.QueryClientStateResponse](ctx, simd, &clienttypes.QueryClientStateRequest{
			ClientId: s.simdClientID,
		})
		s.Require().NoError(err)
		var clientState mock.ClientState
		err = simd.Config().EncodingConfig.Codec.Unmarshal(resp.ClientState.Value, &clientState)
		s.Require().NoError(err)

		txResp, err := s.BroadcastMessages(ctx, simd, s.UserB, 200_000, &channeltypes.MsgAcknowledgement{
			Packet:          sendPacket,
			Acknowledgement: ethReceiveAckEvent.Acknowledgement,
			ProofAcked:      []byte("doesn't matter"), // Because mock light client
			ProofHeight:     clienttypes.Height{},
			Signer:          s.UserB.FormattedAddress(),
		})
		s.Require().NoError(err)
		s.Require().Equal(uint32(0), txResp.Code)
	}))

	s.Require().True(s.Run("Approve the SdkICS20Transfer.sol contract to spend the erc20 tokens", func() {
		tx, err := ibcERC20.Approve(s.GetTransactOpts(s.key), ics20Address, sdkTransferAmount)
		s.Require().NoError(err)
		receipt := s.GetTxReciept(ctx, eth, tx.Hash())
		s.Require().Equal(ethtypes.ReceiptStatusSuccessful, receipt.Status)

		allowance, err := ibcERC20.Allowance(nil, userAddress, ics20Address)
		s.Require().NoError(err)
		s.Require().Equal(sdkTransferAmount, allowance)
	}))

	var returnPacket ics26router.IICS26RouterMsgsPacket
	returnMemo := "testreturnmemo"
	s.Require().True(s.Run("sendTransfer on Ethereum side", func() {
		timeout := uint64(time.Now().Add(30 * time.Minute).Unix())
		msgSendTransfer := sdkics20transfer.IICS20TransferMsgsSendTransferMsg{
			Denom:            ibcERC20Address,
			Amount:           sdkTransferAmount,
			Receiver:         s.UserB.FormattedAddress(),
			SourceChannel:    s.ethClientID,
			DestPort:         transfertypes.PortID,
			TimeoutTimestamp: timeout,
			Memo:             returnMemo,
		}

		tx, err := s.ics20Contract.SendTransfer(s.GetTransactOpts(s.key), msgSendTransfer)
		s.Require().NoError(err)
		receipt := s.GetTxReciept(ctx, eth, tx.Hash())
		s.Require().Equal(ethtypes.ReceiptStatusSuccessful, receipt.Status)

		transferEvent, err := e2esuite.GetEvmEvent(receipt, s.ics20Contract.ParseICS20Transfer)
		s.Require().NoError(err)
		s.Require().Equal(denomOnEthereum.GetFullDenomPath(), transferEvent.PacketData.Denom)
		s.Require().Equal(sdkTransferAmount, transferEvent.PacketData.Amount) // Here we can see the amount has been converted from the erc20 amount to the sdk amount in the packet
		s.Require().Equal(strings.ToLower(userAddress.Hex()), strings.ToLower(transferEvent.PacketData.Sender))
		s.Require().Equal(s.UserB.FormattedAddress(), transferEvent.PacketData.Receiver)
		s.Require().Equal(returnMemo, transferEvent.PacketData.Memo)

		sendPacketEvent, err := e2esuite.GetEvmEvent(receipt, s.ics26Contract.ParseSendPacket)
		s.Require().NoError(err)
		returnPacket = sendPacketEvent.Packet
		s.Require().Equal(uint32(1), returnPacket.Sequence)
		s.Require().Equal(timeout, returnPacket.TimeoutTimestamp)
		s.Require().Equal(transfertypes.PortID, returnPacket.SourcePort)
		s.Require().Equal(s.ethClientID, returnPacket.SourceChannel)
		s.Require().Equal(transfertypes.PortID, returnPacket.DestPort)
		s.Require().Equal(s.simdClientID, returnPacket.DestChannel)
		s.Require().Equal(transfertypes.Version, returnPacket.Version)

		s.True(s.Run("Verify balances", func() {
			userBalance, err := ibcERC20.BalanceOf(nil, userAddress)
			s.Require().NoError(err)
			s.Require().Equal(int64(0), userBalance.Int64())

			// the whole balance should have been burned
			ics20TransferBalance, err := ibcERC20.BalanceOf(nil, ics20Address)
			s.Require().NoError(err)
			s.Require().Equal(int64(0), ics20TransferBalance.Int64())
		}))
	}))

	// TODO: When using a non-mock light client on the cosmos side, the client there needs to be updated at this point

	var cosmosReceiveAck []byte
	s.Require().True(s.Run("recvPacket on Cosmos side", func() {
		resp, err := e2esuite.GRPCQuery[clienttypes.QueryClientStateResponse](ctx, simd, &clienttypes.QueryClientStateRequest{
			ClientId: s.simdClientID,
		})
		s.Require().NoError(err)
		var clientState mock.ClientState
		err = simd.Config().EncodingConfig.Codec.Unmarshal(resp.ClientState.Value, &clientState)
		s.Require().NoError(err)

		txResp, err := s.BroadcastMessages(ctx, simd, s.UserB, 200_000, &channeltypes.MsgRecvPacket{
			Packet: channeltypes.Packet{
				Sequence:           uint64(returnPacket.Sequence),
				SourcePort:         returnPacket.SourcePort,
				SourceChannel:      returnPacket.SourceChannel,
				DestinationPort:    returnPacket.DestPort,
				DestinationChannel: returnPacket.DestChannel,
				Data:               returnPacket.Data,
				TimeoutHeight:      clienttypes.Height{},
				TimeoutTimestamp:   returnPacket.TimeoutTimestamp * 1_000_000_000,
			},
			ProofCommitment: []byte("doesn't matter"),
			ProofHeight:     clienttypes.Height{},
			Signer:          s.UserB.FormattedAddress(),
		})
		s.Require().NoError(err)

		cosmosReceiveAck, err = ibctesting.ParseAckFromEvents(txResp.Events)
		s.Require().NoError(err)
		s.Require().NotNil(cosmosReceiveAck)

		s.Require().True(s.Run("Verify balances", func() {
			// Check the balance of UserB
			resp, err := e2esuite.GRPCQuery[banktypes.QueryBalanceResponse](ctx, simd, &banktypes.QueryBalanceRequest{
				Address: s.UserB.FormattedAddress(),
				Denom:   transferCoin.Denom,
			})
			s.Require().NoError(err)
			s.Require().NotNil(resp.Balance)
			s.Require().Equal(testvalues.StartingTokenAmount, resp.Balance.Amount.Int64())
		}))
	}))

	s.Require().True(s.Run("acknowledgePacket on Ethereum side", func() {
		clientState, err := s.sp1Ics07Contract.GetClientState(nil)
		s.Require().NoError(err)

		trustedHeight := clientState.LatestHeight.RevisionHeight
		latestHeight, err := simd.Height(ctx)
		s.Require().NoError(err)

		// This will be a membership proof since the acknowledgement is written
		packetAckPath := ibchost.PacketAcknowledgementPath(returnPacket.DestPort, returnPacket.DestChannel, uint64(returnPacket.Sequence))
		proofHeight, ucAndMemProof, err := operator.UpdateClientAndMembershipProof(
			uint64(trustedHeight), uint64(latestHeight), packetAckPath,
			"--trust-level", testvalues.DefaultTrustLevel.String(),
			"--trusting-period", strconv.Itoa(testvalues.DefaultTrustPeriod),
		)
		s.Require().NoError(err)

		msg := ics26router.IICS26RouterMsgsMsgAckPacket{
			Packet:          returnPacket,
			Acknowledgement: cosmosReceiveAck,
			ProofAcked:      ucAndMemProof,
			ProofHeight:     *proofHeight,
		}

		tx, err := s.ics26Contract.AckPacket(s.GetTransactOpts(s.key), msg)
		s.Require().NoError(err)

		receipt := s.GetTxReciept(ctx, eth, tx.Hash())
		s.Require().Equal(ethtypes.ReceiptStatusSuccessful, receipt.Status)
	}))
}

func (s *IbcEurekaTestSuite) TestICS20Timeout() {
	ctx := context.Background()

	s.SetupSuite(ctx)

	eth, simd := s.ChainA, s.ChainB

	transferAmount := big.NewInt(testvalues.ERC20TransferAmount)
	userAddress := crypto.PubkeyToAddress(s.key.PublicKey)
	receiver := s.UserB

	var packet ics26router.IICS26RouterMsgsPacket
	s.Require().True(s.Run("Approve the SdkICS20Transfer.sol contract to spend the erc20 tokens", func() {
		ics20Address := ethcommon.HexToAddress(s.contractAddresses.Ics20Transfer)
		tx, err := s.erc20Contract.Approve(s.GetTransactOpts(s.key), ics20Address, transferAmount)
		s.Require().NoError(err)
		receipt := s.GetTxReciept(ctx, eth, tx.Hash())
		s.Require().Equal(ethtypes.ReceiptStatusSuccessful, receipt.Status)

		allowance, err := s.erc20Contract.Allowance(nil, userAddress, ics20Address)
		s.Require().NoError(err)
		s.Require().Equal(transferAmount, allowance)
	}))

	var timeout uint64
	s.Require().True(s.Run("sendTransfer on Ethereum side", func() {
		timeout = uint64(time.Now().Add(30 * time.Second).Unix())
		msgSendTransfer := sdkics20transfer.IICS20TransferMsgsSendTransferMsg{
			Denom:            s.contractAddresses.Erc20,
			Amount:           transferAmount,
			Receiver:         receiver.FormattedAddress(),
			SourceChannel:    s.ethClientID,
			DestPort:         "transfer",
			TimeoutTimestamp: timeout,
			Memo:             "testmemo",
		}

		tx, err := s.ics20Contract.SendTransfer(s.GetTransactOpts(s.key), msgSendTransfer)
		s.Require().NoError(err)
		receipt := s.GetTxReciept(ctx, eth, tx.Hash())
		s.Require().Equal(ethtypes.ReceiptStatusSuccessful, receipt.Status)

		transferEvent, err := e2esuite.GetEvmEvent(receipt, s.ics20Contract.ParseICS20Transfer)
		s.Require().NoError(err)
		s.Require().Equal(s.contractAddresses.Erc20, strings.ToLower(transferEvent.Erc20Address.Hex()))
		s.Require().Equal(testvalues.SDKTransferAmount, transferEvent.PacketData.Amount.Int64()) // Because the amount is converted to the sdk amount
		s.Require().Equal(strings.ToLower(userAddress.Hex()), strings.ToLower(transferEvent.PacketData.Sender))
		s.Require().Equal(receiver.FormattedAddress(), transferEvent.PacketData.Receiver)
		s.Require().Equal("testmemo", transferEvent.PacketData.Memo)

		sendPacketEvent, err := e2esuite.GetEvmEvent(receipt, s.ics26Contract.ParseSendPacket)
		s.Require().NoError(err)
		packet = sendPacketEvent.Packet
		s.Require().Equal(uint32(1), packet.Sequence)
		s.Require().Equal(timeout, packet.TimeoutTimestamp)
		s.Require().Equal("transfer", packet.SourcePort)
		s.Require().Equal(s.ethClientID, packet.SourceChannel)
		s.Require().Equal("transfer", packet.DestPort)
		s.Require().Equal(s.simdClientID, packet.DestChannel)
		s.Require().Equal(transfertypes.Version, packet.Version)

		s.Require().True(s.Run("Verify balances", func() {
			// Check the balance of the SdkICS20Transfer.sol contract
			ics20Address := ethcommon.HexToAddress(s.contractAddresses.Ics20Transfer)
			ics20Bal, err := s.erc20Contract.BalanceOf(nil, ics20Address)
			s.Require().NoError(err)
			s.Require().Equal(transferAmount, ics20Bal)

			// Check the balance of the sender
			userBalance, err := s.erc20Contract.BalanceOf(nil, userAddress)
			s.Require().NoError(err)
			s.Require().Equal(testvalues.StartingERC20TokenAmount-testvalues.ERC20TransferAmount, userBalance.Int64())
		}))
	}))

	// sleep for 45 seconds to let the packet timeout
	time.Sleep(45 * time.Second)

	s.True(s.Run("timeoutPacket on Ethereum", func() {
		clientState, err := s.sp1Ics07Contract.GetClientState(nil)
		s.Require().NoError(err)

		trustedHeight := clientState.LatestHeight.RevisionHeight
		latestHeight, err := simd.Height(ctx)
		s.Require().NoError(err)

		// This will be a non-membership proof since no packets have been sent
		packetReceiptPath := ibchost.PacketReceiptPath(packet.DestPort, packet.DestChannel, uint64(packet.Sequence))
		proofHeight, ucAndMemProof, err := operator.UpdateClientAndMembershipProof(
			uint64(trustedHeight), uint64(latestHeight), packetReceiptPath,
			"--trust-level", testvalues.DefaultTrustLevel.String(),
			"--trusting-period", strconv.Itoa(testvalues.DefaultTrustPeriod),
		)
		s.Require().NoError(err)

		msg := ics26router.IICS26RouterMsgsMsgTimeoutPacket{
			Packet:       packet,
			ProofTimeout: ucAndMemProof,
			ProofHeight:  *proofHeight,
		}

		tx, err := s.ics26Contract.TimeoutPacket(s.GetTransactOpts(s.key), msg)
		s.Require().NoError(err)

		receipt := s.GetTxReciept(ctx, eth, tx.Hash())
		s.Require().Equal(ethtypes.ReceiptStatusSuccessful, receipt.Status)

		if s.generateFixtures {
			s.Require().NoError(types.GenerateAndSaveFixture("timeoutPacket.json", s.contractAddresses.Erc20, "timeoutPacket", msg, packet))
		}

		s.Require().True(s.Run("Verify balances", func() {
			// Check the balance of the SdkICS20Transfer.sol contract
			ics20Address := ethcommon.HexToAddress(s.contractAddresses.Ics20Transfer)
			ics20Bal, err := s.erc20Contract.BalanceOf(nil, ics20Address)
			s.Require().NoError(err)
			s.Require().Equal(int64(0), ics20Bal.Int64())

			// Check the balance of the sender
			userBalance, err := s.erc20Contract.BalanceOf(nil, userAddress)
			s.Require().NoError(err)
			s.Require().Equal(testvalues.StartingERC20TokenAmount, userBalance.Int64())
		}))
	}))
}
