package main

import (
	"context"
	"crypto/ecdsa"
	"encoding/binary"
	"encoding/hex"
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

	solanago "github.com/gagliardetto/solana-go"
	"github.com/gagliardetto/solana-go/programs/token"
	"github.com/gagliardetto/solana-go/rpc"

	"github.com/cosmos/solidity-ibc-eureka/packages/go-abigen/ics26router"

	access_manager "github.com/cosmos/solidity-ibc-eureka/packages/go-anchor/accessmanager"
	attestation "github.com/cosmos/solidity-ibc-eureka/packages/go-anchor/attestation"
	ics26_router "github.com/cosmos/solidity-ibc-eureka/packages/go-anchor/ics26router"
	ics27_gmp "github.com/cosmos/solidity-ibc-eureka/packages/go-anchor/ics27gmp"
	ift "github.com/cosmos/solidity-ibc-eureka/packages/go-anchor/ift"

	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/attestor"
	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/e2esuite"
	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/ethereum"
	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/relayer"
	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/solana"
	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/testvalues"
	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/types/evmift"
	relayertypes "github.com/srdtrk/solidity-ibc-eureka/e2e/v8/types/relayer"
)

const (
	EthSolanaIFTTokenDecimals  = uint8(6)
	EthSolanaIFTMintAmount     = uint64(10_000_000) // 10 tokens with 6 decimals
	EthSolanaIFTTransferAmount = uint64(1_000_000)  // 1 token with 6 decimals

	EthClientIDOnSolana = testvalues.FirstAttestationsClientID // "attestations-0"
	SolanaClientIDOnEth = testvalues.CustomClientID            // "cosmoshub-1"

	numEthAttestors    = 1
	numSolAttestors    = 1
	ethSolGMPPortID    = testvalues.SolanaGMPPortID
	ethSolComputeUnits = uint32(400_000)

	ethAttestorKeystorePathTemplate    = "/tmp/ethsol_eth_attestor_%d"
	solanaAttestorKeystorePathTemplate = "/tmp/ethsol_sol_attestor_%d"
)

type EthereumSolanaIFTTestSuite struct {
	e2esuite.TestSuite

	SolanaRelayer *solanago.Wallet

	ethDeployer *ecdsa.PrivateKey
	ethUser     *ecdsa.PrivateKey

	contractAddresses ethereum.DeployedContracts

	RelayerClient  relayertypes.RelayerServiceClient
	RelayerProcess *os.Process

	SolanaAltAddress string

	ethAttestorAddresses []string
	ethAttestorResult    attestor.SetupResult
	solanaAttestorResult attestor.SetupResult

	IFTMintWallet      *solanago.Wallet
	IFTAppState        solanago.PublicKey
	IFTMintAuthority   solanago.PublicKey
	IFTBridge          solanago.PublicKey
	SenderTokenAccount solanago.PublicKey

	GMPAppStatePDA    solanago.PublicKey
	RouterStatePDA    solanago.PublicKey
	IBCClientPDA      solanago.PublicKey
	GMPIBCAppPDA      solanago.PublicKey
	ClientSequencePDA solanago.PublicKey
}

func (s *EthereumSolanaIFTTestSuite) IFTMint() solanago.PublicKey {
	return s.IFTMintWallet.PublicKey()
}

func (s *EthereumSolanaIFTTestSuite) IFTMintBytes() []byte {
	pk := s.IFTMintWallet.PublicKey()
	return pk[:]
}

func TestWithEthereumSolanaIFTTestSuite(t *testing.T) {
	suite.Run(t, new(EthereumSolanaIFTTestSuite))
}

func (s *EthereumSolanaIFTTestSuite) TearDownSuite() {
	ctx := context.Background()
	attestor.CleanupContainers(ctx, s.T(), s.ethAttestorResult.Containers)
	attestor.CleanupContainers(ctx, s.T(), s.solanaAttestorResult.Containers)

	if s.RelayerProcess != nil {
		s.T().Logf("Cleaning up relayer process (PID: %d)", s.RelayerProcess.Pid)
		if err := s.RelayerProcess.Kill(); err != nil {
			s.T().Logf("Failed to kill relayer process: %v", err)
		}
	}
}

func (s *EthereumSolanaIFTTestSuite) SetupSuite(ctx context.Context) {
	var err error

	err = os.Chdir("../..")
	s.Require().NoError(err)

	os.Setenv(testvalues.EnvKeyEthTestnetType, testvalues.EthTestnetTypeAnvil)
	os.Setenv(testvalues.EnvKeySolanaTestnetType, testvalues.SolanaTestnetType_Localnet)
	s.TestSuite.SetupSuite(ctx)

	s.T().Log("Waiting for Solana cluster to be ready...")
	err = s.Solana.Chain.WaitForClusterReady(ctx, 30*time.Second)
	s.Require().NoError(err, "Solana cluster failed to initialize")

	eth := s.Eth.Chains[0]

	s.Require().True(s.Run("Set up environment", func() {
		s.ethUser, err = eth.CreateAndFundUser()
		s.Require().NoError(err)

		s.ethDeployer, err = eth.CreateAndFundUserFromKey(testvalues.E2EDeployerPrivateKeyHex)
		s.Require().NoError(err)

		operatorKey, err := eth.CreateAndFundUser()
		s.Require().NoError(err)

		prover := os.Getenv(testvalues.EnvKeySp1Prover)
		if prover == "" {
			prover = testvalues.EnvValueSp1Prover_Mock
		}
		os.Setenv(testvalues.EnvKeySp1Prover, prover)
		os.Setenv(testvalues.EnvKeyVerifier, testvalues.EnvValueVerifier_Mock)

		if os.Getenv(testvalues.EnvKeyRustLog) == "" {
			os.Setenv(testvalues.EnvKeyRustLog, testvalues.EnvValueRustLog_Info)
		}
		os.Setenv(testvalues.EnvKeyEthRPC, eth.RPC)
		os.Setenv(testvalues.EnvKeyOperatorPrivateKey, hex.EncodeToString(crypto.FromECDSA(operatorKey)))
	}))

	s.Require().True(s.Run("Deploy Solana programs", func() {
		solanaUser := solanago.NewWallet()
		s.T().Logf("Created SolanaRelayer wallet: %s", solanaUser.PublicKey())

		s.Require().True(s.Run("Fund wallets", func() {
			const deployerFunding = 100 * testvalues.InitialSolBalance
			err := e2esuite.RunParallelTasks(
				e2esuite.ParallelTask{
					Name: "Fund SolanaRelayer",
					Run: func() error {
						_, err := s.Solana.Chain.FundUserWithRetry(ctx, solanaUser.PublicKey(), testvalues.InitialSolBalance, 5)
						return err
					},
				},
				e2esuite.ParallelTask{
					Name: "Fund Deployer",
					Run: func() error {
						_, err := s.Solana.Chain.FundUserWithRetry(ctx, solana.DeployerPubkey, deployerFunding, 5)
						return err
					},
				},
			)
			s.Require().NoError(err)
			s.SolanaRelayer = solanaUser
		}))

		s.Require().True(s.Run("Deploy programs", func() {
			const keypairDir = "solana-keypairs/localnet"
			const deployerPath = keypairDir + "/deployer_wallet.json"

			deployProgram := func(displayName, programName string) e2esuite.ParallelTaskWithResult[solanago.PublicKey] {
				return e2esuite.ParallelTaskWithResult[solanago.PublicKey]{
					Name: displayName,
					Run: func() (solanago.PublicKey, error) {
						s.T().Logf("Deploying %s...", displayName)
						keypairPath := fmt.Sprintf("%s/%s-keypair.json", keypairDir, programName)
						programID, err := s.Solana.Chain.DeploySolanaProgramAsync(ctx, programName, keypairPath, deployerPath)
						if err == nil {
							s.T().Logf("Deployed %s at: %s", displayName, programID)
						}
						return programID, err
					},
				}
			}

			deployResults, err := e2esuite.RunParallelTasksWithResults(
				deployProgram("Access Manager", "access_manager"),
				deployProgram("ICS26 Router", "ics26_router"),
				deployProgram("ICS27 GMP", "ics27_gmp"),
				deployProgram("IFT", "ift"),
				deployProgram("Attestation", "attestation"),
			)
			s.Require().NoError(err)

			access_manager.ProgramID = deployResults["Access Manager"]
			ics26_router.ProgramID = deployResults["ICS26 Router"]
			ics27_gmp.ProgramID = deployResults["ICS27 GMP"]
			ift.ProgramID = deployResults["IFT"]
			attestation.ProgramID = deployResults["Attestation"]
		}))
	}))

	s.Require().True(s.Run("Initialize Access Control", func() {
		accessControlAccount, _ := solana.AccessManager.AccessManagerPDA(access_manager.ProgramID)
		initInstruction, err := access_manager.NewInitializeInstruction(
			s.SolanaRelayer.PublicKey(),
			accessControlAccount,
			s.SolanaRelayer.PublicKey(),
			solanago.SystemProgramID,
			solanago.SysVarInstructionsPubkey,
		)
		s.Require().NoError(err)

		tx, err := s.Solana.Chain.NewTransactionFromInstructions(s.SolanaRelayer.PublicKey(), initInstruction)
		s.Require().NoError(err)
		_, err = s.Solana.Chain.SignAndBroadcastTxWithRetryAndTimeout(ctx, tx, rpc.CommitmentConfirmed, 30, s.SolanaRelayer)
		s.Require().NoError(err)
	}))

	s.Require().True(s.Run("Grant roles", func() {
		accessControlAccount, _ := solana.AccessManager.AccessManagerPDA(access_manager.ProgramID)
		const RELAYER_ROLE = uint64(1)
		const ID_CUSTOMIZER_ROLE = uint64(6)

		grantRelayerRoleIx, err := access_manager.NewGrantRoleInstruction(
			RELAYER_ROLE, s.SolanaRelayer.PublicKey(), accessControlAccount,
			s.SolanaRelayer.PublicKey(), solanago.SysVarInstructionsPubkey,
		)
		s.Require().NoError(err)

		grantIdCustomizerRoleIx, err := access_manager.NewGrantRoleInstruction(
			ID_CUSTOMIZER_ROLE, s.SolanaRelayer.PublicKey(), accessControlAccount,
			s.SolanaRelayer.PublicKey(), solanago.SysVarInstructionsPubkey,
		)
		s.Require().NoError(err)

		tx, err := s.Solana.Chain.NewTransactionFromInstructions(s.SolanaRelayer.PublicKey(), grantRelayerRoleIx, grantIdCustomizerRoleIx)
		s.Require().NoError(err)
		_, err = s.Solana.Chain.SignAndBroadcastTxWithRetryAndTimeout(ctx, tx, rpc.CommitmentConfirmed, 30, s.SolanaRelayer)
		s.Require().NoError(err)
	}))

	s.Require().True(s.Run("Initialize ICS26 Router", func() {
		routerStateAccount, _ := solana.Ics26Router.RouterStatePDA(ics26_router.ProgramID)
		initInstruction, err := ics26_router.NewInitializeInstruction(
			access_manager.ProgramID, routerStateAccount,
			s.SolanaRelayer.PublicKey(), solanago.SystemProgramID,
		)
		s.Require().NoError(err)

		tx, err := s.Solana.Chain.NewTransactionFromInstructions(s.SolanaRelayer.PublicKey(), initInstruction)
		s.Require().NoError(err)
		_, err = s.Solana.Chain.SignAndBroadcastTxWithRetryAndTimeout(ctx, tx, rpc.CommitmentConfirmed, 30, s.SolanaRelayer)
		s.Require().NoError(err)
	}))

	s.Require().True(s.Run("Initialize ICS27 GMP", func() {
		gmpAppStatePDA, _ := solana.Ics27Gmp.AppStateGmpportPDA(ics27_gmp.ProgramID)
		initInstruction, err := ics27_gmp.NewInitializeInstruction(
			access_manager.ProgramID, gmpAppStatePDA,
			s.SolanaRelayer.PublicKey(), solanago.SystemProgramID,
		)
		s.Require().NoError(err)

		tx, err := s.Solana.Chain.NewTransactionFromInstructions(s.SolanaRelayer.PublicKey(), initInstruction)
		s.Require().NoError(err)
		_, err = s.Solana.Chain.SignAndBroadcastTxWithRetry(ctx, tx, rpc.CommitmentConfirmed, s.SolanaRelayer)
		s.Require().NoError(err)
	}))

	s.Require().True(s.Run("Register ICS27 GMP with Router", func() {
		routerStateAccount, _ := solana.Ics26Router.RouterStatePDA(ics26_router.ProgramID)
		accessControlAccount, _ := solana.AccessManager.AccessManagerPDA(access_manager.ProgramID)
		ibcAppAccount, _ := solana.Ics26Router.IbcAppWithArgSeedPDA(ics26_router.ProgramID, []byte(ethSolGMPPortID))

		registerInstruction, err := ics26_router.NewAddIbcAppInstruction(
			ethSolGMPPortID,
			routerStateAccount, accessControlAccount, ibcAppAccount,
			ics27_gmp.ProgramID, s.SolanaRelayer.PublicKey(), s.SolanaRelayer.PublicKey(),
			solanago.SystemProgramID, solanago.SysVarInstructionsPubkey,
		)
		s.Require().NoError(err)

		tx, err := s.Solana.Chain.NewTransactionFromInstructions(s.SolanaRelayer.PublicKey(), registerInstruction)
		s.Require().NoError(err)
		_, err = s.Solana.Chain.SignAndBroadcastTxWithRetry(ctx, tx, rpc.CommitmentConfirmed, s.SolanaRelayer)
		s.Require().NoError(err)
	}))

	s.GMPAppStatePDA, _ = solana.Ics27Gmp.AppStateGmpportPDA(ics27_gmp.ProgramID)
	s.RouterStatePDA, _ = solana.Ics26Router.RouterStatePDA(ics26_router.ProgramID)
	s.IBCClientPDA, _ = solana.Ics26Router.ClientWithArgSeedPDA(ics26_router.ProgramID, []byte(EthClientIDOnSolana))
	s.GMPIBCAppPDA, _ = solana.Ics26Router.IbcAppWithArgSeedPDA(ics26_router.ProgramID, []byte(ethSolGMPPortID))
	s.ClientSequencePDA, _ = solana.Ics26Router.ClientSequenceWithArgSeedPDA(ics26_router.ProgramID, []byte(EthClientIDOnSolana))

	s.Require().True(s.Run("Generate Eth attestor keys", func() {
		var err error
		s.ethAttestorAddresses, err = attestor.GenerateAttestorKeys(ctx, attestor.GenerateAttestorKeysParams{
			Client:               s.GetDockerClient(),
			NumKeys:              numEthAttestors,
			KeystorePathTemplate: ethAttestorKeystorePathTemplate,
		})
		s.Require().NoError(err)
		s.T().Logf("Generated %d Eth attestor keys: %v", len(s.ethAttestorAddresses), s.ethAttestorAddresses)
	}))

	s.Require().True(s.Run("Deploy EVM contracts", func() {
		stdout, err := eth.ForgeScript(s.ethDeployer, testvalues.E2EDeployScriptPath)
		s.Require().NoError(err)

		s.contractAddresses, err = ethereum.GetEthContractsFromDeployOutput(string(stdout))
		s.Require().NoError(err)
		s.T().Logf("ICS26Router: %s, IFT: %s", s.contractAddresses.Ics26Router, s.contractAddresses.Ift)
	}))

	s.Require().True(s.Run("Verify SolanaIFTSendCallConstructor", func() {
		s.Require().NotEmpty(s.contractAddresses.SolanaIftConstructor, "SolanaIFTSendCallConstructor should be deployed by the deploy script")
		s.T().Logf("SolanaIFTSendCallConstructor at: %s", s.contractAddresses.SolanaIftConstructor)
	}))

	// NOTE: SetupAttestors registers t.Cleanup to stop containers. Must be called outside
	// s.Run() subtests so cleanup runs at end of test, not when subtest finishes.
	s.T().Log("Starting Eth attestors...")
	s.ethAttestorResult = attestor.SetupAttestors(ctx, s.T(), attestor.SetupParams{
		NumAttestors:         numEthAttestors,
		KeystorePathTemplate: ethAttestorKeystorePathTemplate,
		ChainType:            attestor.ChainTypeEvm,
		AdapterURL:           eth.DockerRPC,
		RouterAddress:        s.contractAddresses.Ics26Router,
		DockerClient:         s.GetDockerClient(),
		NetworkID:            s.GetNetworkID(),
	})
	for i, endpoint := range s.ethAttestorResult.Endpoints {
		err := attestor.CheckAttestorHealth(ctx, endpoint)
		s.Require().NoError(err, "Eth attestor %d at %s is not healthy", i, endpoint)
	}

	s.T().Log("Starting Solana attestors...")
	s.solanaAttestorResult = attestor.SetupAttestors(ctx, s.T(), attestor.SetupParams{
		NumAttestors:         numSolAttestors,
		KeystorePathTemplate: solanaAttestorKeystorePathTemplate,
		ChainType:            attestor.ChainTypeSolana,
		AdapterURL:           attestor.TransformLocalhostToDockerHost(testvalues.SolanaLocalnetRPC),
		RouterAddress:        ics26_router.ProgramID.String(),
		DockerClient:         s.GetDockerClient(),
		NetworkID:            s.GetNetworkID(),
		EnableHostAccess:     true,
	})

	s.Require().True(s.Run("Create Address Lookup Table", func() {
		altAddress := s.Solana.Chain.CreateIBCAddressLookupTableWithAttestation(
			ctx, s.T(), s.Require(), s.SolanaRelayer,
			eth.ChainID.String(), ethSolGMPPortID, EthClientIDOnSolana, EthClientIDOnSolana,
		)
		s.SolanaAltAddress = altAddress.String()
		s.T().Logf("Created ALT: %s", s.SolanaAltAddress)
	}))

	s.Require().True(s.Run("Initialize Attestation Light Client on Solana", func() {
		s.initializeAttestationLightClientOnSolana(ctx, EthClientIDOnSolana)
	}))

	s.Require().True(s.Run("Start Relayer", func() {
		config := relayer.NewConfigBuilder().
			EthToSolanaAttested(relayer.EthToSolanaAttestedParams{
				EthChainID:        eth.ChainID.String(),
				SolanaChainID:     testvalues.SolanaChainID,
				EthRPC:            eth.RPC,
				ICS26Address:      s.contractAddresses.Ics26Router,
				SolanaRPC:         testvalues.SolanaLocalnetRPC,
				ICS26ProgramID:    ics26_router.ProgramID.String(),
				FeePayer:          s.SolanaRelayer.PublicKey().String(),
				ALTAddress:        s.SolanaAltAddress,
				AttestorEndpoints: s.ethAttestorResult.Endpoints,
				AttestorTimeout:   30000,
				QuorumThreshold:   numEthAttestors,
			}).
			SolanaToEthAttested(relayer.SolanaToEthAttestedParams{
				SolanaChainID:     testvalues.SolanaChainID,
				EthChainID:        eth.ChainID.String(),
				SolanaRPC:         testvalues.SolanaLocalnetRPC,
				ICS26ProgramID:    ics26_router.ProgramID.String(),
				EthRPC:            eth.RPC,
				ICS26Address:      s.contractAddresses.Ics26Router,
				AttestorEndpoints: s.solanaAttestorResult.Endpoints,
				AttestorTimeout:   30000,
				QuorumThreshold:   numSolAttestors,
			}).
			Build()

		err := config.GenerateConfigFile(testvalues.RelayerConfigFilePath)
		s.Require().NoError(err)

		s.RelayerProcess, err = relayer.StartRelayer(testvalues.RelayerConfigFilePath)
		s.Require().NoError(err)

		s.T().Cleanup(func() {
			os.Remove(testvalues.RelayerConfigFilePath)
		})
	}))

	s.Require().True(s.Run("Create Relayer Client", func() {
		s.RelayerClient, err = relayer.GetGRPCClient(relayer.DefaultRelayerGRPCAddress())
		s.Require().NoError(err)
	}))

	s.Require().True(s.Run("Create Solana light client on Ethereum", func() {
		currentFinalizedSlot, err := s.Solana.Chain.RPCClient.GetSlot(ctx, rpc.CommitmentFinalized)
		s.Require().NoError(err)
		solanaTimestamp, err := s.Solana.Chain.RPCClient.GetBlockTime(ctx, currentFinalizedSlot)
		s.Require().NoError(err)

		// Convert attestor addresses to EIP-55 checksummed format (required by eth_attested.rs)
		checksummedAddrs := make([]string, len(s.solanaAttestorResult.Addresses))
		for i, addr := range s.solanaAttestorResult.Addresses {
			checksummedAddrs[i] = ethcommon.HexToAddress(addr).Hex()
		}

		resp, err := s.RelayerClient.CreateClient(ctx, &relayertypes.CreateClientRequest{
			SrcChain: testvalues.SolanaChainID,
			DstChain: eth.ChainID.String(),
			Parameters: map[string]string{
				testvalues.ParameterKey_AttestorAddresses: strings.Join(checksummedAddrs, ","),
				testvalues.ParameterKey_MinRequiredSigs:   strconv.Itoa(numSolAttestors),
				testvalues.ParameterKey_height:            strconv.FormatUint(currentFinalizedSlot, 10),
				testvalues.ParameterKey_timestamp:         strconv.FormatInt(int64(*solanaTimestamp), 10),
			},
		})
		s.Require().NoError(err)
		s.Require().NotEmpty(resp.Tx)

		// BroadcastTx with nil address = contract deployment
		ethRelayerSubmitter, err := eth.CreateAndFundUser()
		s.Require().NoError(err)
		receipt, err := eth.BroadcastTx(ctx, ethRelayerSubmitter, 15_000_000, nil, resp.Tx)
		s.Require().NoError(err)
		s.Require().Equal(ethtypes.ReceiptStatusSuccessful, receipt.Status)
		sp1Ics07Address := receipt.ContractAddress
		s.T().Logf("Solana light client deployed on Ethereum at: %s", sp1Ics07Address.Hex())

		ics26Contract, err := ics26router.NewContract(ethcommon.HexToAddress(s.contractAddresses.Ics26Router), eth.RPCClient)
		s.Require().NoError(err)

		counterpartyInfo := ics26router.IICS02ClientMsgsCounterpartyInfo{
			ClientId:     EthClientIDOnSolana,
			MerklePrefix: [][]byte{[]byte("")},
		}

		txOpts, err := eth.GetTransactOpts(s.ethDeployer)
		s.Require().NoError(err)

		tx, err := ics26Contract.AddClient(txOpts, SolanaClientIDOnEth, counterpartyInfo, sp1Ics07Address)
		s.Require().NoError(err)

		addClientReceipt, err := eth.GetTxReciept(ctx, tx.Hash())
		s.Require().NoError(err)
		s.Require().Equal(ethtypes.ReceiptStatusSuccessful, addClientReceipt.Status)
	}))

	s.Require().True(s.Run("Add attestation client to Router on Solana", func() {
		routerStateAccount, _ := solana.Ics26Router.RouterStatePDA(ics26_router.ProgramID)
		accessControlAccount, _ := solana.AccessManager.AccessManagerPDA(access_manager.ProgramID)
		clientAccount, _ := solana.Ics26Router.ClientWithArgSeedPDA(ics26_router.ProgramID, []byte(EthClientIDOnSolana))
		clientSequenceAccount, _ := solana.Ics26Router.ClientSequenceWithArgSeedPDA(ics26_router.ProgramID, []byte(EthClientIDOnSolana))

		counterpartyInfo := ics26_router.SolanaIbcTypesRouterCounterpartyInfo{
			ClientId:     SolanaClientIDOnEth,
			MerklePrefix: [][]byte{[]byte("")},
		}

		addClientInstruction, err := ics26_router.NewAddClientInstruction(
			EthClientIDOnSolana, counterpartyInfo,
			s.SolanaRelayer.PublicKey(), routerStateAccount, accessControlAccount,
			clientAccount, clientSequenceAccount, s.SolanaRelayer.PublicKey(),
			attestation.ProgramID, solanago.SystemProgramID, solanago.SysVarInstructionsPubkey,
		)
		s.Require().NoError(err)

		tx, err := s.Solana.Chain.NewTransactionFromInstructions(s.SolanaRelayer.PublicKey(), addClientInstruction)
		s.Require().NoError(err)
		_, err = s.Solana.Chain.SignAndBroadcastTxWithRetry(ctx, tx, rpc.CommitmentConfirmed, s.SolanaRelayer)
		s.Require().NoError(err)
	}))
}

func (s *EthereumSolanaIFTTestSuite) initializeAttestationLightClientOnSolana(ctx context.Context, clientID string) {
	var attestorAddresses [][20]uint8
	for _, addr := range s.ethAttestorAddresses {
		addrHex := addr
		if len(addrHex) >= 2 && addrHex[:2] == "0x" {
			addrHex = addrHex[2:]
		}

		addrBytes, err := hex.DecodeString(addrHex)
		s.Require().NoError(err)
		s.Require().Len(addrBytes, 20, "Attestor address must be 20 bytes")

		var addrArray [20]uint8
		copy(addrArray[:], addrBytes)
		attestorAddresses = append(attestorAddresses, addrArray)
	}

	latestHeight := uint64(1)
	timestamp := uint64(time.Now().Unix())
	minRequiredSigs := uint8(numEthAttestors)

	clientStatePDA, _ := solana.Attestation.ClientWithArgSeedPDA(attestation.ProgramID, []byte(clientID))
	appStatePDA, _ := solana.Attestation.AppStatePDA(attestation.ProgramID)

	heightBytes := make([]byte, 8)
	binary.LittleEndian.PutUint64(heightBytes, latestHeight)
	consensusStatePDA, _ := solana.Attestation.ConsensusStateWithArgAndAccountSeedPDA(
		attestation.ProgramID, clientStatePDA.Bytes(), heightBytes,
	)

	initInstruction, err := attestation.NewInitializeInstruction(
		clientID, latestHeight, attestorAddresses, minRequiredSigs, timestamp,
		access_manager.ProgramID,
		clientStatePDA, consensusStatePDA, appStatePDA,
		s.SolanaRelayer.PublicKey(), solanago.SystemProgramID,
	)
	s.Require().NoError(err)

	tx, err := s.Solana.Chain.NewTransactionFromInstructions(s.SolanaRelayer.PublicKey(), initInstruction)
	s.Require().NoError(err)

	sig, err := s.Solana.Chain.SignAndBroadcastTxWithRetryAndTimeout(ctx, tx, rpc.CommitmentConfirmed, 30, s.SolanaRelayer)
	s.Require().NoError(err)
	s.T().Logf("Attestation Light Client initialized on Solana - tx: %s", sig)
}

func (s *EthereumSolanaIFTTestSuite) createIFTSplToken(ctx context.Context, mintWallet *solanago.Wallet) {
	mint := mintWallet.PublicKey()
	appStatePDA, _ := solana.Ift.IftAppStatePDA(ift.ProgramID, mint[:])
	mintAuthorityPDA, _ := solana.Ift.IftMintAuthorityPDA(ift.ProgramID, mint[:])

	s.IFTAppState = appStatePDA
	s.IFTMintAuthority = mintAuthorityPDA

	initIx, err := ift.NewCreateSplTokenInstruction(
		EthSolanaIFTTokenDecimals,
		s.SolanaRelayer.PublicKey(), // admin
		ics27_gmp.ProgramID,
		appStatePDA, mint, mintAuthorityPDA,
		s.SolanaRelayer.PublicKey(),
		token.ProgramID, solanago.SystemProgramID,
	)
	s.Require().NoError(err)

	tx, err := s.Solana.Chain.NewTransactionFromInstructions(s.SolanaRelayer.PublicKey(), initIx)
	s.Require().NoError(err)

	_, err = s.Solana.Chain.SignAndBroadcastTxWithRetry(ctx, tx, rpc.CommitmentConfirmed, s.SolanaRelayer, mintWallet)
	s.Require().NoError(err)
}

func (s *EthereumSolanaIFTTestSuite) registerSolanaIFTBridgeForEVM(ctx context.Context, clientID string, counterpartyIFTAddress string) {
	bridgePDA, _ := solana.Ift.IftBridgePDA(ift.ProgramID, s.IFTMintBytes(), []byte(clientID))
	s.IFTBridge = bridgePDA

	// EVM counterparty uses the unit variant (no fields)
	evmOpt := ift.IftStateChainOptions_Evm(0)
	registerMsg := ift.IftStateRegisterIftBridgeMsg{
		ClientId:               clientID,
		CounterpartyIftAddress: counterpartyIFTAddress,
		ChainOptions:           &evmOpt,
	}

	registerIx, err := ift.NewRegisterIftBridgeInstruction(
		registerMsg, s.IFTAppState, bridgePDA,
		s.SolanaRelayer.PublicKey(), // admin
		s.SolanaRelayer.PublicKey(), // payer
		solanago.SystemProgramID,
	)
	s.Require().NoError(err)

	tx, err := s.Solana.Chain.NewTransactionFromInstructions(s.SolanaRelayer.PublicKey(), registerIx)
	s.Require().NoError(err)

	_, err = s.Solana.Chain.SignAndBroadcastTxWithRetry(ctx, tx, rpc.CommitmentConfirmed, s.SolanaRelayer)
	s.Require().NoError(err)

	s.T().Logf("IFT Bridge registered for client %s (EVM counterparty)", clientID)
	s.T().Logf("  Bridge PDA: %s, Counterparty IFT: %s", bridgePDA, counterpartyIFTAddress)
}

func (s *EthereumSolanaIFTTestSuite) Test_EthSolana_IFT_Roundtrip() {
	ctx := context.Background()
	s.SetupSuite(ctx)

	eth := s.Eth.Chains[0]
	ethIFTAddress := ethcommon.HexToAddress(s.contractAddresses.Ift)

	s.Require().True(s.Run("Create IFT SPL token on Solana", func() {
		s.IFTMintWallet = solanago.NewWallet()
		s.createIFTSplToken(ctx, s.IFTMintWallet)

		mint := s.IFTMint()
		tokenAccount, err := s.Solana.Chain.CreateOrGetAssociatedTokenAccount(ctx, s.SolanaRelayer, mint, s.SolanaRelayer.PublicKey())
		s.Require().NoError(err)
		s.SenderTokenAccount = tokenAccount
		s.T().Logf("SPL token mint: %s, token account: %s", mint, tokenAccount)
	}))

	s.Require().True(s.Run("Register IFT bridges", func() {
		s.registerSolanaIFTBridgeForEVM(ctx, EthClientIDOnSolana, ethIFTAddress.Hex())

		iftContract, err := evmift.NewContract(ethIFTAddress, eth.RPCClient)
		s.Require().NoError(err)

		txOpts, err := eth.GetTransactOpts(s.ethDeployer)
		s.Require().NoError(err)

		// counterpartyIFTAddress for Solana is the IFT program ID
		tx, err := iftContract.RegisterIFTBridge(txOpts, SolanaClientIDOnEth, ift.ProgramID.String(), ethcommon.HexToAddress(s.contractAddresses.SolanaIftConstructor))
		s.Require().NoError(err)

		receipt, err := eth.GetTxReciept(ctx, tx.Hash())
		s.Require().NoError(err)
		s.Require().Equal(ethtypes.ReceiptStatusSuccessful, receipt.Status)
		s.T().Logf("IFT bridge registered on Ethereum for Solana counterparty")
	}))

	ethUserAddr := crypto.PubkeyToAddress(s.ethUser.PublicKey)
	transferAmount := big.NewInt(int64(EthSolanaIFTTransferAmount))

	s.Require().True(s.Run("Mint tokens on Ethereum", func() {
		iftContract, err := evmift.NewContract(ethIFTAddress, eth.RPCClient)
		s.Require().NoError(err)

		txOpts, err := eth.GetTransactOpts(s.ethDeployer)
		s.Require().NoError(err)

		tx, err := iftContract.Mint(txOpts, ethUserAddr, transferAmount)
		s.Require().NoError(err)

		receipt, err := eth.GetTxReciept(ctx, tx.Hash())
		s.Require().NoError(err)
		s.Require().Equal(ethtypes.ReceiptStatusSuccessful, receipt.Status)

		balance, err := iftContract.BalanceOf(nil, ethUserAddr)
		s.Require().NoError(err)
		s.Require().Equal(transferAmount.String(), balance.String())
	}))

	var ethSendTxHash []byte
	s.Require().True(s.Run("Transfer: Ethereum -> Solana", func() {
		iftContract, err := evmift.NewContract(ethIFTAddress, eth.RPCClient)
		s.Require().NoError(err)

		txOpts, err := eth.GetTransactOpts(s.ethUser)
		s.Require().NoError(err)

		timeout := uint64(time.Now().Add(30 * time.Minute).Unix())
		// Receiver is the Solana public key in hex format (SolanaIFTSendCallConstructor expects 0x + 64 hex chars)
		solanaReceiverHex := "0x" + hex.EncodeToString(s.SolanaRelayer.PublicKey().Bytes())
		tx, err := iftContract.IftTransfer(txOpts, SolanaClientIDOnEth, solanaReceiverHex, transferAmount, timeout)
		s.Require().NoError(err)

		receipt, err := eth.GetTxReciept(ctx, tx.Hash())
		s.Require().NoError(err)
		s.Require().Equal(ethtypes.ReceiptStatusSuccessful, receipt.Status)
		ethSendTxHash = receipt.TxHash.Bytes()
		s.T().Logf("Ethereum -> Solana transfer tx: %s", receipt.TxHash.Hex())
	}))

	s.Require().True(s.Run("Verify tokens burned on Ethereum", func() {
		iftContract, err := evmift.NewContract(ethIFTAddress, eth.RPCClient)
		s.Require().NoError(err)

		balance, err := iftContract.BalanceOf(nil, ethUserAddr)
		s.Require().NoError(err)
		s.Require().Equal("0", balance.String())
	}))

	s.Require().True(s.Run("Relay to Solana", func() {
		ics26Address := ethcommon.HexToAddress(s.contractAddresses.Ics26Router)

		resp, err := s.RelayerClient.RelayByTx(ctx, &relayertypes.RelayByTxRequest{
			SrcChain:    eth.ChainID.String(),
			DstChain:    testvalues.SolanaChainID,
			SourceTxIds: [][]byte{ethSendTxHash},
			SrcClientId: SolanaClientIDOnEth,
			DstClientId: EthClientIDOnSolana,
		})
		s.Require().NoError(err)
		s.Require().NotEmpty(resp.Tx)

		sig, err := s.Solana.Chain.SubmitChunkedRelayPackets(ctx, s.T(), resp, s.SolanaRelayer)
		s.Require().NoError(err)
		s.T().Logf("Solana recv tx: %s", sig)

		ackResp, err := s.RelayerClient.RelayByTx(ctx, &relayertypes.RelayByTxRequest{
			SrcChain:    testvalues.SolanaChainID,
			DstChain:    eth.ChainID.String(),
			SourceTxIds: [][]byte{[]byte(sig.String())},
			SrcClientId: EthClientIDOnSolana,
			DstClientId: SolanaClientIDOnEth,
		})
		s.Require().NoError(err)

		receipt, err := eth.BroadcastTx(ctx, s.ethUser, 15_000_000, &ics26Address, ackResp.Tx)
		s.Require().NoError(err)
		s.Require().Equal(ethtypes.ReceiptStatusSuccessful, receipt.Status)
	}))

	s.Require().True(s.Run("Verify tokens minted on Solana", func() {
		balance, err := s.Solana.Chain.GetTokenBalance(ctx, s.SenderTokenAccount)
		s.Require().NoError(err)
		s.Require().Equal(EthSolanaIFTTransferAmount, balance)
	}))

	var solanaToEthSequence uint64
	var solanaTransferTxSig solanago.Signature
	s.Require().True(s.Run("Transfer: Solana -> Ethereum", func() {
		baseSeq, err := s.Solana.Chain.GetNextSequenceNumber(ctx, s.ClientSequencePDA)
		s.Require().NoError(err)

		solanaToEthSequence = solana.CalculateNamespacedSequence(baseSeq, ics27_gmp.ProgramID, s.SolanaRelayer.PublicKey())
		seqBytes := make([]byte, 8)
		binary.LittleEndian.PutUint64(seqBytes, solanaToEthSequence)

		packetCommitmentPDA, _ := solana.Ics26Router.PacketCommitmentWithArgSeedPDA(ics26_router.ProgramID, []byte(EthClientIDOnSolana), seqBytes)
		pendingTransferPDA, _ := solana.Ift.PendingTransferPDA(ift.ProgramID, s.IFTMintBytes(), []byte(EthClientIDOnSolana), seqBytes)

		solanaClockTime, err := s.Solana.Chain.GetSolanaClockTime(ctx)
		s.Require().NoError(err)

		transferMsg := ift.IftStateIftTransferMsg{
			ClientId:         EthClientIDOnSolana,
			Receiver:         ethUserAddr.Hex(),
			Amount:           EthSolanaIFTTransferAmount,
			TimeoutTimestamp: solanaClockTime + 900,
		}

		transferIx, err := ift.NewIftTransferInstruction(
			transferMsg, s.IFTAppState, s.IFTBridge, s.IFTMint(), s.SenderTokenAccount,
			s.SolanaRelayer.PublicKey(), s.SolanaRelayer.PublicKey(),
			token.ProgramID, solanago.SystemProgramID, ics27_gmp.ProgramID, s.GMPAppStatePDA,
			ics26_router.ProgramID, s.RouterStatePDA, s.ClientSequencePDA, packetCommitmentPDA,
			solanago.SysVarInstructionsPubkey, s.GMPIBCAppPDA, s.IBCClientPDA, pendingTransferPDA,
		)
		s.Require().NoError(err)

		computeBudgetIx := solana.NewComputeBudgetInstruction(ethSolComputeUnits)
		tx, err := s.Solana.Chain.NewTransactionFromInstructions(s.SolanaRelayer.PublicKey(), computeBudgetIx, transferIx)
		s.Require().NoError(err)

		solanaTransferTxSig, err = s.Solana.Chain.SignAndBroadcastTxWithRetry(ctx, tx, rpc.CommitmentConfirmed, s.SolanaRelayer)
		s.Require().NoError(err)
		s.T().Logf("Solana -> Ethereum transfer tx: %s", solanaTransferTxSig)
	}))

	s.Require().True(s.Run("Verify tokens burned on Solana", func() {
		balance, err := s.Solana.Chain.GetTokenBalance(ctx, s.SenderTokenAccount)
		s.Require().NoError(err)
		s.Require().Equal(uint64(0), balance)
	}))

	s.Require().True(s.Run("Relay to Ethereum", func() {
		ics26Address := ethcommon.HexToAddress(s.contractAddresses.Ics26Router)

		resp, err := s.RelayerClient.RelayByTx(ctx, &relayertypes.RelayByTxRequest{
			SrcChain:    testvalues.SolanaChainID,
			DstChain:    eth.ChainID.String(),
			SourceTxIds: [][]byte{[]byte(solanaTransferTxSig.String())},
			SrcClientId: EthClientIDOnSolana,
			DstClientId: SolanaClientIDOnEth,
		})
		s.Require().NoError(err)
		s.Require().NotEmpty(resp.Tx)

		receipt, err := eth.BroadcastTx(ctx, s.ethUser, 15_000_000, &ics26Address, resp.Tx)
		s.Require().NoError(err)
		s.Require().Equal(ethtypes.ReceiptStatusSuccessful, receipt.Status)
		s.T().Logf("Ethereum recv tx: %s", receipt.TxHash.Hex())

		ackResp, err := s.RelayerClient.RelayByTx(ctx, &relayertypes.RelayByTxRequest{
			SrcChain:    eth.ChainID.String(),
			DstChain:    testvalues.SolanaChainID,
			SourceTxIds: [][]byte{receipt.TxHash.Bytes()},
			SrcClientId: SolanaClientIDOnEth,
			DstClientId: EthClientIDOnSolana,
		})
		s.Require().NoError(err)

		_, err = s.Solana.Chain.SubmitChunkedRelayPackets(ctx, s.T(), ackResp, s.SolanaRelayer)
		s.Require().NoError(err)
	}))

	s.Require().True(s.Run("Verify tokens received on Ethereum", func() {
		iftContract, err := evmift.NewContract(ethIFTAddress, eth.RPCClient)
		s.Require().NoError(err)

		balance, err := iftContract.BalanceOf(nil, ethUserAddr)
		s.Require().NoError(err)
		s.Require().Equal(transferAmount.String(), balance.String(), "Ethereum user should have tokens back after roundtrip")
	}))

	s.Require().True(s.Run("Verify PendingTransfer closed on Solana", func() {
		s.Solana.Chain.VerifyPendingTransferClosed(ctx, s.T(), s.Require(),
			ift.ProgramID, s.IFTMint(), EthClientIDOnSolana, solanaToEthSequence)
	}))
}
