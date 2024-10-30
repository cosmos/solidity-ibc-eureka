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
	ibctesting "github.com/cosmos/ibc-go/v8/testing"

	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/e2esuite"
	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/ethereum"
	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/operator"
	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/testvalues"
	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/types"
	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/types/erc20"
	ethereumligthclient "github.com/srdtrk/solidity-ibc-eureka/e2e/v8/types/ethereumlightclient"
	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/types/ibcerc20"
	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/types/ics02client"
	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/types/ics20transfer"
	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/types/ics26router"
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
	deployer *ecdsa.PrivateKey

	contractAddresses ethereum.DeployedContracts

	sp1Ics07Contract   *sp1ics07tendermint.Contract
	ics02Contract      *ics02client.Contract
	ics26Contract      *ics26router.Contract
	ics20Contract      *ics20transfer.Contract
	erc20Contract      *erc20.Contract
	escrowContractAddr ethcommon.Address
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

		s.key, err = eth.CreateAndFundUser()
		fmt.Println(err)
		time.Sleep(5 * time.Minute)
		s.Require().NoError(err)

		operatorKey, err := eth.CreateAndFundUser()
		s.Require().NoError(err)

		s.deployer, err = eth.CreateAndFundUser()
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
		os.Setenv(testvalues.EnvKeyEthRPC, eth.RPC)
		os.Setenv(testvalues.EnvKeyTendermintRPC, simd.GetHostRPCAddress())
		os.Setenv(testvalues.EnvKeySp1Prover, prover)
		os.Setenv(testvalues.EnvKeyOperatorPrivateKey, hex.EncodeToString(crypto.FromECDSA(operatorKey)))
		if os.Getenv(testvalues.EnvKeyGenerateFixtures) == testvalues.EnvValueGenerateFixtures_True {
			s.generateFixtures = true
		}
	}))

	s.Require().True(s.Run("Deploy ethereum contracts", func() {
		s.Require().NoError(operator.RunGenesis(
			"--trust-level", testvalues.DefaultTrustLevel.String(),
			"--trusting-period", strconv.Itoa(testvalues.DefaultTrustPeriod),
			"-o", testvalues.Sp1GenesisFilePath,
		))

		var (
			stdout []byte
			err    error
		)
		switch prover {
		case testvalues.EnvValueSp1Prover_Mock:
			stdout, err = eth.ForgeScript(s.deployer, "scripts/MockE2ETestDeploy.s.sol:MockE2ETestDeploy")
			s.Require().NoError(err)
		case testvalues.EnvValueSp1Prover_Network:
			// make sure that the SP1_PRIVATE_KEY is set.
			s.Require().NotEmpty(os.Getenv(testvalues.EnvKeySp1PrivateKey))

			stdout, err = eth.ForgeScript(s.deployer, "scripts/E2ETestDeploy.s.sol:E2ETestDeploy")
			s.Require().NoError(err)
		default:
			s.Require().Fail("invalid prover type: %s", prover)
		}

		ethClient, err := ethclient.Dial(eth.RPC)
		s.Require().NoError(err)

		s.contractAddresses, err = ethereum.GetEthContractsFromDeployOutput(string(stdout))
		s.Require().NoError(err)
		s.sp1Ics07Contract, err = sp1ics07tendermint.NewContract(ethcommon.HexToAddress(s.contractAddresses.Ics07Tendermint), ethClient)
		s.Require().NoError(err)
		s.ics02Contract, err = ics02client.NewContract(ethcommon.HexToAddress(s.contractAddresses.Ics02Client), ethClient)
		s.Require().NoError(err)
		s.ics26Contract, err = ics26router.NewContract(ethcommon.HexToAddress(s.contractAddresses.Ics26Router), ethClient)
		s.Require().NoError(err)
		s.ics20Contract, err = ics20transfer.NewContract(ethcommon.HexToAddress(s.contractAddresses.Ics20Transfer), ethClient)
		s.Require().NoError(err)
		s.erc20Contract, err = erc20.NewContract(ethcommon.HexToAddress(s.contractAddresses.Erc20), ethClient)
		s.Require().NoError(err)
		s.escrowContractAddr = ethcommon.HexToAddress(s.contractAddresses.Escrow)
	}))

	s.T().Cleanup(func() {
		_ = os.Remove(testvalues.Sp1GenesisFilePath)
	})

	s.Require().True(s.Run("Fund address with ERC20", func() {
		tx, err := s.erc20Contract.Transfer(s.GetTransactOpts(eth.Faucet), crypto.PubkeyToAddress(s.key.PublicKey), big.NewInt(testvalues.InitialBalance))
		s.Require().NoError(err)

		_ = s.GetTxReciept(ctx, eth, tx.Hash()) // wait for the tx to be mined
	}))

	_, simdRelayerUser := s.GetRelayerUsers(ctx)

	s.Require().True(s.Run("Add ethereum light client on Cosmos chain", func() {
		s.CreateEthereumLightClient(ctx, simdRelayerUser, s.contractAddresses.Ics07Tendermint)
	}))

	s.Require().True(s.Run("Add client and counterparty on EVM", func() {
		counterpartyInfo := ics02client.IICS02ClientMsgsCounterpartyInfo{
			ClientId:     s.EthereumLightClientID,
			MerklePrefix: [][]byte{[]byte(ibcexported.StoreKey), []byte("")},
		}
		lightClientAddress := ethcommon.HexToAddress(s.contractAddresses.Ics07Tendermint)
		tx, err := s.ics02Contract.AddClient(s.GetTransactOpts(s.key), ibcexported.Tendermint, counterpartyInfo, lightClientAddress)
		s.Require().NoError(err)

		receipt := s.GetTxReciept(ctx, eth, tx.Hash())
		event, err := e2esuite.GetEvmEvent(receipt, s.ics02Contract.ParseICS02ClientAdded)
		s.Require().NoError(err)
		s.Require().Equal(ibctesting.FirstClientID, event.ClientId)
		s.Require().Equal(s.EthereumLightClientID, event.CounterpartyInfo.ClientId)
		s.TendermintLightClientID = event.ClientId
	}))

	s.Require().True(s.Run("Register counterparty on Cosmos chain", func() {
		merklePathPrefix := commitmenttypes.NewMerklePath([]byte(""))

		_, err := s.BroadcastMessages(ctx, simd, simdRelayerUser, 200_000, &clienttypes.MsgProvideCounterparty{
			ClientId:         s.EthereumLightClientID,
			CounterpartyId:   s.TendermintLightClientID,
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

	eth, simd := s.ChainA, s.ChainB

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
		s.Require().Equal(strings.ToLower(crypto.PubkeyToAddress(s.deployer.PublicKey).Hex()), strings.ToLower(owner.Hex()))

		clientAddress, err := s.ics02Contract.GetClient(nil, s.TendermintLightClientID)
		s.Require().NoError(err)
		s.Require().Equal(s.contractAddresses.Ics07Tendermint, strings.ToLower(clientAddress.Hex()))

		counterpartyInfo, err := s.ics02Contract.GetCounterparty(nil, s.TendermintLightClientID)
		s.Require().NoError(err)
		s.Require().Equal(s.EthereumLightClientID, counterpartyInfo.ClientId)
	}))

	s.Require().True(s.Run("Verify ICS26 Router", func() {
		owner, err := s.ics26Contract.Owner(nil)
		s.Require().NoError(err)
		s.Require().Equal(strings.ToLower(crypto.PubkeyToAddress(s.deployer.PublicKey).Hex()), strings.ToLower(owner.Hex()))

		transferAddress, err := s.ics26Contract.GetIBCApp(nil, transfertypes.PortID)
		s.Require().NoError(err)
		s.Require().Equal(s.contractAddresses.Ics20Transfer, strings.ToLower(transferAddress.Hex()))
	}))

	s.Require().True(s.Run("Verify ERC20 Genesis", func() {
		userBalance, err := s.erc20Contract.BalanceOf(nil, crypto.PubkeyToAddress(s.key.PublicKey))
		s.Require().NoError(err)
		s.Require().Equal(testvalues.InitialBalance, userBalance.Int64())
	}))

	var latestClientStateHeight clienttypes.Height
	s.Require().True(s.Run("Verify ethereum wasm light client state", func() {
		wasmClientState, unionClientState := s.GetUnionClientState(ctx, s.EthereumLightClientID)

		s.Require().NotZero(wasmClientState.LatestHeight.RevisionHeight)
		latestClientStateHeight = wasmClientState.LatestHeight

		s.Require().Equal(eth.ChainID.String(), unionClientState.ChainId)
	}))

	s.Require().True(s.Run("Verify ethereum wasm light client consensus state", func() {
		wasmConsensusState, unionConsensusState := s.GetUnionConsensusState(ctx, s.EthereumLightClientID, latestClientStateHeight)
		s.Require().NoError(wasmConsensusState.ValidateBasic())
		s.Require().Equal(latestClientStateHeight.RevisionHeight, unionConsensusState.Slot)
	}))
}

// TestICS20TransferERC20TokenfromEthereumToCosmosAndBack tests the ICS20 transfer functionality
// by transferring ERC20 tokens from Ethereum to Cosmos chain
// and then back from Cosmos chain to Ethereum
func (s *IbcEurekaTestSuite) TestICS20TransferERC20TokenfromEthereumToCosmosAndBack() {
	ctx := context.Background()

	s.SetupSuite(ctx)

	eth, simd := s.ChainA, s.ChainB

	ics20Address := ethcommon.HexToAddress(s.contractAddresses.Ics20Transfer)
	transferAmount := big.NewInt(testvalues.TransferAmount)
	ethereumUserAddress := crypto.PubkeyToAddress(s.key.PublicKey)
	cosmosUserWallet := s.UserB
	cosmosUserAddress := cosmosUserWallet.FormattedAddress()
	_, simdRelayerUser := s.GetRelayerUsers(ctx)

	s.Require().True(s.Run("Approve the ICS20Transfer.sol contract to spend the erc20 tokens", func() {
		tx, err := s.erc20Contract.Approve(s.GetTransactOpts(s.key), ics20Address, transferAmount)
		s.Require().NoError(err)
		receipt := s.GetTxReciept(ctx, eth, tx.Hash())
		s.Require().Equal(ethtypes.ReceiptStatusSuccessful, receipt.Status)

		allowance, err := s.erc20Contract.Allowance(nil, ethereumUserAddress, ics20Address)
		s.Require().NoError(err)
		s.Require().Equal(transferAmount, allowance)
	}))

	var sendPacket ics26router.IICS26RouterMsgsPacket
	var sendBlockNumber int64
	s.Require().True(s.Run("sendTransfer on Ethereum", func() {
		timeout := uint64(time.Now().Add(30 * time.Minute).Unix())
		msgSendTransfer := ics20transfer.IICS20TransferMsgsSendTransferMsg{
			Denom:            s.contractAddresses.Erc20,
			Amount:           transferAmount,
			Receiver:         cosmosUserAddress,
			SourceChannel:    s.TendermintLightClientID,
			DestPort:         transfertypes.PortID,
			TimeoutTimestamp: timeout,
			Memo:             "",
		}

		tx, err := s.ics20Contract.SendTransfer(s.GetTransactOpts(s.key), msgSendTransfer)
		s.Require().NoError(err)
		receipt := s.GetTxReciept(ctx, eth, tx.Hash())
		s.Require().Equal(ethtypes.ReceiptStatusSuccessful, receipt.Status)
		sendBlockNumber = receipt.BlockNumber.Int64()

		transferEvent, err := e2esuite.GetEvmEvent(receipt, s.ics20Contract.ParseICS20Transfer)
		s.Require().NoError(err)
		s.Require().Equal(s.contractAddresses.Erc20, strings.ToLower(transferEvent.Erc20Address.Hex()))
		s.Require().Equal(transferAmount, transferEvent.PacketData.Amount) // converted from erc20 amount to sdk coin amount
		s.Require().Equal(strings.ToLower(ethereumUserAddress.Hex()), strings.ToLower(transferEvent.PacketData.Sender))
		s.Require().Equal(cosmosUserAddress, transferEvent.PacketData.Receiver)
		s.Require().Equal("", transferEvent.PacketData.Memo)

		sendPacketEvent, err := e2esuite.GetEvmEvent(receipt, s.ics26Contract.ParseSendPacket)
		s.Require().NoError(err)
		sendPacket = sendPacketEvent.Packet
		s.Require().Equal(uint32(1), sendPacket.Sequence)
		s.Require().Equal(timeout, sendPacket.TimeoutTimestamp)
		s.Require().Equal(transfertypes.PortID, sendPacket.SourcePort)
		s.Require().Equal(s.TendermintLightClientID, sendPacket.SourceChannel)
		s.Require().Equal(transfertypes.PortID, sendPacket.DestPort)
		s.Require().Equal(s.EthereumLightClientID, sendPacket.DestChannel)
		s.Require().Equal(transfertypes.Version, sendPacket.Version)

		s.True(s.Run("Verify balances on Ethereum", func() {
			// User balance on Ethereum
			userBalance, err := s.erc20Contract.BalanceOf(nil, ethereumUserAddress)
			s.Require().NoError(err)
			s.Require().Equal(testvalues.InitialBalance-testvalues.TransferAmount, userBalance.Int64())

			// ICS20 contract balance on Ethereum
			escrowBalance, err := s.erc20Contract.BalanceOf(nil, s.escrowContractAddr)
			s.Require().NoError(err)
			s.Require().Equal(transferAmount, escrowBalance)
		}))
	}))

	var recvAck []byte
	var denomOnCosmos transfertypes.DenomTrace
	s.Require().True(s.Run("recvPacket on Cosmos chain", func() {
		s.UpdateEthClient(ctx, s.contractAddresses.Ics26Router, sendBlockNumber, simdRelayerUser)

		path := fmt.Sprintf("commitments/ports/%s/channels/%s/sequences/%d", sendPacket.SourcePort, sendPacket.SourceChannel, sendPacket.Sequence)
		storageProofBz := s.getCommitmentProof(path)

		packet := channeltypes.Packet{
			Sequence:           uint64(sendPacket.Sequence),
			SourcePort:         sendPacket.SourcePort,
			SourceChannel:      sendPacket.SourceChannel,
			DestinationPort:    sendPacket.DestPort,
			DestinationChannel: sendPacket.DestChannel,
			Data:               sendPacket.Data,
			TimeoutHeight:      clienttypes.Height{},
			TimeoutTimestamp:   sendPacket.TimeoutTimestamp * 1_000_000_000,
		}

		txResp, err := s.BroadcastMessages(ctx, simd, cosmosUserWallet, 200_000, &channeltypes.MsgRecvPacket{
			Packet:          packet,
			ProofCommitment: storageProofBz,
			ProofHeight: clienttypes.Height{
				RevisionNumber: 0,
				RevisionHeight: s.LastEtheruemLightClientUpdate,
			},
			Signer: cosmosUserAddress,
		})
		s.Require().NoError(err)

		recvAck, err = ibctesting.ParseAckFromEvents(txResp.Events)
		s.Require().NoError(err)
		s.Require().NotNil(recvAck)

		s.Require().True(s.Run("Verify balances on Cosmos chain", func() {
			denomOnCosmos = transfertypes.ParseDenomTrace(
				fmt.Sprintf("%s/%s/%s", transfertypes.PortID, s.EthereumLightClientID, s.contractAddresses.Erc20),
			)

			// User balance on Cosmos chain
			resp, err := e2esuite.GRPCQuery[banktypes.QueryBalanceResponse](ctx, simd, &banktypes.QueryBalanceRequest{
				Address: cosmosUserAddress,
				Denom:   denomOnCosmos.IBCDenom(),
			})
			s.Require().NoError(err)
			s.Require().NotNil(resp.Balance)
			s.Require().Equal(sdkmath.NewIntFromBigInt(transferAmount), resp.Balance.Amount)
			s.Require().Equal(denomOnCosmos.IBCDenom(), resp.Balance.Denom)
		}))
	}))

	s.Require().True(s.Run("acknowledgePacket on Ethereum", func() {
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

		s.Require().True(s.Run("Verify balances on Ethereum", func() {
			// User balance on Ethereum
			userBalance, err := s.erc20Contract.BalanceOf(nil, ethereumUserAddress)
			s.Require().NoError(err)
			s.Require().Equal(testvalues.InitialBalance-testvalues.TransferAmount, userBalance.Int64())

			// ICS20 contract balance on Ethereum
			escrowBalance, err := s.erc20Contract.BalanceOf(nil, s.escrowContractAddr)
			s.Require().NoError(err)
			s.Require().Equal(testvalues.TransferAmount, escrowBalance.Int64())
		}))
	}))

	var returnPacket channeltypes.Packet
	s.Require().True(s.Run("Transfer tokens back from Cosmos chain", func() {
		// We need the timeout to be a whole number of seconds to be received by eth
		timeout := uint64(time.Now().Add(30*time.Minute).Unix() * 1_000_000_000)
		ibcCoin := sdk.NewCoin(denomOnCosmos.IBCDenom(), sdkmath.NewIntFromBigInt(transferAmount))

		msgTransfer := transfertypes.MsgTransfer{
			SourcePort:       transfertypes.PortID,
			SourceChannel:    s.EthereumLightClientID,
			Token:            ibcCoin,
			Sender:           cosmosUserAddress,
			Receiver:         strings.ToLower(ethereumUserAddress.Hex()),
			TimeoutHeight:    clienttypes.Height{},
			TimeoutTimestamp: timeout,
			Memo:             "",
			DestPort:         transfertypes.PortID,
			DestChannel:      s.TendermintLightClientID,
		}

		txResp, err := s.BroadcastMessages(ctx, simd, cosmosUserWallet, 200_000, &msgTransfer)
		s.Require().NoError(err)
		returnPacket, err = ibctesting.ParsePacketFromEvents(txResp.Events)
		s.Require().NoError(err)

		s.Require().Equal(uint64(1), returnPacket.Sequence)
		s.Require().Equal(transfertypes.PortID, returnPacket.SourcePort)
		s.Require().Equal(s.EthereumLightClientID, returnPacket.SourceChannel)
		s.Require().Equal(transfertypes.PortID, returnPacket.DestinationPort)
		s.Require().Equal(s.TendermintLightClientID, returnPacket.DestinationChannel)
		s.Require().Equal(clienttypes.Height{}, returnPacket.TimeoutHeight)
		s.Require().Equal(timeout, returnPacket.TimeoutTimestamp)

		var transferPacketData transfertypes.FungibleTokenPacketData
		err = json.Unmarshal(returnPacket.Data, &transferPacketData)
		s.Require().NoError(err)
		s.Require().Equal(denomOnCosmos.GetFullDenomPath(), transferPacketData.Denom)
		s.Require().Equal(transferAmount.String(), transferPacketData.Amount)
		s.Require().Equal(cosmosUserAddress, transferPacketData.Sender)
		s.Require().Equal(strings.ToLower(ethereumUserAddress.Hex()), transferPacketData.Receiver)
		s.Require().Equal("", transferPacketData.Memo)

		s.Require().True(s.Run("Verify balances on Cosmos chain", func() {
			// User balance on Cosmos chain
			resp, err := e2esuite.GRPCQuery[banktypes.QueryBalanceResponse](ctx, simd, &banktypes.QueryBalanceRequest{
				Address: cosmosUserAddress,
				Denom:   denomOnCosmos.IBCDenom(),
			})
			s.Require().NoError(err)
			s.Require().NotNil(resp.Balance)
			s.Require().Equal(sdkmath.ZeroInt(), resp.Balance.Amount)
			s.Require().Equal(denomOnCosmos.GetFullDenomPath(), resp.Balance.Denom)
		}))
	}))

	var recvBlockNumber int64
	var returnWriteAckEvent *ics26router.ContractWriteAcknowledgement
	s.Require().True(s.Run("Receive packet on Ethereum", func() {
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

		recvBlockNumber = receipt.BlockNumber.Int64()

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
		s.Require().Equal(cosmosUserAddress, ethReceiveData.Sender)
		s.Require().Equal(strings.ToLower(ethereumUserAddress.Hex()), ethReceiveData.Receiver)
		s.Require().Equal(transferAmount, ethReceiveData.Amount) // the amount transferred the user on the evm side is converted, but the packet doesn't change
		s.Require().Equal("", ethReceiveData.Memo)

		s.True(s.Run("Verify balances on Ethereum", func() {
			// User balance should be back to the starting point
			userBalance, err := s.erc20Contract.BalanceOf(nil, ethereumUserAddress)
			s.Require().NoError(err)
			s.Require().Equal(testvalues.InitialBalance, userBalance.Int64())

			escrowBalance, err := s.erc20Contract.BalanceOf(nil, s.escrowContractAddr)
			s.Require().NoError(err)
			s.Require().Equal(int64(0), escrowBalance.Int64())
		}))
	}))

	s.Require().True(s.Run("Acknowledge packet on Cosmos chain", func() {
		s.UpdateEthClient(ctx, s.contractAddresses.Ics26Router, recvBlockNumber, simdRelayerUser)

		path := fmt.Sprintf("acks/ports/%s/channels/%s/sequences/%d", returnPacket.DestinationPort, returnPacket.DestinationChannel, returnPacket.Sequence)
		storageProofBz := s.getCommitmentProof(path)
		txResp, err := s.BroadcastMessages(ctx, simd, cosmosUserWallet, 200_000, &channeltypes.MsgAcknowledgement{
			Packet:          returnPacket,
			Acknowledgement: returnWriteAckEvent.Acknowledgement,
			ProofAcked:      storageProofBz,
			ProofHeight: clienttypes.Height{
				RevisionNumber: 0,
				RevisionHeight: s.LastEtheruemLightClientUpdate,
			},
			Signer: cosmosUserAddress,
		})
		s.Require().NoError(err)
		s.Require().Equal(uint32(0), txResp.Code)
	}))
}

// TestICS20TransferNativeCosmosCoinsToEthereumAndBack tests the ICS20 transfer functionality
// by transferring native coins from a Cosmos chain to Ethereum and back
func (s *IbcEurekaTestSuite) TestICS20TransferNativeCosmosCoinsToEthereumAndBack() {
	ctx := context.Background()

	s.SetupSuite(ctx)

	eth, simd := s.ChainA, s.ChainB

	ics20Address := ethcommon.HexToAddress(s.contractAddresses.Ics20Transfer)
	transferAmount := big.NewInt(testvalues.TransferAmount)
	ethereumUserAddress := crypto.PubkeyToAddress(s.key.PublicKey)
	cosmosUserWallet := s.UserB
	cosmosUserAddress := cosmosUserWallet.FormattedAddress()
	_, simdRelayerUser := s.GetRelayerUsers(ctx)
	sendMemo := "nonnativesend"

	var sendPacket channeltypes.Packet
	var transferCoin sdk.Coin
	s.Require().True(s.Run("Send transfer on Cosmos chain", func() {
		// We need the timeout to be a whole number of seconds to be received by eth
		timeout := uint64(time.Now().Add(30*time.Minute).Unix() * 1_000_000_000)
		transferCoin = sdk.NewCoin(s.ChainB.Config().Denom, sdkmath.NewIntFromBigInt(transferAmount))

		msgTransfer := transfertypes.MsgTransfer{
			SourcePort:       transfertypes.PortID,
			SourceChannel:    s.EthereumLightClientID,
			Token:            transferCoin,
			Sender:           cosmosUserAddress,
			Receiver:         strings.ToLower(ethereumUserAddress.Hex()),
			TimeoutHeight:    clienttypes.Height{},
			TimeoutTimestamp: timeout,
			Memo:             sendMemo,
			DestPort:         transfertypes.PortID,
			DestChannel:      s.TendermintLightClientID,
		}

		txResp, err := s.BroadcastMessages(ctx, simd, cosmosUserWallet, 200_000, &msgTransfer)
		s.Require().NoError(err)

		sendPacket, err = ibctesting.ParsePacketFromEvents(txResp.Events)
		s.Require().NoError(err)

		s.Require().Equal(uint64(1), sendPacket.Sequence)
		s.Require().Equal(transfertypes.PortID, sendPacket.SourcePort)
		s.Require().Equal(s.EthereumLightClientID, sendPacket.SourceChannel)
		s.Require().Equal(transfertypes.PortID, sendPacket.DestinationPort)
		s.Require().Equal(s.TendermintLightClientID, sendPacket.DestinationChannel)
		s.Require().Equal(clienttypes.Height{}, sendPacket.TimeoutHeight)
		s.Require().Equal(timeout, sendPacket.TimeoutTimestamp)

		var transferPacketData transfertypes.FungibleTokenPacketData
		err = json.Unmarshal(sendPacket.Data, &transferPacketData)
		s.Require().NoError(err)
		s.Require().Equal(transferCoin.Denom, transferPacketData.Denom)
		s.Require().Equal(transferAmount.String(), transferPacketData.Amount)
		s.Require().Equal(cosmosUserAddress, transferPacketData.Sender)
		s.Require().Equal(strings.ToLower(ethereumUserAddress.Hex()), transferPacketData.Receiver)
		s.Require().Equal(sendMemo, transferPacketData.Memo)

		s.Require().True(s.Run("Verify balances on Cosmos chain", func() {
			// Check the balance of UserB
			resp, err := e2esuite.GRPCQuery[banktypes.QueryBalanceResponse](ctx, simd, &banktypes.QueryBalanceRequest{
				Address: cosmosUserAddress,
				Denom:   transferCoin.Denom,
			})
			s.Require().NoError(err)
			s.Require().NotNil(resp.Balance)
			s.Require().Equal(testvalues.InitialBalance-testvalues.TransferAmount, resp.Balance.Amount.Int64())
		}))
	}))

	var ethReceiveAckEvent *ics26router.ContractWriteAcknowledgement
	var ethReceiveTransferPacket ics20transfer.ICS20LibPacketDataJSON
	var denomOnEthereum transfertypes.DenomTrace
	var ibcERC20 *ibcerc20.Contract
	var ibcERC20Address string
	var recvBlockNumber int64
	s.Require().True(s.Run("Receive packet on Ethereum", func() {
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

		recvBlockNumber = receipt.BlockNumber.Int64()

		if s.generateFixtures {
			s.Require().NoError(types.GenerateAndSaveFixture("receiveNativePacket.json", s.contractAddresses.Erc20, "recvPacket", msg, packet))
		}

		ethReceiveAckEvent, err = e2esuite.GetEvmEvent(receipt, s.ics26Contract.ParseWriteAcknowledgement)
		s.Require().NoError(err)

		ethReceiveTransferEvent, err := e2esuite.GetEvmEvent(receipt, s.ics20Contract.ParseICS20ReceiveTransfer)
		s.Require().NoError(err)

		ethClient, err := ethclient.Dial(eth.RPC)
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
		s.Require().Equal(transferAmount, ethReceiveTransferPacket.Amount)
		s.Require().Equal(cosmosUserAddress, ethReceiveTransferPacket.Sender)
		s.Require().Equal(strings.ToLower(ethereumUserAddress.Hex()), strings.ToLower(ethReceiveTransferPacket.Receiver))
		s.Require().Equal(sendMemo, ethReceiveTransferPacket.Memo)

		s.True(s.Run("Verify balances on Ethereum", func() {
			// User balance on Ethereum
			userBalance, err := ibcERC20.BalanceOf(nil, ethereumUserAddress)
			s.Require().NoError(err)
			s.Require().Equal(transferAmount, userBalance)

			// ICS20 contract balance on Ethereum
			ics20TransferBalance, err := ibcERC20.BalanceOf(nil, ics20Address)
			s.Require().NoError(err)
			s.Require().Equal(int64(0), ics20TransferBalance.Int64())
		}))
	}))

	s.Require().True(s.Run("Acknowledge packet on Cosmos chain", func() {
		s.UpdateEthClient(ctx, s.contractAddresses.Ics26Router, recvBlockNumber, simdRelayerUser)

		path := fmt.Sprintf("acks/ports/%s/channels/%s/sequences/%d", sendPacket.DestinationPort, sendPacket.DestinationChannel, sendPacket.Sequence)
		storageProofBz := s.getCommitmentProof(path)

		txResp, err := s.BroadcastMessages(ctx, simd, cosmosUserWallet, 200_000, &channeltypes.MsgAcknowledgement{
			Packet:          sendPacket, // TODO: Does this need to be modified with correct timestamp?
			Acknowledgement: ethReceiveAckEvent.Acknowledgement,
			ProofAcked:      storageProofBz,
			ProofHeight: clienttypes.Height{
				RevisionNumber: 0,
				RevisionHeight: s.LastEtheruemLightClientUpdate,
			},
			Signer: cosmosUserAddress,
		})
		s.Require().NoError(err)
		s.Require().Equal(uint32(0), txResp.Code)
	}))

	s.Require().True(s.Run("Approve the ICS20Transfer.sol contract to spend the erc20 tokens", func() {
		tx, err := ibcERC20.Approve(s.GetTransactOpts(s.key), ics20Address, transferAmount)
		s.Require().NoError(err)
		receipt := s.GetTxReciept(ctx, eth, tx.Hash())
		s.Require().Equal(ethtypes.ReceiptStatusSuccessful, receipt.Status)

		allowance, err := ibcERC20.Allowance(nil, ethereumUserAddress, ics20Address)
		s.Require().NoError(err)
		s.Require().Equal(transferAmount, allowance)
	}))

	var returnPacket ics26router.IICS26RouterMsgsPacket
	returnMemo := "testreturnmemo"
	var sendBlockNumber int64
	s.Require().True(s.Run("Transfer tokens back from Ethereum", func() {
		timeout := uint64(time.Now().Add(30 * time.Minute).Unix())
		msgSendTransfer := ics20transfer.IICS20TransferMsgsSendTransferMsg{
			Denom:            ibcERC20Address,
			Amount:           transferAmount,
			Receiver:         cosmosUserAddress,
			SourceChannel:    s.TendermintLightClientID,
			DestPort:         transfertypes.PortID,
			TimeoutTimestamp: timeout,
			Memo:             returnMemo,
		}

		tx, err := s.ics20Contract.SendTransfer(s.GetTransactOpts(s.key), msgSendTransfer)
		s.Require().NoError(err)
		receipt := s.GetTxReciept(ctx, eth, tx.Hash())
		s.Require().Equal(ethtypes.ReceiptStatusSuccessful, receipt.Status)

		sendBlockNumber = receipt.BlockNumber.Int64()

		transferEvent, err := e2esuite.GetEvmEvent(receipt, s.ics20Contract.ParseICS20Transfer)
		s.Require().NoError(err)
		s.Require().Equal(denomOnEthereum.GetFullDenomPath(), transferEvent.PacketData.Denom)
		s.Require().Equal(transferAmount, transferEvent.PacketData.Amount)
		s.Require().Equal(strings.ToLower(ethereumUserAddress.Hex()), strings.ToLower(transferEvent.PacketData.Sender))
		s.Require().Equal(cosmosUserAddress, transferEvent.PacketData.Receiver)
		s.Require().Equal(returnMemo, transferEvent.PacketData.Memo)

		sendPacketEvent, err := e2esuite.GetEvmEvent(receipt, s.ics26Contract.ParseSendPacket)
		s.Require().NoError(err)
		returnPacket = sendPacketEvent.Packet
		s.Require().Equal(uint32(1), returnPacket.Sequence)
		s.Require().Equal(timeout, returnPacket.TimeoutTimestamp)
		s.Require().Equal(transfertypes.PortID, returnPacket.SourcePort)
		s.Require().Equal(s.TendermintLightClientID, returnPacket.SourceChannel)
		s.Require().Equal(transfertypes.PortID, returnPacket.DestPort)
		s.Require().Equal(s.EthereumLightClientID, returnPacket.DestChannel)
		s.Require().Equal(transfertypes.Version, returnPacket.Version)

		s.True(s.Run("Verify balances on Ethereum", func() {
			userBalance, err := ibcERC20.BalanceOf(nil, ethereumUserAddress)
			s.Require().NoError(err)
			s.Require().Equal(int64(0), userBalance.Int64())

			// the whole balance should have been burned
			ics20TransferBalance, err := ibcERC20.BalanceOf(nil, ics20Address)
			s.Require().NoError(err)
			s.Require().Equal(int64(0), ics20TransferBalance.Int64())
		}))
	}))

	var cosmosReceiveAck []byte
	s.Require().True(s.Run("Receive packet on Cosmos chain", func() {
		s.UpdateEthClient(ctx, s.contractAddresses.Ics26Router, sendBlockNumber, simdRelayerUser)

		path := fmt.Sprintf("commitments/ports/%s/channels/%s/sequences/%d", returnPacket.SourcePort, returnPacket.SourceChannel, returnPacket.Sequence)
		storageProofBz := s.getCommitmentProof(path)

		txResp, err := s.BroadcastMessages(ctx, simd, cosmosUserWallet, 200_000, &channeltypes.MsgRecvPacket{
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
			ProofCommitment: storageProofBz,
			ProofHeight: clienttypes.Height{
				RevisionNumber: 0,
				RevisionHeight: s.LastEtheruemLightClientUpdate,
			},
			Signer: cosmosUserAddress,
		})
		s.Require().NoError(err)

		cosmosReceiveAck, err = ibctesting.ParseAckFromEvents(txResp.Events)
		s.Require().NoError(err)
		s.Require().NotNil(cosmosReceiveAck)

		s.Require().True(s.Run("Verify balances on Cosmos chain", func() {
			// Check the balance of UserB
			resp, err := e2esuite.GRPCQuery[banktypes.QueryBalanceResponse](ctx, simd, &banktypes.QueryBalanceRequest{
				Address: cosmosUserAddress,
				Denom:   transferCoin.Denom,
			})
			s.Require().NoError(err)
			s.Require().NotNil(resp.Balance)
			s.Require().Equal(testvalues.InitialBalance, resp.Balance.Amount.Int64())
		}))
	}))

	s.Require().True(s.Run("Acknowledge packet on Ethereum", func() {
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

func (s *IbcEurekaTestSuite) TestICS20TransferTimeoutFromEthereumToCosmosChain() {
	ctx := context.Background()

	s.SetupSuite(ctx)

	eth, simd := s.ChainA, s.ChainB

	transferAmount := big.NewInt(testvalues.TransferAmount)
	ethereumUserAddress := crypto.PubkeyToAddress(s.key.PublicKey)
	cosmosUserWallet := s.UserB
	cosmosUserAddress := cosmosUserWallet.FormattedAddress()

	var packet ics26router.IICS26RouterMsgsPacket
	s.Require().True(s.Run("Approve the ICS20Transfer.sol contract to spend the erc20 tokens", func() {
		ics20Address := ethcommon.HexToAddress(s.contractAddresses.Ics20Transfer)
		tx, err := s.erc20Contract.Approve(s.GetTransactOpts(s.key), ics20Address, transferAmount)
		s.Require().NoError(err)
		receipt := s.GetTxReciept(ctx, eth, tx.Hash())
		s.Require().Equal(ethtypes.ReceiptStatusSuccessful, receipt.Status)

		allowance, err := s.erc20Contract.Allowance(nil, ethereumUserAddress, ics20Address)
		s.Require().NoError(err)
		s.Require().Equal(transferAmount, allowance)
	}))

	var timeout uint64
	s.Require().True(s.Run("Send transfer on Ethereum", func() {
		timeout = uint64(time.Now().Add(30 * time.Second).Unix())
		msgSendTransfer := ics20transfer.IICS20TransferMsgsSendTransferMsg{
			Denom:            s.contractAddresses.Erc20,
			Amount:           transferAmount,
			Receiver:         cosmosUserAddress,
			SourceChannel:    s.TendermintLightClientID,
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
		s.Require().Equal(testvalues.TransferAmount, transferEvent.PacketData.Amount.Int64()) // Because the amount is converted to the sdk amount
		s.Require().Equal(strings.ToLower(ethereumUserAddress.Hex()), strings.ToLower(transferEvent.PacketData.Sender))
		s.Require().Equal(cosmosUserAddress, transferEvent.PacketData.Receiver)
		s.Require().Equal("testmemo", transferEvent.PacketData.Memo)

		sendPacketEvent, err := e2esuite.GetEvmEvent(receipt, s.ics26Contract.ParseSendPacket)
		s.Require().NoError(err)
		packet = sendPacketEvent.Packet
		s.Require().Equal(uint32(1), packet.Sequence)
		s.Require().Equal(timeout, packet.TimeoutTimestamp)
		s.Require().Equal("transfer", packet.SourcePort)
		s.Require().Equal(s.TendermintLightClientID, packet.SourceChannel)
		s.Require().Equal("transfer", packet.DestPort)
		s.Require().Equal(s.EthereumLightClientID, packet.DestChannel)
		s.Require().Equal(transfertypes.Version, packet.Version)

		s.Require().True(s.Run("Verify balances on Ethereum", func() {
			// User balance on Etherem
			userBalance, err := s.erc20Contract.BalanceOf(nil, ethereumUserAddress)
			s.Require().NoError(err)
			s.Require().Equal(testvalues.InitialBalance-testvalues.TransferAmount, userBalance.Int64())

			// ICS20 contract balance on Ethereum
			escrowBalance, err := s.erc20Contract.BalanceOf(nil, s.escrowContractAddr)
			s.Require().NoError(err)
			s.Require().Equal(transferAmount, escrowBalance)
		}))
	}))

	// sleep for 45 seconds to let the packet timeout
	time.Sleep(45 * time.Second)

	s.True(s.Run("Timeout packet on Ethereum", func() {
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

		s.Require().True(s.Run("Verify balances on Ethereum", func() {
			// User balance on Ethereum
			userBalance, err := s.erc20Contract.BalanceOf(nil, ethereumUserAddress)
			s.Require().NoError(err)
			s.Require().Equal(testvalues.InitialBalance, userBalance.Int64())

			// ICS20 contract balance on Ethereum
			escrowBalance, err := s.erc20Contract.BalanceOf(nil, s.escrowContractAddr)
			s.Require().NoError(err)
			s.Require().Equal(int64(0), escrowBalance.Int64())
		}))
	}))
}

func (s *IbcEurekaTestSuite) getCommitmentProof(path string) []byte {
	eth, simd := s.ChainA, s.ChainB

	storageKey := ethereum.GetCommitmentsStorageKey(path)
	storageKeys := []string{storageKey.Hex()}

	blockNumberHex := fmt.Sprintf("0x%x", s.LastEtheruemLightClientUpdate)
	proofResp, err := eth.EthAPI.GetProof(s.contractAddresses.Ics26Router, storageKeys, blockNumberHex)
	s.Require().NoError(err)
	s.Require().Len(proofResp.StorageProof, 1)
	s.Require().NotEmpty(ethcommon.FromHex(proofResp.StorageProof[0].Value))

	var proofBz [][]byte
	for _, proofStr := range proofResp.StorageProof[0].Proof {
		proofBz = append(proofBz, ethcommon.FromHex(proofStr))
	}
	storageProof := ethereumligthclient.StorageProof{
		Key:   ethereum.HexToBeBytes(proofResp.StorageProof[0].Key),
		Value: ethereum.HexToBeBytes(proofResp.StorageProof[0].Value),
		Proof: proofBz,
	}
	return simd.Config().EncodingConfig.Codec.MustMarshal(&storageProof)
}
