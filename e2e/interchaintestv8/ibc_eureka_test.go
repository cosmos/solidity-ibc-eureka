package main

import (
	"context"
	"crypto/ecdsa"
	"encoding/hex"
	"fmt"
	"math/big"
	"os"
	"strconv"
	"strings"
	"testing"
	"time"

	"github.com/cosmos/solidity-ibc-eureka/abigen/ibcerc20"
	"github.com/cosmos/solidity-ibc-eureka/abigen/ics20lib"
	"github.com/cosmos/solidity-ibc-eureka/abigen/ics20transfer"
	"github.com/cosmos/solidity-ibc-eureka/abigen/ics26router"
	"github.com/cosmos/solidity-ibc-eureka/abigen/icscore"
	"github.com/stretchr/testify/suite"

	"github.com/ethereum/go-ethereum/accounts/abi"
	ethcommon "github.com/ethereum/go-ethereum/common"
	ethtypes "github.com/ethereum/go-ethereum/core/types"
	"github.com/ethereum/go-ethereum/crypto"
	"github.com/ethereum/go-ethereum/ethclient"

	sdkmath "cosmossdk.io/math"

	sdk "github.com/cosmos/cosmos-sdk/types"
	banktypes "github.com/cosmos/cosmos-sdk/x/bank/types"

	transfertypes "github.com/cosmos/ibc-go/v9/modules/apps/transfer/types"
	clienttypes "github.com/cosmos/ibc-go/v9/modules/core/02-client/types"
	channeltypesv1 "github.com/cosmos/ibc-go/v9/modules/core/04-channel/types"
	channeltypesv2 "github.com/cosmos/ibc-go/v9/modules/core/04-channel/v2/types"
	commitmenttypesv2 "github.com/cosmos/ibc-go/v9/modules/core/23-commitment/types/v2"
	ibchostv2 "github.com/cosmos/ibc-go/v9/modules/core/24-host/v2"
	ibcexported "github.com/cosmos/ibc-go/v9/modules/core/exported"
	ibctesting "github.com/cosmos/ibc-go/v9/testing"

	"github.com/strangelove-ventures/interchaintest/v8/chain/cosmos"

	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/e2esuite"
	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/ethereum"
	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/operator"
	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/testvalues"
	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/types"
	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/types/erc20"
	ethereumligthclient "github.com/srdtrk/solidity-ibc-eureka/e2e/v8/types/ethereumlightclient"
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
	icsCoreContract    *icscore.Contract
	ics26Contract      *ics26router.Contract
	ics20Contract      *ics20transfer.Contract
	erc20Contract      *erc20.Contract
	escrowContractAddr ethcommon.Address
}

// TestWithIbcEurekaTestSuite is the boilerplate code that allows the test suite to be run
func TestWithIbcEurekaTestSuite(t *testing.T) {
	suite.Run(t, new(IbcEurekaTestSuite))
}

// SetupSuite calls the underlying IbcEurekaTestSuite's SetupSuite method
// and deploys the IbcEureka contract
func (s *IbcEurekaTestSuite) SetupSuite(ctx context.Context, proofType operator.SupportedProofType) {
	s.TestSuite.SetupSuite(ctx)

	eth, simd := s.ChainA, s.ChainB

	var prover string
	s.Require().True(s.Run("Set up environment", func() {
		err := os.Chdir("../..")
		s.Require().NoError(err)

		s.key, err = eth.CreateAndFundUser()
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
		args := append([]string{
			"--trust-level", testvalues.DefaultTrustLevel.String(),
			"--trusting-period", strconv.Itoa(testvalues.DefaultTrustPeriod),
			"-o", testvalues.Sp1GenesisFilePath,
		}, proofType.ToOperatorArgs()...)
		s.Require().NoError(operator.RunGenesis(args...))

		var (
			stdout []byte
			err    error
		)
		switch prover {
		case testvalues.EnvValueSp1Prover_Mock:
			s.FailNow("Mock prover not supported")
		case testvalues.EnvValueSp1Prover_Network:
			// make sure that the SP1_PRIVATE_KEY is set.
			s.Require().NotEmpty(os.Getenv(testvalues.EnvKeySp1PrivateKey))

			stdout, err = eth.ForgeScript(s.deployer, testvalues.E2EDeployScriptPath)
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
		s.icsCoreContract, err = icscore.NewContract(ethcommon.HexToAddress(s.contractAddresses.IcsCore), ethClient)
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
		s.CreateEthereumLightClient(ctx, simdRelayerUser, s.contractAddresses.IbcStore)
	}))

	s.Require().True(s.Run("Add client and counterparty on EVM", func() {
		channel := icscore.IICS04ChannelMsgsChannel{
			CounterpartyId: ibctesting.FirstChannelID,
			MerklePrefix:   [][]byte{[]byte(ibcexported.StoreKey), []byte("")},
		}
		lightClientAddress := ethcommon.HexToAddress(s.contractAddresses.Ics07Tendermint)
		tx, err := s.icsCoreContract.AddChannel(s.GetTransactOpts(s.key), ibcexported.Tendermint, channel, lightClientAddress)
		s.Require().NoError(err)

		receipt := s.GetTxReciept(ctx, eth, tx.Hash())
		event, err := e2esuite.GetEvmEvent(receipt, s.icsCoreContract.ParseICS04ChannelAdded)
		s.Require().NoError(err)
		s.Require().Equal(ibctesting.FirstClientID, event.ChannelId)
		s.Require().Equal(ibctesting.FirstChannelID, event.Channel.CounterpartyId)
		s.TendermintLightClientID = event.ChannelId
	}))

	s.Require().True(s.Run("Create channel and register counterparty on Cosmos chain", func() {
		merklePathPrefix := commitmenttypesv2.NewMerklePath([]byte(""))

		_, err := s.BroadcastMessages(ctx, simd, simdRelayerUser, 200_000, &channeltypesv2.MsgCreateChannel{
			ClientId:         s.EthereumLightClientID,
			MerklePathPrefix: merklePathPrefix,
			Signer:           simdRelayerUser.FormattedAddress(),
		})
		s.Require().NoError(err)

		_, err = s.BroadcastMessages(ctx, simd, simdRelayerUser, 200_000, &channeltypesv2.MsgRegisterCounterparty{
			ChannelId:             ibctesting.FirstChannelID,
			CounterpartyChannelId: s.TendermintLightClientID,
			Signer:                simdRelayerUser.FormattedAddress(),
		})
		s.Require().NoError(err)
	}))
}

func (s *IbcEurekaTestSuite) TestDeploy_Groth16() {
	ctx := context.Background()
	s.DeployTest(ctx, operator.ProofTypeGroth16)
}

func (s *IbcEurekaTestSuite) TestDeploy_Plonk() {
	ctx := context.Background()
	s.DeployTest(ctx, operator.ProofTypePlonk)
}

// DeployTest tests the deployment of the IbcEureka contracts
func (s *IbcEurekaTestSuite) DeployTest(ctx context.Context, proofType operator.SupportedProofType) {
	s.SetupSuite(ctx, proofType)

	simd := s.ChainB

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
		owner, err := s.icsCoreContract.Owner(nil)
		s.Require().NoError(err)
		s.Require().Equal(strings.ToLower(crypto.PubkeyToAddress(s.deployer.PublicKey).Hex()), strings.ToLower(owner.Hex()))

		clientAddress, err := s.icsCoreContract.GetClient(nil, s.TendermintLightClientID)
		s.Require().NoError(err)
		s.Require().Equal(s.contractAddresses.Ics07Tendermint, strings.ToLower(clientAddress.Hex()))

		counterpartyInfo, err := s.icsCoreContract.GetChannel(nil, s.TendermintLightClientID)
		s.Require().NoError(err)
		s.Require().Equal(ibctesting.FirstChannelID, counterpartyInfo.CounterpartyId)
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

	s.Require().True(s.Run("Verify etheruem light client", func() {
		_, err := e2esuite.GRPCQuery[clienttypes.QueryClientStateResponse](ctx, simd, &clienttypes.QueryClientStateRequest{
			ClientId: s.EthereumLightClientID,
		})
		s.Require().NoError(err)

		channelResp, err := e2esuite.GRPCQuery[channeltypesv2.QueryChannelResponse](ctx, simd, &channeltypesv2.QueryChannelRequest{
			ChannelId: ibctesting.FirstChannelID,
		})
		s.Require().NoError(err)
		s.Require().Equal(s.EthereumLightClientID, channelResp.Channel.ClientId)
	}))
}

func (s *IbcEurekaTestSuite) TestICS20TransferERC20TokenfromEthereumToCosmosAndBack_Groth16() {
	ctx := context.Background()
	s.ICS20TransferERC20TokenfromEthereumToCosmosAndBackTest(ctx, operator.ProofTypeGroth16, 1)
}

func (s *IbcEurekaTestSuite) TestICS20TransferERC20TokenfromEthereumToCosmosAndBack_Plonk() {
	ctx := context.Background()
	s.ICS20TransferERC20TokenfromEthereumToCosmosAndBackTest(ctx, operator.ProofTypePlonk, 1)
}

func (s *IbcEurekaTestSuite) Test_25_ICS20TransferERC20TokenfromEthereumToCosmosAndBack_Groth16() {
	ctx := context.Background()
	s.ICS20TransferERC20TokenfromEthereumToCosmosAndBackTest(ctx, operator.ProofTypeGroth16, 25)
}

func (s *IbcEurekaTestSuite) Test_50_ICS20TransferERC20TokenfromEthereumToCosmosAndBack_Groth16() {
	ctx := context.Background()
	s.ICS20TransferERC20TokenfromEthereumToCosmosAndBackTest(ctx, operator.ProofTypeGroth16, 50)
}

func (s *IbcEurekaTestSuite) Test_50_ICS20TransferERC20TokenfromEthereumToCosmosAndBack_Plonk() {
	ctx := context.Background()
	s.ICS20TransferERC20TokenfromEthereumToCosmosAndBackTest(ctx, operator.ProofTypePlonk, 50)
}

// ICS20TransferERC20TokenfromEthereumToCosmosAndBackTest tests the ICS20 transfer functionality by transferring
// ERC20 tokens with n packets from Ethereum to Cosmos chain and then back from Cosmos chain to Ethereum
func (s *IbcEurekaTestSuite) ICS20TransferERC20TokenfromEthereumToCosmosAndBackTest(
	ctx context.Context, proofType operator.SupportedProofType, numOfTransfers int,
) {
	s.SetupSuite(ctx, proofType)

	eth, simd := s.ChainA, s.ChainB

	ics20Address := ethcommon.HexToAddress(s.contractAddresses.Ics20Transfer)
	transferAmount := big.NewInt(testvalues.TransferAmount)
	totalTransferAmount := big.NewInt(testvalues.TransferAmount * int64(numOfTransfers)) // total amount transferred
	if totalTransferAmount.Int64() > testvalues.InitialBalance {
		s.FailNow("Total transfer amount exceeds the initial balance")
	}
	ethereumUserAddress := crypto.PubkeyToAddress(s.key.PublicKey)
	cosmosUserWallet := s.UserB
	cosmosUserAddress := cosmosUserWallet.FormattedAddress()
	_, simdRelayerUser := s.GetRelayerUsers(ctx)

	ics26routerAbi, err := abi.JSON(strings.NewReader(ics26router.ContractABI))
	s.Require().NoError(err)

	s.Require().True(s.Run("Approve the ICS20Transfer.sol contract to spend the erc20 tokens", func() {
		tx, err := s.erc20Contract.Approve(s.GetTransactOpts(s.key), ics20Address, totalTransferAmount)
		s.Require().NoError(err)
		receipt := s.GetTxReciept(ctx, eth, tx.Hash())
		s.Require().Equal(ethtypes.ReceiptStatusSuccessful, receipt.Status)

		allowance, err := s.erc20Contract.Allowance(nil, ethereumUserAddress, ics20Address)
		s.Require().NoError(err)
		s.Require().Equal(totalTransferAmount, allowance)
	}))

	var sendPacket ics26router.IICS26RouterMsgsPacket
	var sendBlockNumber int64
	s.Require().True(s.Run(fmt.Sprintf("Send %d transfers on Ethereum", numOfTransfers), func() {
		timeout := uint64(time.Now().Add(30 * time.Minute).Unix())
		transferMulticall := make([][]byte, numOfTransfers)

		msgSendPacket := s.createICS20MsgSendPacket(
			ethereumUserAddress,
			s.contractAddresses.Erc20,
			transferAmount,
			cosmosUserAddress,
			s.TendermintLightClientID,
			timeout,
			"",
		)

		encodedMsg, err := ics26routerAbi.Pack("sendPacket", msgSendPacket)
		s.Require().NoError(err)
		for i := 0; i < numOfTransfers; i++ {
			transferMulticall[i] = encodedMsg
		}

		tx, err := s.ics26Contract.Multicall(s.GetTransactOpts(s.key), transferMulticall)
		s.Require().NoError(err)
		receipt := s.GetTxReciept(ctx, eth, tx.Hash())
		s.Require().Equal(ethtypes.ReceiptStatusSuccessful, receipt.Status)
		s.T().Logf("Multicall send %d transfers gas used: %d", numOfTransfers, receipt.GasUsed)
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
		s.Require().Len(sendPacket.Payloads, 1)
		s.Require().Equal(transfertypes.PortID, sendPacket.Payloads[0].SourcePort)
		s.Require().Equal(s.TendermintLightClientID, sendPacket.SourceChannel)
		s.Require().Equal(transfertypes.PortID, sendPacket.Payloads[0].DestPort)
		s.Require().Equal(ibctesting.FirstChannelID, sendPacket.DestChannel)
		s.Require().Equal(transfertypes.V1, sendPacket.Payloads[0].Version)
		s.Require().Equal(transfertypes.EncodingABI, sendPacket.Payloads[0].Encoding)

		s.True(s.Run("Verify balances on Ethereum", func() {
			// User balance on Ethereum
			userBalance, err := s.erc20Contract.BalanceOf(nil, ethereumUserAddress)
			s.Require().NoError(err)
			s.Require().Equal(testvalues.InitialBalance-totalTransferAmount.Int64(), userBalance.Int64())

			// ICS20 contract balance on Ethereum
			escrowBalance, err := s.erc20Contract.BalanceOf(nil, s.escrowContractAddr)
			s.Require().NoError(err)
			s.Require().Equal(totalTransferAmount, escrowBalance)
		}))
	}))

	var recvAck []byte
	var denomOnCosmos transfertypes.Denom
	s.Require().True(s.Run("Receive packets on Cosmos chain", func() {
		s.UpdateEthClient(ctx, s.contractAddresses.IbcStore, sendBlockNumber, simdRelayerUser)

		recvPacketMsgs := make([]sdk.Msg, numOfTransfers)
		for i := 0; i < numOfTransfers; i++ {
			path := ibchostv2.PacketCommitmentKey(sendPacket.SourceChannel, uint64(i+1))
			storageProofBz := s.getCommitmentProof(path)

			packet := channeltypesv2.Packet{
				Sequence:           uint64(i + 1),
				SourceChannel:      sendPacket.SourceChannel,
				DestinationChannel: sendPacket.DestChannel,
				TimeoutTimestamp:   sendPacket.TimeoutTimestamp,
				Payloads: []channeltypesv2.Payload{
					{
						SourcePort:      sendPacket.Payloads[0].SourcePort,
						DestinationPort: sendPacket.Payloads[0].DestPort,
						Version:         sendPacket.Payloads[0].Version,
						Encoding:        sendPacket.Payloads[0].Encoding,
						Value:           sendPacket.Payloads[0].Value,
					},
				},
			}
			recvPacketMsgs[i] = &channeltypesv2.MsgRecvPacket{
				Packet:          packet,
				ProofCommitment: storageProofBz,
				ProofHeight: clienttypes.Height{
					RevisionNumber: 0,
					RevisionHeight: s.LastEtheruemLightClientUpdate,
				},
				Signer: cosmosUserAddress,
			}
		}

		_, err := s.BroadcastMessages(ctx, simd, cosmosUserWallet, 20_000_000, recvPacketMsgs...)
		s.Require().NoError(err)

		// TODO: Replace with a proper parse from events as soon as it is available in ibc-go
		// recvAck, err = ibctesting.ParseAckFromEvents(txResp.Events)
		// s.Require().NoError(err)
		// s.Require().NotNil(recvAck)
		ack := channeltypesv1.NewResultAcknowledgement([]byte{byte(1)})
		recvAck = ack.Acknowledgement()
		s.Require().True(s.Run("Verify balances on Cosmos chain", func() {
			denomOnCosmos = transfertypes.NewDenom(s.contractAddresses.Erc20, transfertypes.NewHop(transfertypes.PortID, ibctesting.FirstChannelID))

			// User balance on Cosmos chain
			resp, err := e2esuite.GRPCQuery[banktypes.QueryBalanceResponse](ctx, simd, &banktypes.QueryBalanceRequest{
				Address: cosmosUserAddress,
				Denom:   denomOnCosmos.IBCDenom(),
			})
			s.Require().NoError(err)
			s.Require().NotNil(resp.Balance)
			s.Require().Equal(totalTransferAmount.Uint64(), resp.Balance.Amount.Uint64())
			s.Require().Equal(denomOnCosmos.IBCDenom(), resp.Balance.Denom)
		}))
	}))

	s.Require().True(s.Run("Acknowledge packets on Ethereum", func() {
		// This will be a membership proof since the acknowledgement is written
		proofPaths := make([][]byte, numOfTransfers)
		for i := 0; i < numOfTransfers; i++ {
			proofPaths[i] = ibchostv2.PacketAcknowledgementKey(sendPacket.DestChannel, uint64(i+1))
		}
		proofHeight, ucAndMemProof := s.updateClientAndMembershipProof(ctx, simd, proofType, proofPaths)

		ackMulticall := make([][]byte, numOfTransfers)
		for i := 0; i < numOfTransfers; i++ {
			msg := ics26router.IICS26RouterMsgsMsgAckPacket{
				Packet:          sendPacket,
				Acknowledgement: recvAck,
				ProofAcked:      []byte(""),
				ProofHeight:     *proofHeight,
			}
			msg.Packet.Sequence = uint32(i + 1)
			if i == 0 {
				msg.ProofAcked = ucAndMemProof
			}

			ackMulticall[i], err = ics26routerAbi.Pack("ackPacket", msg)
			s.Require().NoError(err)
		}

		tx, err := s.ics26Contract.Multicall(s.GetTransactOpts(s.key), ackMulticall)
		s.Require().NoError(err)

		receipt := s.GetTxReciept(ctx, eth, tx.Hash())
		s.Require().Equal(ethtypes.ReceiptStatusSuccessful, receipt.Status)
		s.T().Logf("Multicall ack %d packets gas used: %d", numOfTransfers, receipt.GasUsed)

		if s.generateFixtures {
			s.Require().NoError(types.GenerateAndSaveFixture(
				fmt.Sprintf("acknowledgeMultiPacket_%d-%s.json", numOfTransfers, proofType.String()),
				s.contractAddresses.Erc20, "multicall", ackMulticall, sendPacket,
			))
		}

		s.Require().True(s.Run("Verify balances on Ethereum", func() {
			// User balance on Ethereum
			userBalance, err := s.erc20Contract.BalanceOf(nil, ethereumUserAddress)
			s.Require().NoError(err)
			s.Require().Equal(testvalues.InitialBalance-totalTransferAmount.Int64(), userBalance.Int64())

			// ICS20 contract balance on Ethereum
			escrowBalance, err := s.erc20Contract.BalanceOf(nil, s.escrowContractAddr)
			s.Require().NoError(err)
			s.Require().Equal(totalTransferAmount.Int64(), escrowBalance.Int64())
		}))
	}))

	var returnPacket channeltypesv2.Packet
	s.Require().True(s.Run("Transfer tokens back from Cosmos chain", func() {
		// We need the timeout to be a whole number of seconds to be received by eth
		timeout := uint64(time.Now().Add(30 * time.Minute).Unix())
		ibcCoin := sdk.NewCoin(denomOnCosmos.Path(), sdkmath.NewIntFromBigInt(transferAmount))

		transferPayload := ics20lib.ICS20LibFungibleTokenPacketData{
			Denom:    ibcCoin.Denom,
			Amount:   ibcCoin.Amount.BigInt(),
			Sender:   cosmosUserWallet.FormattedAddress(),
			Receiver: strings.ToLower(ethereumUserAddress.Hex()),
			Memo:     "",
		}
		transferBz, err := ics20lib.EncodeFungibleTokenPacketData(transferPayload)
		s.Require().NoError(err)
		payload := channeltypesv2.Payload{
			SourcePort:      transfertypes.PortID,
			DestinationPort: transfertypes.PortID,
			Version:         transfertypes.V1,
			Encoding:        transfertypes.EncodingABI,
			Value:           transferBz,
		}

		transferMsgs := make([]sdk.Msg, numOfTransfers)
		for i := 0; i < numOfTransfers; i++ {
			transferMsgs[i] = &channeltypesv2.MsgSendPacket{
				SourceChannel:    ibctesting.FirstChannelID,
				TimeoutTimestamp: timeout,
				Payloads: []channeltypesv2.Payload{
					payload,
				},
				Signer: cosmosUserWallet.FormattedAddress(),
			}
		}

		_, err = s.BroadcastMessages(ctx, simd, cosmosUserWallet, 20_000_000, transferMsgs...)
		s.Require().NoError(err)

		// TODO: Replace with a proper parse from events as soon as it is available in ibc-go
		sequence := uint64(1)
		// TODO: Until we get this packet from the events, we will construct it manually
		// The denom should be the full denom path, not just the ibc denom
		transferPayload.Denom = denomOnCosmos.Path()
		payload.Value, err = ics20lib.EncodeFungibleTokenPacketData(transferPayload)
		s.Require().NoError(err)
		returnPacket = channeltypesv2.Packet{
			Sequence:           sequence,
			SourceChannel:      ibctesting.FirstChannelID,
			DestinationChannel: s.TendermintLightClientID,
			TimeoutTimestamp:   timeout,
			Payloads: []channeltypesv2.Payload{
				payload,
			},
		}

		s.Require().Equal(uint64(1), returnPacket.Sequence)
		s.Require().Equal(transfertypes.PortID, returnPacket.Payloads[0].SourcePort)
		s.Require().Equal(ibctesting.FirstChannelID, returnPacket.SourceChannel)
		s.Require().Equal(transfertypes.PortID, returnPacket.Payloads[0].DestinationPort)
		s.Require().Equal(s.TendermintLightClientID, returnPacket.DestinationChannel)
		s.Require().Equal(timeout, returnPacket.TimeoutTimestamp)

		transferPacketData, err := transfertypes.DecodeABIFungibleTokenPacketData(returnPacket.Payloads[0].Value)
		s.Require().NoError(err)
		s.Require().Equal(denomOnCosmos.Path(), transferPacketData.Denom)
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
			s.Require().Equal(denomOnCosmos.IBCDenom(), resp.Balance.Denom)
		}))
	}))

	var recvBlockNumber int64
	var returnWriteAckEvent *ics26router.ContractWriteAcknowledgement
	s.Require().True(s.Run(fmt.Sprintf("Receive %d packets on Ethereum", numOfTransfers), func() {
		proofPaths := make([][]byte, numOfTransfers)
		for i := 0; i < numOfTransfers; i++ {
			proofPaths[i] = ibchostv2.PacketCommitmentKey(returnPacket.SourceChannel, uint64(i+1))
		}
		proofHeight, ucAndMemProof := s.updateClientAndMembershipProof(ctx, simd, proofType, proofPaths)

		packet := ics26router.IICS26RouterMsgsPacket{
			Sequence:         uint32(returnPacket.Sequence),
			SourceChannel:    returnPacket.SourceChannel,
			DestChannel:      returnPacket.DestinationChannel,
			TimeoutTimestamp: returnPacket.TimeoutTimestamp,
			Payloads: []ics26router.IICS26RouterMsgsPayload{
				{
					SourcePort: returnPacket.Payloads[0].SourcePort,
					DestPort:   returnPacket.Payloads[0].DestinationPort,
					Version:    returnPacket.Payloads[0].Version,
					Encoding:   returnPacket.Payloads[0].Encoding,
					Value:      returnPacket.Payloads[0].Value,
				},
			},
		}
		multicallRecvMsg := make([][]byte, numOfTransfers)
		for i := 0; i < numOfTransfers; i++ {
			msg := ics26router.IICS26RouterMsgsMsgRecvPacket{
				Packet:          packet,
				ProofCommitment: []byte(""),
				ProofHeight:     *proofHeight,
			}
			msg.Packet.Sequence = uint32(i + 1)
			if i == 0 {
				msg.ProofCommitment = ucAndMemProof
			}

			encodedMsg, err := ics26routerAbi.Pack("recvPacket", msg)
			s.Require().NoError(err)
			multicallRecvMsg[i] = encodedMsg
		}

		tx, err := s.ics26Contract.Multicall(s.GetTransactOpts(s.key), multicallRecvMsg)
		s.Require().NoError(err)

		receipt := s.GetTxReciept(ctx, eth, tx.Hash())
		s.Require().Equal(ethtypes.ReceiptStatusSuccessful, receipt.Status)
		s.T().Logf("Multicall receive %d packets gas used: %d", numOfTransfers, receipt.GasUsed)

		recvBlockNumber = receipt.BlockNumber.Int64()

		if s.generateFixtures {
			s.Require().NoError(types.GenerateAndSaveFixture(
				fmt.Sprintf("receiveMultiPacket_%d-%s.json", numOfTransfers, proofType.String()),
				s.contractAddresses.Erc20, "multicall", multicallRecvMsg, packet,
			))
		}

		returnWriteAckEvent, err = e2esuite.GetEvmEvent(receipt, s.ics26Contract.ParseWriteAcknowledgement)
		s.Require().NoError(err)

		receiveEvent, err := e2esuite.GetEvmEvent(receipt, s.ics20Contract.ParseICS20ReceiveTransfer)
		s.Require().NoError(err)
		ethReceiveData := receiveEvent.PacketData
		s.Require().Equal(denomOnCosmos.Path(), ethReceiveData.Denom)
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

	s.Require().True(s.Run("Acknowledge packets on Cosmos chain", func() {
		s.UpdateEthClient(ctx, s.contractAddresses.IbcStore, recvBlockNumber, simdRelayerUser)

		ackMsgs := make([]sdk.Msg, numOfTransfers)
		for i := 0; i < numOfTransfers; i++ {
			path := ibchostv2.PacketAcknowledgementKey(returnPacket.DestinationChannel, uint64(i+1))
			storageProofBz := s.getCommitmentProof(path)

			ackMsgs[i] = &channeltypesv2.MsgAcknowledgement{
				Packet: returnPacket,
				Acknowledgement: channeltypesv2.Acknowledgement{
					AppAcknowledgements: returnWriteAckEvent.Acknowledgements,
				},
				ProofAcked: storageProofBz,
				ProofHeight: clienttypes.Height{
					RevisionNumber: 0,
					RevisionHeight: s.LastEtheruemLightClientUpdate,
				},
				Signer: simdRelayerUser.FormattedAddress(),
			}
		}
		txResp, err := s.BroadcastMessages(ctx, simd, simdRelayerUser, 20_000_000, ackMsgs...)
		s.Require().NoError(err)
		s.Require().Equal(uint32(0), txResp.Code)
	}))
}

func (s *IbcEurekaTestSuite) TestICS20TransferNativeCosmosCoinsToEthereumAndBack_Groth16() {
	ctx := context.Background()
	s.ICS20TransferNativeCosmosCoinsToEthereumAndBackTest(ctx, operator.ProofTypeGroth16)
}

func (s *IbcEurekaTestSuite) TestICS20TransferNativeCosmosCoinsToEthereumAndBack_Plonk() {
	ctx := context.Background()
	s.ICS20TransferNativeCosmosCoinsToEthereumAndBackTest(ctx, operator.ProofTypePlonk)
}

// ICS20TransferNativeCosmosCoinsToEthereumAndBackTest tests the ICS20 transfer functionality
// by transferring native coins from a Cosmos chain to Ethereum and back
func (s *IbcEurekaTestSuite) ICS20TransferNativeCosmosCoinsToEthereumAndBackTest(ctx context.Context, pt operator.SupportedProofType) {
	s.SetupSuite(ctx, pt)

	eth, simd := s.ChainA, s.ChainB

	ics20Address := ethcommon.HexToAddress(s.contractAddresses.Ics20Transfer)
	transferAmount := big.NewInt(testvalues.TransferAmount)
	ethereumUserAddress := crypto.PubkeyToAddress(s.key.PublicKey)
	cosmosUserWallet := s.UserB
	cosmosUserAddress := cosmosUserWallet.FormattedAddress()
	_, simdRelayerUser := s.GetRelayerUsers(ctx)
	sendMemo := "nonnativesend"

	var sendPacket channeltypesv2.Packet
	var transferCoin sdk.Coin
	s.Require().True(s.Run("Send transfer on Cosmos chain", func() {
		// We need the timeout to be a whole number of seconds to be received by eth
		timeout := uint64(time.Now().Add(30 * time.Minute).Unix())
		transferCoin = sdk.NewCoin(s.ChainB.Config().Denom, sdkmath.NewIntFromBigInt(transferAmount))

		transferPayload := ics20lib.ICS20LibFungibleTokenPacketData{
			Denom:    transferCoin.Denom,
			Amount:   transferCoin.Amount.BigInt(),
			Sender:   cosmosUserAddress,
			Receiver: strings.ToLower(ethereumUserAddress.Hex()),
			Memo:     sendMemo,
		}
		transferBz, err := ics20lib.EncodeFungibleTokenPacketData(transferPayload)
		s.Require().NoError(err)

		payload := channeltypesv2.Payload{
			SourcePort:      transfertypes.PortID,
			DestinationPort: transfertypes.PortID,
			Version:         transfertypes.V1,
			Encoding:        transfertypes.EncodingABI,
			Value:           transferBz,
		}
		msgSendPacket := channeltypesv2.MsgSendPacket{
			SourceChannel:    ibctesting.FirstChannelID,
			TimeoutTimestamp: timeout,
			Payloads: []channeltypesv2.Payload{
				payload,
			},
			Signer: cosmosUserWallet.FormattedAddress(),
		}

		_, err = s.BroadcastMessages(ctx, simd, cosmosUserWallet, 200_000, &msgSendPacket)
		s.Require().NoError(err)

		// TODO: Replace with a proper parse from events as soon as it is available in ibc-go
		sequence := uint64(1)
		// TODO: Until we get this packet from the events, we will construct it manually
		// The denom should be the full denom path, not just the ibc denom
		transferPayload.Denom = transferCoin.Denom
		payload.Value, err = ics20lib.EncodeFungibleTokenPacketData(transferPayload)
		s.Require().NoError(err)
		sendPacket = channeltypesv2.Packet{
			Sequence:           sequence,
			SourceChannel:      msgSendPacket.SourceChannel,
			DestinationChannel: s.TendermintLightClientID,
			TimeoutTimestamp:   timeout,
			Payloads: []channeltypesv2.Payload{
				payload,
			},
		}

		s.Require().Equal(uint64(1), sendPacket.Sequence)
		s.Require().Equal(transfertypes.PortID, sendPacket.Payloads[0].SourcePort)
		s.Require().Equal(ibctesting.FirstChannelID, sendPacket.SourceChannel)
		s.Require().Equal(transfertypes.PortID, sendPacket.Payloads[0].DestinationPort)
		s.Require().Equal(s.TendermintLightClientID, sendPacket.DestinationChannel)
		s.Require().Equal(timeout, sendPacket.TimeoutTimestamp)

		transferPacketData, err := transfertypes.DecodeABIFungibleTokenPacketData(sendPacket.Payloads[0].Value)
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
	var ethReceiveTransferPacket ics20transfer.ICS20LibFungibleTokenPacketData
	var denomOnEthereum transfertypes.Denom
	var ibcERC20 *ibcerc20.Contract
	var ibcERC20Address string
	var recvBlockNumber int64
	s.Require().True(s.Run("Receive packet on Ethereum", func() {
		packetCommitmentPath := ibchostv2.PacketCommitmentKey(sendPacket.SourceChannel, sendPacket.Sequence)
		proofHeight, ucAndMemProof := s.updateClientAndMembershipProof(ctx, simd, pt, [][]byte{packetCommitmentPath})

		packet := ics26router.IICS26RouterMsgsPacket{
			Sequence:         uint32(sendPacket.Sequence),
			SourceChannel:    sendPacket.SourceChannel,
			DestChannel:      sendPacket.DestinationChannel,
			TimeoutTimestamp: sendPacket.TimeoutTimestamp,
			Payloads: []ics26router.IICS26RouterMsgsPayload{
				{
					SourcePort: sendPacket.Payloads[0].SourcePort,
					DestPort:   sendPacket.Payloads[0].DestinationPort,
					Version:    transfertypes.V1,
					Encoding:   transfertypes.EncodingABI,
					Value:      sendPacket.Payloads[0].Value,
				},
			},
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
			s.Require().NoError(types.GenerateAndSaveFixture(fmt.Sprintf("receiveNativePacket-%s.json", pt.String()), s.contractAddresses.Erc20, "recvPacket", msg, packet))
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

		denomOnEthereum = transfertypes.NewDenom(transferCoin.Denom, transfertypes.NewHop(sendPacket.Payloads[0].DestinationPort, sendPacket.DestinationChannel))
		actualDenom, err := ibcERC20.Name(nil)
		s.Require().NoError(err)
		s.Require().Equal(denomOnEthereum.IBCDenom(), actualDenom)

		actualBaseDenom, err := ibcERC20.Symbol(nil)
		s.Require().NoError(err)
		s.Require().Equal(transferCoin.Denom, actualBaseDenom)

		actualFullDenom, err := ibcERC20.FullDenomPath(nil)
		s.Require().NoError(err)
		s.Require().Equal(denomOnEthereum.Path(), actualFullDenom)

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
		s.UpdateEthClient(ctx, s.contractAddresses.IbcStore, recvBlockNumber, simdRelayerUser)

		path := ibchostv2.PacketAcknowledgementKey(sendPacket.DestinationChannel, sendPacket.Sequence)
		storageProofBz := s.getCommitmentProof(path)

		_, err := s.BroadcastMessages(ctx, simd, simdRelayerUser, 200_000, &channeltypesv2.MsgAcknowledgement{
			Packet: sendPacket,
			Acknowledgement: channeltypesv2.Acknowledgement{
				AppAcknowledgements: [][]byte{ethReceiveAckEvent.Acknowledgements[0]},
			},
			ProofAcked: storageProofBz,
			ProofHeight: clienttypes.Height{
				RevisionNumber: 0,
				RevisionHeight: s.LastEtheruemLightClientUpdate,
			},
			Signer: simdRelayerUser.FormattedAddress(),
		})
		s.Require().NoError(err)
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
		msgSendPacket := s.createICS20MsgSendPacket(
			ethereumUserAddress,
			ibcERC20Address,
			transferAmount,
			cosmosUserAddress,
			s.TendermintLightClientID,
			timeout,
			returnMemo,
		)

		tx, err := s.ics26Contract.SendPacket(s.GetTransactOpts(s.key), msgSendPacket)
		s.Require().NoError(err)
		receipt := s.GetTxReciept(ctx, eth, tx.Hash())
		s.Require().Equal(ethtypes.ReceiptStatusSuccessful, receipt.Status)

		sendBlockNumber = receipt.BlockNumber.Int64()

		transferEvent, err := e2esuite.GetEvmEvent(receipt, s.ics20Contract.ParseICS20Transfer)
		s.Require().NoError(err)
		s.Require().Equal(denomOnEthereum.Path(), transferEvent.PacketData.Denom)
		s.Require().Equal(transferAmount, transferEvent.PacketData.Amount)
		s.Require().Equal(strings.ToLower(ethereumUserAddress.Hex()), strings.ToLower(transferEvent.PacketData.Sender))
		s.Require().Equal(cosmosUserAddress, transferEvent.PacketData.Receiver)
		s.Require().Equal(returnMemo, transferEvent.PacketData.Memo)

		sendPacketEvent, err := e2esuite.GetEvmEvent(receipt, s.ics26Contract.ParseSendPacket)
		s.Require().NoError(err)
		returnPacket = sendPacketEvent.Packet
		s.Require().Equal(uint32(1), returnPacket.Sequence)
		s.Require().Equal(timeout, returnPacket.TimeoutTimestamp)
		s.Require().Equal(transfertypes.PortID, returnPacket.Payloads[0].SourcePort)
		s.Require().Equal(s.TendermintLightClientID, returnPacket.SourceChannel)
		s.Require().Equal(transfertypes.PortID, returnPacket.Payloads[0].DestPort)
		s.Require().Equal(ibctesting.FirstChannelID, returnPacket.DestChannel)
		s.Require().Equal(transfertypes.V1, returnPacket.Payloads[0].Version)
		s.Require().Equal(transfertypes.EncodingABI, returnPacket.Payloads[0].Encoding)

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
		s.UpdateEthClient(ctx, s.contractAddresses.IbcStore, sendBlockNumber, simdRelayerUser)

		path := ibchostv2.PacketCommitmentKey(returnPacket.SourceChannel, uint64(returnPacket.Sequence))
		storageProofBz := s.getCommitmentProof(path)

		_, err := s.BroadcastMessages(ctx, simd, simdRelayerUser, 200_000, &channeltypesv2.MsgRecvPacket{
			Packet: channeltypesv2.Packet{
				Sequence:           uint64(returnPacket.Sequence),
				SourceChannel:      returnPacket.SourceChannel,
				DestinationChannel: returnPacket.DestChannel,
				TimeoutTimestamp:   returnPacket.TimeoutTimestamp,
				Payloads: []channeltypesv2.Payload{
					{
						SourcePort:      returnPacket.Payloads[0].SourcePort,
						DestinationPort: returnPacket.Payloads[0].DestPort,
						Version:         returnPacket.Payloads[0].Version,
						Encoding:        returnPacket.Payloads[0].Encoding,
						Value:           returnPacket.Payloads[0].Value,
					},
				},
			},
			ProofCommitment: storageProofBz,
			ProofHeight: clienttypes.Height{
				RevisionNumber: 0,
				RevisionHeight: s.LastEtheruemLightClientUpdate,
			},
			Signer: simdRelayerUser.FormattedAddress(),
		})
		s.Require().NoError(err)

		// TODO: Replace with a proper parse from events as soon as it is available in ibc-go
		// cosmosReceiveAck, err = ibctesting.ParseAckFromEvents(txResp.Events)
		// s.Require().NoError(err)
		// s.Require().NotNil(cosmosReceiveAck)
		ack := channeltypesv1.NewResultAcknowledgement([]byte{byte(1)})
		cosmosReceiveAck = ack.Acknowledgement()

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
		// This will be a membership proof since the acknowledgement is written
		packetAckPath := ibchostv2.PacketAcknowledgementKey(returnPacket.DestChannel, uint64(returnPacket.Sequence))
		proofHeight, ucAndMemProof := s.updateClientAndMembershipProof(ctx, simd, pt, [][]byte{packetAckPath})

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

func (s *IbcEurekaTestSuite) TestICS20TransferTimeoutFromEthereumToCosmosChain_Groth16() {
	ctx := context.Background()
	s.ICS20TransferTimeoutFromEthereumToCosmosChainTest(ctx, operator.ProofTypeGroth16)
}

func (s *IbcEurekaTestSuite) TestICS20TransferTimeoutFromEthereumToCosmosChain_Plonk() {
	ctx := context.Background()
	s.ICS20TransferTimeoutFromEthereumToCosmosChainTest(ctx, operator.ProofTypePlonk)
}

func (s *IbcEurekaTestSuite) ICS20TransferTimeoutFromEthereumToCosmosChainTest(ctx context.Context, pt operator.SupportedProofType) {
	s.SetupSuite(ctx, pt)

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

		msgSendPacket := s.createICS20MsgSendPacket(
			ethereumUserAddress,
			s.contractAddresses.Erc20,
			transferAmount,
			cosmosUserAddress,
			s.TendermintLightClientID,
			timeout,
			"testmemo",
		)

		tx, err := s.ics26Contract.SendPacket(s.GetTransactOpts(s.key), msgSendPacket)
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
		s.Require().Equal(transfertypes.PortID, packet.Payloads[0].SourcePort)
		s.Require().Equal(s.TendermintLightClientID, packet.SourceChannel)
		s.Require().Equal(transfertypes.PortID, packet.Payloads[0].DestPort)
		s.Require().Equal(ibctesting.FirstChannelID, packet.DestChannel)
		s.Require().Equal(transfertypes.V1, packet.Payloads[0].Version)

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
		// This will be a non-membership proof since no packets have been sent
		packetReceiptPath := ibchostv2.PacketReceiptKey(packet.DestChannel, uint64(packet.Sequence))
		proofHeight, ucAndMemProof := s.updateClientAndMembershipProof(ctx, simd, pt, [][]byte{packetReceiptPath})

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
			s.Require().NoError(types.GenerateAndSaveFixture(fmt.Sprintf("timeoutPacket-%s.json", pt.String()), s.contractAddresses.Erc20, "timeoutPacket", msg, packet))
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

func (s *IbcEurekaTestSuite) createICS20MsgSendPacket(
	sender ethcommon.Address,
	denom string,
	amount *big.Int,
	receiver string,
	sourceChannel string,
	timeoutTimestamp uint64,
	memo string,
) ics26router.IICS26RouterMsgsMsgSendPacket {
	msgSendTransfer := ics20transfer.IICS20TransferMsgsSendTransferMsg{
		Denom:            denom,
		Amount:           amount,
		Receiver:         receiver,
		SourceChannel:    sourceChannel,
		DestPort:         transfertypes.PortID,
		TimeoutTimestamp: timeoutTimestamp,
		Memo:             memo,
	}
	msgSendPacket, err := s.ics20Contract.ContractCaller.NewMsgSendPacketV1(nil, sender, msgSendTransfer)
	s.Require().NoError(err)

	// Because of the way abi generation work, the type returned by ics20 is ics20transfer.IICS26RouterMsgsMsgSendPacket
	// So we just move the values over here:
	return ics26router.IICS26RouterMsgsMsgSendPacket{
		SourceChannel:    sourceChannel,
		TimeoutTimestamp: timeoutTimestamp,
		Payloads: []ics26router.IICS26RouterMsgsPayload{
			{
				SourcePort: msgSendPacket.Payloads[0].SourcePort,
				DestPort:   msgSendPacket.Payloads[0].DestPort,
				Version:    msgSendPacket.Payloads[0].Version,
				Encoding:   msgSendPacket.Payloads[0].Encoding,
				Value:      msgSendPacket.Payloads[0].Value,
			},
		},
	}
}

func (s *IbcEurekaTestSuite) getCommitmentProof(path []byte) []byte {
	eth, simd := s.ChainA, s.ChainB

	storageKey := ethereum.GetCommitmentsStorageKey(path)
	storageKeys := []string{storageKey.Hex()}

	blockNumberHex := fmt.Sprintf("0x%x", s.LastEtheruemLightClientUpdate)
	proofResp, err := eth.EthAPI.GetProof(s.contractAddresses.IbcStore, storageKeys, blockNumberHex)
	s.Require().NoError(err)

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

func (s *IbcEurekaTestSuite) updateClientAndMembershipProof(
	ctx context.Context,
	counterpartyChain *cosmos.CosmosChain,
	proofType operator.SupportedProofType,
	ibcProofPaths [][]byte,
) (*ics26router.IICS02ClientMsgsHeight, []byte) {
	clientState, err := s.sp1Ics07Contract.GetClientState(nil)
	s.Require().NoError(err)

	trustedHeight := clientState.LatestHeight.RevisionHeight
	latestHeight, err := counterpartyChain.Height(ctx)
	s.Require().NoError(err)

	proofPaths := make([][][]byte, len(ibcProofPaths))
	for i, path := range ibcProofPaths {
		proofPaths[i] = [][]byte{
			[]byte("ibc"),
			path,
		}
	}

	proofPathsStr := operator.ToBase64KeyPaths(proofPaths...)

	args := append([]string{
		"--trust-level", testvalues.DefaultTrustLevel.String(),
		"--trusting-period", strconv.Itoa(testvalues.DefaultTrustPeriod),
		"--base64",
	},
		proofType.ToOperatorArgs()...,
	)
	proofHeight, ucAndMemProof, err := operator.UpdateClientAndMembershipProof(
		uint64(trustedHeight), uint64(latestHeight), proofPathsStr, args...,
	)
	s.Require().NoError(err)

	return &ics26router.IICS02ClientMsgsHeight{
		RevisionNumber: proofHeight.RevisionNumber,
		RevisionHeight: proofHeight.RevisionHeight,
	}, ucAndMemProof
}
