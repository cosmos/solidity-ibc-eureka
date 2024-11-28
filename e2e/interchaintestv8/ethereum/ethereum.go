package ethereum

import (
	"bytes"
	"context"
	"crypto/ecdsa"
	"encoding/hex"
	"fmt"
	"io"
	"math/big"
	"os"
	"os/exec"
	"strconv"
	"strings"

	ethcommon "github.com/ethereum/go-ethereum/common"
	"github.com/ethereum/go-ethereum/crypto"
	"github.com/ethereum/go-ethereum/ethclient"

	"cosmossdk.io/math"

	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/testvalues"
)

type Ethereum struct {
	ChainID         *big.Int
	RPC             string
	EthAPI          EthAPI
	BeaconAPIClient *BeaconAPIClient

	Faucet *ecdsa.PrivateKey
}

func NewEthereum(ctx context.Context, rpc string, beaconAPIClient *BeaconAPIClient, faucet *ecdsa.PrivateKey) (Ethereum, error) {
	ethClient, err := ethclient.Dial(rpc)
	if err != nil {
		return Ethereum{}, err
	}
	chainID, err := ethClient.ChainID(ctx)
	if err != nil {
		return Ethereum{}, err
	}
	ethAPI, err := NewEthAPI(rpc)
	if err != nil {
		return Ethereum{}, err
	}

	return Ethereum{
		ChainID:         chainID,
		RPC:             rpc,
		EthAPI:          ethAPI,
		BeaconAPIClient: beaconAPIClient,
		Faucet:          faucet,
	}, nil
}

func (e Ethereum) ForgeScript(deployer *ecdsa.PrivateKey, solidityContract string, args ...string) ([]byte, error) {
	args = append(args, "script", "--rpc-url", e.RPC, "--private-key",
		hex.EncodeToString(deployer.D.Bytes()), "--broadcast",
		"--non-interactive", "-vvvv", solidityContract,
	)
	cmd := exec.Command(
		"forge", args...,
	)

	faucetAddress := crypto.PubkeyToAddress(e.Faucet.PublicKey)
	extraEnv := []string{
		fmt.Sprintf("%s=%s", testvalues.EnvKeyE2EFacuetAddress, faucetAddress.Hex()),
	}

	cmd.Env = os.Environ()
	cmd.Env = append(cmd.Env, extraEnv...)

	var stdoutBuf bytes.Buffer

	// Create a MultiWriter to write to both os.Stdout and the buffer
	multiWriter := io.MultiWriter(os.Stdout, &stdoutBuf)

	// Set the command's stdout to the MultiWriter
	cmd.Stdout = multiWriter
	cmd.Stderr = os.Stderr

	// Run the command
	if err := cmd.Run(); err != nil {
		fmt.Println("Error start command", cmd.Args, err)
		return nil, err
	}

	// Get the output as byte slices
	stdoutBytes := stdoutBuf.Bytes()

	return stdoutBytes, nil
}

func (e Ethereum) CreateAndFundUser() (*ecdsa.PrivateKey, error) {
	key, err := crypto.GenerateKey()
	if err != nil {
		return nil, err
	}

	address := crypto.PubkeyToAddress(key.PublicKey).Hex()
	if err := e.FundUser(address, testvalues.StartingEthBalance); err != nil {
		return nil, err
	}

	return key, nil
}

func (e Ethereum) FundUser(address string, amount math.Int) error {
	return e.SendEth(e.Faucet, address, amount)
}

func (e Ethereum) SendEth(key *ecdsa.PrivateKey, toAddress string, amount math.Int) error {
	cmd := exec.Command(
		"cast",
		"send",
		toAddress,
		"--value", amount.String(),
		"--private-key", fmt.Sprintf("0x%s", ethcommon.Bytes2Hex(key.D.Bytes())),
		"--rpc-url", e.RPC,
	)
	cmd.Stdout = os.Stdout
	cmd.Stderr = os.Stderr

	if err := cmd.Run(); err != nil {
		return fmt.Errorf("failed to send eth with %s: %w", strings.Join(cmd.Args, " "), err)
	}

	return nil
}

func (e *Ethereum) Height() (int64, error) {
	cmd := exec.Command("cast", "block-number", "--rpc-url", e.RPC)
	stdout, err := cmd.Output()
	if err != nil {
		return 0, err
	}
	return strconv.ParseInt(strings.TrimSpace(string(stdout)), 10, 64)
}
