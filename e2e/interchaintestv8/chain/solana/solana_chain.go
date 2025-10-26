package solana

import (
	"bytes"
	"context"
	"encoding/binary"
	"fmt"
	"io"
	"net"
	"slices"
	"strconv"
	"time"

	dockercontainer "github.com/docker/docker/api/types/container"
	dockerimagetypes "github.com/docker/docker/api/types/image"
	"github.com/docker/docker/api/types/volume"
	"github.com/docker/go-connections/nat"
	bin "github.com/gagliardetto/binary"
	dockerclient "github.com/moby/moby/client"
	"go.uber.org/zap"

	"github.com/gagliardetto/solana-go"
	"github.com/gagliardetto/solana-go/programs/system"
	"github.com/gagliardetto/solana-go/rpc"
	confirm "github.com/gagliardetto/solana-go/rpc/sendAndConfirmTransaction"
	"github.com/gagliardetto/solana-go/rpc/ws"

	sdkmath "cosmossdk.io/math"

	"github.com/cosmos/interchaintest/v10/dockerutil"
	"github.com/cosmos/interchaintest/v10/ibc"
	"github.com/cosmos/interchaintest/v10/testutil"

	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/testvalues"
)

const (
	rpcPort = "8899/tcp"
	wsPort  = "8900/tcp"
)

var natPorts = nat.PortMap{
	nat.Port(rpcPort): {},
	nat.Port(wsPort):  {},
}

type SolanaChain struct {
	testName string
	cfg      ibc.ChainConfig
	log      *zap.Logger

	volumeName   string
	networkID    string
	dockerClient *dockerclient.Client

	containerLifecycle *dockerutil.ContainerLifecycle

	hostRPCPort string
	hostWSPort  string
	RPCClient   *rpc.Client
	WSClient    *ws.Client

	// Faucet wallet for funding test accounts
	Faucet *solana.Wallet

	// Store created wallets
	wallets map[string]*solana.Wallet
}

func NewSolanaChain(testName string, chainConfig ibc.ChainConfig, log *zap.Logger) *SolanaChain {
	return &SolanaChain{
		testName: testName,
		cfg:      chainConfig,
		log:      log,
		Faucet:   solana.NewWallet(),
		wallets:  make(map[string]*solana.Wallet),
	}
}

func (c *SolanaChain) Config() ibc.ChainConfig {
	return c.cfg
}

func (c *SolanaChain) Initialize(ctx context.Context, testName string, cli *dockerclient.Client, networkID string) error {
	chainCfg := c.Config()
	c.pullImages(ctx, cli)
	image := chainCfg.Images[0]

	c.containerLifecycle = dockerutil.NewContainerLifecycle(c.log, cli, c.Name())

	v, err := cli.VolumeCreate(ctx, volume.CreateOptions{
		Labels: map[string]string{
			dockerutil.CleanupLabel:   testName,
			dockerutil.NodeOwnerLabel: c.Name(),
		},
	})
	if err != nil {
		return fmt.Errorf("creating volume for chain node: %w", err)
	}
	c.volumeName = v.Name
	c.networkID = networkID
	c.dockerClient = cli

	if err := dockerutil.SetVolumeOwner(ctx, dockerutil.VolumeOwnerOptions{
		Log:        c.log,
		Client:     cli,
		VolumeName: v.Name,
		ImageRef:   image.Ref(),
		TestName:   testName,
		UidGid:     image.UIDGID,
	}); err != nil {
		return fmt.Errorf("set volume owner: %w", err)
	}

	return nil
}

func (c *SolanaChain) Name() string {
	return fmt.Sprintf("solana-%s-%s-%s", c.cfg.ChainID, c.cfg.Bin, dockerutil.SanitizeContainerName(c.testName))
}

func (c *SolanaChain) HomeDir() string {
	return "/home/solana"
}

func (c *SolanaChain) Bind() []string {
	return []string{fmt.Sprintf("%s:%s", c.volumeName, c.HomeDir())}
}

func (c *SolanaChain) pullImages(ctx context.Context, cli *dockerclient.Client) {
	for _, image := range c.Config().Images {
		rc, err := cli.ImagePull(
			ctx,
			image.Repository+":"+image.Version,
			dockerimagetypes.PullOptions{},
		)
		if err != nil {
			c.log.Error("Failed to pull image",
				zap.Error(err),
				zap.String("repository", image.Repository),
				zap.String("tag", image.Version),
			)
		} else {
			_, _ = io.Copy(io.Discard, rc)
			_ = rc.Close()
		}
	}
}

func (c *SolanaChain) Start(testName string, ctx context.Context, additionalGenesisWallets ...ibc.WalletAmount) error {
	// Initialize dockerClient if needed for proper interchaintest integration
	if c.containerLifecycle == nil {
		return fmt.Errorf("containerLifecycle not initialized - call Initialize first")
	}
	// Solana test validator command
	cmd := []string{
		"solana-test-validator",
		"--reset",
		"--rpc-port", "8899",
		"--bind-address", "0.0.0.0",
		"--mint", c.Faucet.PublicKey().String(),
		"--faucet-sol", "1000000", // Give faucet 1M SOL
	}

	// Add any additional start args from config
	cmd = append(cmd, c.cfg.AdditionalStartArgs...)

	usingPorts := nat.PortMap{}
	for k, v := range natPorts {
		usingPorts[k] = v
	}

	err := c.containerLifecycle.CreateContainer(ctx, c.testName, c.networkID, c.cfg.Images[0], usingPorts, "", c.Bind(), nil, c.HostName(), cmd, nil, c.cfg.Env)
	if err != nil {
		return err
	}

	c.log.Info("Starting Solana container", zap.String("container", c.Name()))

	if err := c.containerLifecycle.StartContainer(ctx); err != nil {
		return err
	}

	// Get host ports for RPC and WebSocket
	hostPorts, err := c.containerLifecycle.GetHostPorts(ctx, rpcPort)
	if err != nil {
		return err
	}
	if len(hostPorts) == 0 {
		return fmt.Errorf("no RPC host port found")
	}
	c.hostRPCPort = hostPorts[0]
	c.log.Info("RPC endpoint configured", zap.String("hostPort", c.hostRPCPort), zap.String("address", c.GetHostRPCAddress()))

	// For WebSocket, we need to construct the host port manually since solana-test-validator
	// uses RPC port + 1 for WebSocket, but the port might not show up in Docker's port mappings
	// until the validator process actually opens it.
	// Extract just the port number from RPC host port (format is "0.0.0.0:PORT")
	_, rpcPortStr, err := net.SplitHostPort(c.hostRPCPort)
	if err != nil {
		return fmt.Errorf("failed to parse RPC host port %s: %w", c.hostRPCPort, err)
	}
	rpcPortNum, err := strconv.Atoi(rpcPortStr)
	if err != nil {
		return fmt.Errorf("failed to convert RPC port to number: %w", err)
	}
	// WebSocket is always RPC port + 1
	c.hostWSPort = fmt.Sprintf("0.0.0.0:%d", rpcPortNum+1)
	c.log.Info("WebSocket endpoint configured", zap.String("hostPort", c.hostWSPort), zap.String("address", c.GetHostWSAddress()))

	time.Sleep(5 * time.Second)

	c.RPCClient = rpc.New(c.GetHostRPCAddress())

	var wsErr error
	maxAttempts := 30
	for i := 0; i < maxAttempts; i++ {
		c.WSClient, wsErr = ws.Connect(ctx, c.GetHostWSAddress())
		if wsErr == nil {
			c.log.Info("WebSocket connected successfully", zap.Int("attempts", i+1))
			break
		}
		c.log.Debug("WebSocket connection attempt failed", zap.Int("attempt", i+1), zap.String("address", c.GetHostWSAddress()), zap.Error(wsErr))
		time.Sleep(2 * time.Second)
	}
	if wsErr != nil {
		// Get container logs to help debug
		containerID := c.containerLifecycle.ContainerID()
		logsReader, logErr := c.dockerClient.ContainerLogs(ctx, containerID, dockercontainer.LogsOptions{
			ShowStdout: true,
			ShowStderr: true,
			Tail:       "50",
		})
		if logErr == nil {
			defer logsReader.Close()
			logBytes, _ := io.ReadAll(logsReader)
			if len(logBytes) > 0 {
				c.log.Error("Solana container logs (last 50 lines):", zap.String("logs", string(logBytes)))
			}
		}
		return fmt.Errorf("failed to connect to Solana WebSocket at %s after %d attempts (60s): %w", c.GetHostWSAddress(), maxAttempts, wsErr)
	}

	for keyName, wallet := range c.wallets {
		err := c.SendFunds(ctx, "", ibc.WalletAmount{
			Address: wallet.PublicKey().String(),
			Denom:   "SOL",
			Amount:  sdkmath.NewInt(1000000000),
		})
		if err != nil {
			c.log.Warn("Failed to fund wallet", zap.String("wallet", keyName), zap.Error(err))
		}
	}

	// Wait for blocks to ensure chain is running
	return testutil.WaitForBlocks(ctx, 2, c)
}

func (c *SolanaChain) HostName() string {
	return dockerutil.CondenseHostName(c.Name())
}

func (c *SolanaChain) Exec(ctx context.Context, cmd []string, env []string) (stdout, stderr []byte, err error) {
	job := c.NewJob()
	opts := dockerutil.ContainerOptions{
		Env:   env,
		Binds: c.Bind(),
	}
	res := job.Run(ctx, cmd, opts)
	return res.Stdout, res.Stderr, res.Err
}

func (c *SolanaChain) NewJob() *dockerutil.Image {
	return dockerutil.NewImage(c.Logger(), c.dockerClient, c.networkID, c.testName, c.cfg.Images[0].Repository, c.cfg.Images[0].Version)
}

func (c *SolanaChain) Logger() *zap.Logger {
	return c.log.With(
		zap.String("chain_id", c.cfg.ChainID),
		zap.String("test", c.testName),
	)
}

func (c *SolanaChain) GetRPCAddress() string {
	return fmt.Sprintf("http://%s:8899", c.HostName())
}

func (c *SolanaChain) GetWSAddress() string {
	return fmt.Sprintf("ws://%s:8900", c.HostName())
}

func (c *SolanaChain) GetHostRPCAddress() string {
	// Replace 0.0.0.0 with 127.0.0.1 for client connections
	addr := c.hostRPCPort
	if host, port, err := net.SplitHostPort(addr); err == nil && host == "0.0.0.0" {
		addr = net.JoinHostPort("127.0.0.1", port)
	}
	return "http://" + addr
}

func (c *SolanaChain) GetHostWSAddress() string {
	// Replace 0.0.0.0 with 127.0.0.1 for client connections
	addr := c.hostWSPort
	if host, port, err := net.SplitHostPort(addr); err == nil && host == "0.0.0.0" {
		addr = net.JoinHostPort("127.0.0.1", port)
	}
	return "ws://" + addr
}

func (c *SolanaChain) Height(ctx context.Context) (int64, error) {
	slot, err := c.RPCClient.GetSlot(ctx, rpc.CommitmentFinalized)
	if err != nil {
		return 0, fmt.Errorf("failed to get slot: %w", err)
	}
	return int64(slot), nil
}

func (c *SolanaChain) GetBalance(ctx context.Context, address string, denom string) (sdkmath.Int, error) {
	pubkey, err := solana.PublicKeyFromBase58(address)
	if err != nil {
		return sdkmath.Int{}, fmt.Errorf("invalid address: %w", err)
	}

	balance, err := c.RPCClient.GetBalance(ctx, pubkey, rpc.CommitmentFinalized)
	if err != nil {
		return sdkmath.Int{}, fmt.Errorf("failed to get balance: %w", err)
	}

	return sdkmath.NewIntFromUint64(balance.Value), nil
}

func (c *SolanaChain) CreateKey(ctx context.Context, keyName string) error {
	if _, exists := c.wallets[keyName]; exists {
		return fmt.Errorf("wallet with name %s already exists", keyName)
	}

	wallet := solana.NewWallet()
	c.wallets[keyName] = wallet

	if c.RPCClient != nil {
		err := c.SendFunds(ctx, "", ibc.WalletAmount{
			Address: wallet.PublicKey().String(),
			Denom:   "SOL",
			Amount:  sdkmath.NewInt(1000000000),
		})
		return err
	}

	return nil
}

func (c *SolanaChain) RecoverKey(ctx context.Context, keyName string, mnemonic string) error {
	return fmt.Errorf("mnemonic recovery not implemented for Solana")
}

func (c *SolanaChain) GetAddress(ctx context.Context, keyName string) ([]byte, error) {
	wallet, exists := c.wallets[keyName]
	if !exists {
		return nil, fmt.Errorf("wallet %s not found", keyName)
	}

	return wallet.PublicKey().Bytes(), nil
}

// GetWallet returns the wallet for a given key name
func (c *SolanaChain) GetWallet(keyName string) (*solana.Wallet, error) {
	wallet, exists := c.wallets[keyName]
	if !exists {
		return nil, fmt.Errorf("wallet %s not found", keyName)
	}
	return wallet, nil
}

func (c *SolanaChain) SendFunds(ctx context.Context, keyName string, amount ibc.WalletAmount) error {
	recipientPubkey, err := solana.PublicKeyFromBase58(amount.Address)
	if err != nil {
		return fmt.Errorf("invalid recipient address: %w", err)
	}

	// Get sender wallet (use faucet if keyName is empty)
	var senderWallet *solana.Wallet
	if keyName == "" {
		senderWallet = c.Faucet
	} else {
		var exists bool
		senderWallet, exists = c.wallets[keyName]
		if !exists {
			return fmt.Errorf("wallet %s not found", keyName)
		}
	}

	// Create transfer instruction
	recent, err := c.RPCClient.GetLatestBlockhash(ctx, rpc.CommitmentFinalized)
	if err != nil {
		return err
	}

	tx, err := solana.NewTransaction(
		[]solana.Instruction{
			system.NewTransferInstruction(
				amount.Amount.Uint64(),
				senderWallet.PublicKey(),
				recipientPubkey,
			).Build(),
		},
		recent.Value.Blockhash,
		solana.TransactionPayer(senderWallet.PublicKey()),
	)
	if err != nil {
		return err
	}

	// Sign and send transaction
	_, err = tx.Sign(func(key solana.PublicKey) *solana.PrivateKey {
		if key.Equals(senderWallet.PublicKey()) {
			return &senderWallet.PrivateKey
		}
		return nil
	})
	if err != nil {
		return err
	}

	sig, err := c.RPCClient.SendTransactionWithOpts(ctx, tx, rpc.TransactionOpts{
		SkipPreflight: true,
	})
	if err != nil {
		return err
	}

	// Wait for transaction to be confirmed
	err = c.WaitForTxStatus(sig, rpc.ConfirmationStatusFinalized)
	if err != nil {
		return fmt.Errorf("failed to wait for funding transaction: %w", err)
	}

	c.log.Info("Sent funds", zap.String("signature", sig.String()))
	return nil
}

func (c *SolanaChain) SendFundsWithNote(ctx context.Context, keyName string, amount ibc.WalletAmount, note string) (string, error) {
	// Solana doesn't have a standard memo/note field like Cosmos
	// This could be implemented with a memo instruction if needed
	err := c.SendFunds(ctx, keyName, amount)
	return "", err
}

func (c *SolanaChain) BuildWallet(ctx context.Context, keyName string, mnemonic string) (ibc.Wallet, error) {
	if mnemonic != "" {
		if err := c.RecoverKey(ctx, keyName, mnemonic); err != nil {
			return nil, err
		}
	} else {
		if err := c.CreateKey(ctx, keyName); err != nil {
			return nil, err
		}
	}

	wallet, exists := c.wallets[keyName]
	if !exists {
		return nil, fmt.Errorf("wallet %s not found after creation", keyName)
	}

	return NewWallet(keyName, wallet.PublicKey().Bytes()), nil
}

func (c *SolanaChain) BuildRelayerWallet(ctx context.Context, keyName string) (ibc.Wallet, error) {
	// For relayer, we create a new wallet and fund it
	if err := c.CreateKey(ctx, keyName); err != nil {
		return nil, err
	}

	wallet, exists := c.wallets[keyName]
	if !exists {
		return nil, fmt.Errorf("wallet %s not found after creation", keyName)
	}

	return NewWallet(keyName, wallet.PublicKey().Bytes()), nil
}

func (c *SolanaChain) ExportState(ctx context.Context, height int64) (string, error) {
	return "", fmt.Errorf("ExportState not implemented for Solana")
}

func (c *SolanaChain) GetGRPCAddress() string {
	return ""
}

func (c *SolanaChain) GetHostGRPCAddress() string {
	return ""
}

func (c *SolanaChain) GetHostPeerAddress() string {
	return ""
}

func (c *SolanaChain) GetGasFeesInNativeDenom(gasPaid int64) int64 {
	return gasPaid * 5000
}

func (c *SolanaChain) SendIBCTransfer(ctx context.Context, channelID, keyName string, amount ibc.WalletAmount, options ibc.TransferOptions) (ibc.Tx, error) {
	return ibc.Tx{}, fmt.Errorf("SendIBCTransfer not implemented for Solana")
}

func (c *SolanaChain) Acknowledgements(ctx context.Context, height int64) ([]ibc.PacketAcknowledgement, error) {
	return nil, fmt.Errorf("Acknowledgements not implemented for Solana")
}

func (c *SolanaChain) Timeouts(ctx context.Context, height int64) ([]ibc.PacketTimeout, error) {
	return nil, fmt.Errorf("Timeouts not implemented for Solana")
}

func (c *SolanaChain) NewTransactionFromInstructions(payerPubKey solana.PublicKey, instructions ...solana.Instruction) (*solana.Transaction, error) {
	recent, err := c.RPCClient.GetLatestBlockhash(context.TODO(), rpc.CommitmentFinalized)
	if err != nil {
		return nil, err
	}

	return solana.NewTransaction(
		instructions,
		recent.Value.Blockhash,
		solana.TransactionPayer(payerPubKey),
	)
}

func (c *SolanaChain) SignAndBroadcastTx(ctx context.Context, tx *solana.Transaction, signers ...*solana.Wallet) (solana.Signature, error) {
	_, err := c.SignTx(ctx, tx, signers...)
	if err != nil {
		return solana.Signature{}, err
	}

	return c.BroadcastTx(ctx, tx)
}

func (c *SolanaChain) SignTx(ctx context.Context, tx *solana.Transaction, signers ...*solana.Wallet) ([]solana.Signature, error) {
	if len(signers) == 0 {
		return nil, fmt.Errorf("no signers provided")
	}

	signerFn := func(key solana.PublicKey) *solana.PrivateKey {
		keyIdx := slices.IndexFunc(signers, func(signer *solana.Wallet) bool {
			return signer.PublicKey().Equals(key)
		})
		if keyIdx == -1 {
			panic(fmt.Sprintf("signer %s not found in provided signers", key))
		}
		return &signers[keyIdx].PrivateKey
	}

	return tx.Sign(signerFn)
}

func (c *SolanaChain) BroadcastTx(ctx context.Context, tx *solana.Transaction) (solana.Signature, error) {
	return confirm.SendAndConfirmTransaction(
		ctx,
		c.RPCClient,
		c.WSClient,
		tx,
	)
}

func confirmationStatusLevel(status rpc.ConfirmationStatusType) int {
	switch status {
	case rpc.ConfirmationStatusProcessed:
		return 1
	case rpc.ConfirmationStatusConfirmed:
		return 2
	case rpc.ConfirmationStatusFinalized:
		return 3
	default:
		return 0
	}
}

func (c *SolanaChain) WaitForTxStatus(txSig solana.Signature, status rpc.ConfirmationStatusType) error {
	return testutil.WaitForCondition(time.Second*30, time.Second, func() (bool, error) {
		out, err := c.RPCClient.GetSignatureStatuses(context.TODO(), false, txSig)
		if err != nil {
			return false, err
		}

		if out.Value[0].Err != nil {
			return false, fmt.Errorf("transaction %s failed with error: %s", txSig, out.Value[0].Err)
		}

		if confirmationStatusLevel(out.Value[0].ConfirmationStatus) >= confirmationStatusLevel(status) {
			return true, nil
		}

		return false, nil
	})
}

func (c *SolanaChain) FundUser(pubkey solana.PublicKey, amount uint64) (solana.Signature, error) {
	recent, err := c.RPCClient.GetLatestBlockhash(context.TODO(), rpc.CommitmentFinalized)
	if err != nil {
		return solana.Signature{}, err
	}

	tx, err := solana.NewTransaction(
		[]solana.Instruction{
			system.NewTransferInstruction(
				amount,
				c.Faucet.PublicKey(),
				pubkey,
			).Build(),
		},
		recent.Value.Blockhash,
		solana.TransactionPayer(c.Faucet.PublicKey()),
	)
	if err != nil {
		return solana.Signature{}, err
	}

	return c.SignAndBroadcastTxWithConfirmedStatus(context.TODO(), tx, c.Faucet)
}

func (c *SolanaChain) CreateAndFundWallet() (*solana.Wallet, error) {
	wallet := solana.NewWallet()
	if _, err := c.FundUser(wallet.PublicKey(), testvalues.InitialSolBalance); err != nil {
		return nil, err
	}
	return wallet, nil
}

// WaitForProgramAvailability waits for a program to become available with default timeout
func (c *SolanaChain) WaitForProgramAvailability(ctx context.Context, programID solana.PublicKey) bool {
	return c.WaitForProgramAvailabilityWithTimeout(ctx, programID, 30)
}

// WaitForProgramAvailabilityWithTimeout waits for a program to become available with specified timeout
func (c *SolanaChain) WaitForProgramAvailabilityWithTimeout(ctx context.Context, programID solana.PublicKey, timeoutSeconds int) bool {
	for range timeoutSeconds {
		accountInfo, err := c.RPCClient.GetAccountInfo(ctx, programID)
		if err == nil && accountInfo.Value != nil && accountInfo.Value.Executable {
			return true
		}
		time.Sleep(1 * time.Second)
	}
	return false
}

func (c *SolanaChain) SignAndBroadcastTxWithRetry(ctx context.Context, tx *solana.Transaction, signers ...*solana.Wallet) (solana.Signature, error) {
	return c.SignAndBroadcastTxWithRetryTimeout(ctx, tx, 30, signers...)
}

func (c *SolanaChain) SignAndBroadcastTxWithRetryTimeout(ctx context.Context, tx *solana.Transaction, timeoutSeconds int, signers ...*solana.Wallet) (solana.Signature, error) {
	var lastErr error
	for range timeoutSeconds {
		recent, err := c.RPCClient.GetLatestBlockhash(ctx, rpc.CommitmentFinalized)
		if err != nil {
			lastErr = fmt.Errorf("failed to get latest blockhash: %w", err)
			time.Sleep(1 * time.Second)
			continue
		}
		tx.Message.RecentBlockhash = recent.Value.Blockhash

		sig, err := c.SignAndBroadcastTx(ctx, tx, signers...)
		if err == nil {
			return sig, nil
		}
		lastErr = err
		time.Sleep(1 * time.Second)
	}
	return solana.Signature{}, fmt.Errorf("transaction broadcast timed out after %d seconds: %w", timeoutSeconds, lastErr)
}

func (c *SolanaChain) SignAndBroadcastTxWithConfirmedStatus(ctx context.Context, tx *solana.Transaction, wallet *solana.Wallet) (solana.Signature, error) {
	return c.SignAndBroadcastTxWithOpts(ctx, tx, wallet, rpc.ConfirmationStatusConfirmed)
}

func (c *SolanaChain) SignAndBroadcastTxWithOpts(ctx context.Context, tx *solana.Transaction, wallet *solana.Wallet, status rpc.ConfirmationStatusType) (solana.Signature, error) {
	_, err := c.SignTx(ctx, tx, wallet)
	if err != nil {
		return solana.Signature{}, err
	}

	sig, err := c.RPCClient.SendTransactionWithOpts(
		ctx,
		tx,
		rpc.TransactionOpts{
			SkipPreflight: true,
		},
	)
	if err != nil {
		return solana.Signature{}, err
	}

	err = c.WaitForTxStatus(sig, status)
	if err != nil {
		return solana.Signature{}, err
	}

	return sig, err
}

// WaitForBalanceChange waits for an account balance to change from the initial value
func (c *SolanaChain) WaitForBalanceChange(ctx context.Context, account solana.PublicKey, initialBalance uint64) (uint64, bool) {
	return c.WaitForBalanceChangeWithTimeout(ctx, account, initialBalance, 30)
}

// WaitForBalanceChangeWithTimeout waits for an account balance to change with specified timeout
func (c *SolanaChain) WaitForBalanceChangeWithTimeout(ctx context.Context, account solana.PublicKey, initialBalance uint64, timeoutSeconds int) (uint64, bool) {
	for range timeoutSeconds {
		balanceResp, err := c.RPCClient.GetBalance(ctx, account, rpc.CommitmentConfirmed)
		if err == nil {
			currentBalance := balanceResp.Value
			if currentBalance != initialBalance {
				return currentBalance, true
			}
		}
		time.Sleep(1 * time.Second)
	}
	return initialBalance, false
}

// CreateAddressLookupTable creates an Address Lookup Table and extends it with the given accounts.
// Returns the ALT address. Requires at least one account.
func (c *SolanaChain) CreateAddressLookupTable(ctx context.Context, authority *solana.Wallet, accounts []solana.PublicKey) (solana.PublicKey, error) {
	if len(accounts) == 0 {
		return solana.PublicKey{}, fmt.Errorf("at least one account is required for ALT")
	}

	// Get recent slot for ALT creation
	slot, err := c.RPCClient.GetSlot(ctx, "confirmed")
	if err != nil {
		return solana.PublicKey{}, fmt.Errorf("failed to get slot: %w", err)
	}

	// Derive ALT address with bump seed
	// The derivation uses: [authority, recent_slot] seeds
	altAddress, bumpSeed, err := solana.FindProgramAddress(
		[][]byte{authority.PublicKey().Bytes(), Uint64ToLeBytes(slot)},
		solana.AddressLookupTableProgramID,
	)
	if err != nil {
		return solana.PublicKey{}, fmt.Errorf("failed to derive ALT address: %w", err)
	}

	// Create ALT instruction data
	// ProgramInstruction enum: CreateLookupTable { recent_slot: u64, bump_seed: u8 }
	var createBuf bytes.Buffer
	encoder := bin.NewBinEncoder(&createBuf)
	mustWrite(encoder.WriteUint32(0, bin.LE))
	mustWrite(encoder.WriteUint64(slot, bin.LE))
	mustWrite(encoder.WriteUint8(bumpSeed))
	createInstructionData := createBuf.Bytes()

	createAltIx := solana.NewInstruction(
		solana.AddressLookupTableProgramID,
		solana.AccountMetaSlice{
			solana.Meta(altAddress).WRITE(),                     // lookup_table (to be created)
			solana.Meta(authority.PublicKey()).WRITE().SIGNER(), // authority
			solana.Meta(authority.PublicKey()).WRITE().SIGNER(), // payer
			solana.Meta(solana.SystemProgramID),                 // system_program
		},
		createInstructionData,
	)

	// Create ALT
	createTx, err := c.NewTransactionFromInstructions(authority.PublicKey(), createAltIx)
	if err != nil {
		return solana.PublicKey{}, fmt.Errorf("failed to create ALT transaction: %w", err)
	}

	_, err = c.SignAndBroadcastTxWithRetry(ctx, createTx, authority)
	if err != nil {
		return solana.PublicKey{}, fmt.Errorf("failed to create ALT: %w", err)
	}

	// Extend ALT with accounts instruction data
	// ProgramInstruction::ExtendLookupTable { new_addresses: Vec<Pubkey> }
	var extendBuf bytes.Buffer
	extendEncoder := bin.NewBinEncoder(&extendBuf)
	mustWrite(extendEncoder.WriteUint32(2, bin.LE))
	mustWrite(extendEncoder.WriteUint64(uint64(len(accounts)), bin.LE))
	for _, acc := range accounts {
		mustWrite(extendEncoder.WriteBytes(acc.Bytes(), false))
	}
	extendInstructionData := extendBuf.Bytes()

	extendAltIx := solana.NewInstruction(
		solana.AddressLookupTableProgramID,
		solana.AccountMetaSlice{
			solana.Meta(altAddress).WRITE(),                     // lookup_table
			solana.Meta(authority.PublicKey()).WRITE().SIGNER(), // authority
			solana.Meta(authority.PublicKey()).WRITE().SIGNER(), // payer (for reallocation)
			solana.Meta(solana.SystemProgramID),                 // system_program
		},
		extendInstructionData,
	)

	extendTx, err := c.NewTransactionFromInstructions(authority.PublicKey(), extendAltIx)
	if err != nil {
		return solana.PublicKey{}, fmt.Errorf("failed to create extend ALT transaction: %w", err)
	}

	_, err = c.SignAndBroadcastTxWithRetry(ctx, extendTx, authority)
	if err != nil {
		return solana.PublicKey{}, fmt.Errorf("failed to extend ALT: %w", err)
	}

	return altAddress, nil
}

// WaitForClusterReady waits for the Solana cluster to be fully initialized
func (c *SolanaChain) WaitForClusterReady(ctx context.Context, timeout time.Duration) error {
	deadline := time.Now().Add(timeout)

	for time.Now().Before(deadline) {
		// Check 1: Can we get the latest blockhash?
		_, err := c.RPCClient.GetLatestBlockhash(ctx, rpc.CommitmentFinalized)
		if err != nil {
			time.Sleep(1 * time.Second)
			continue
		}

		// Check 2: Can we get the slot?
		slot, err := c.RPCClient.GetSlot(ctx, rpc.CommitmentFinalized)
		if err != nil || slot < 5 {
			time.Sleep(1 * time.Second)
			continue
		}

		// Check 3: Is the faucet account funded and available?
		if c.Faucet != nil {
			balance, err := c.RPCClient.GetBalance(ctx, c.Faucet.PublicKey(), rpc.CommitmentFinalized)
			if err != nil {
				time.Sleep(1 * time.Second)
				continue
			}

			// Ensure faucet has at least 10 SOL for funding operations
			minBalance := uint64(10_000_000_000) // 1000 SOL in lamports
			if balance.Value < minBalance {
				return fmt.Errorf("faucet balance too low: %d lamports (need at least %d). Re-create solana validator node", balance.Value, minBalance)
			}
		}

		// Check 4: Can we get the version? (ensures RPC is fully responsive)
		_, err = c.RPCClient.GetVersion(ctx)
		if err != nil {
			time.Sleep(1 * time.Second)
			continue
		}

		// All checks passed
		return nil
	}

	return fmt.Errorf("cluster not ready after %v", timeout)
}

// CreateAndFundWalletWithRetry creates a wallet with retry logic
func (c *SolanaChain) CreateAndFundWalletWithRetry(ctx context.Context, retries int) (*solana.Wallet, error) {
	var lastErr error

	for i := range retries {
		if i > 0 {
			time.Sleep(time.Duration(i) * time.Second)
		}

		wallet := solana.NewWallet()

		_, err := c.FundUserWithRetry(ctx, wallet.PublicKey(), testvalues.InitialSolBalance, 3)
		if err == nil {
			balance, err := c.RPCClient.GetBalance(ctx, wallet.PublicKey(), rpc.CommitmentConfirmed)
			if err == nil && balance.Value > 0 {
				return wallet, nil
			}
		}

		lastErr = err
	}

	return nil, fmt.Errorf("failed to create and fund wallet after %d retries: %w", retries, lastErr)
}

func (c *SolanaChain) FundUserWithRetry(ctx context.Context, pubkey solana.PublicKey, amount uint64, retries int) (solana.Signature, error) {
	var lastErr error

	for i := range retries {
		if i > 0 {
			time.Sleep(time.Duration(i) * time.Second)
		}

		faucetBalance, err := c.RPCClient.GetBalance(ctx, c.Faucet.PublicKey(), rpc.CommitmentConfirmed)
		if err != nil {
			lastErr = fmt.Errorf("failed to get faucet balance: %w", err)
			continue
		}

		if faucetBalance.Value < amount {
			lastErr = fmt.Errorf("insufficient faucet balance: %d < %d", faucetBalance.Value, amount)
			continue
		}

		recent, err := c.RPCClient.GetLatestBlockhash(ctx, rpc.CommitmentFinalized)
		if err != nil {
			lastErr = fmt.Errorf("failed to get blockhash: %w", err)
			continue
		}

		tx, err := solana.NewTransaction(
			[]solana.Instruction{
				system.NewTransferInstruction(
					amount,
					c.Faucet.PublicKey(),
					pubkey,
				).Build(),
			},
			recent.Value.Blockhash,
			solana.TransactionPayer(c.Faucet.PublicKey()),
		)
		if err != nil {
			lastErr = fmt.Errorf("failed to create transaction: %w", err)
			continue
		}

		sig, err := c.SignAndBroadcastTx(ctx, tx, c.Faucet)
		if err == nil {
			time.Sleep(2 * time.Second)

			balance, err := c.RPCClient.GetBalance(ctx, pubkey, rpc.CommitmentConfirmed)
			if err == nil && balance.Value >= amount {
				return sig, nil
			}
		}

		lastErr = err
	}

	return solana.Signature{}, fmt.Errorf("failed to fund user after %d retries: %w", retries, lastErr)
}

func ComputeBudgetProgramID() solana.PublicKey {
	return solana.MustPublicKeyFromBase58("ComputeBudget111111111111111111111111111111")
}

func NewComputeBudgetInstruction(computeUnits uint32) solana.Instruction {
	data := make([]byte, 5)
	data[0] = 0x02
	binary.LittleEndian.PutUint32(data[1:], computeUnits)

	return solana.NewInstruction(
		ComputeBudgetProgramID(),
		solana.AccountMetaSlice{},
		data,
	)
}

func Uint64ToLeBytes(n uint64) []byte {
	b := make([]byte, 8)
	binary.LittleEndian.PutUint64(b, n)
	return b
}

func mustWrite(err error) {
	if err != nil {
		panic(fmt.Sprintf("unexpected encoding error: %v", err))
	}
}
