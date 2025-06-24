package chainconfig

import (
	"context"
	"os"
	"os/exec"

	"github.com/gagliardetto/solana-go/rpc"

	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/testvalues"
)

// TODO: The agave docker image currently does not work on Apple Silicon.
// Once it does, we can migrate to using the agave docker image instead of local agave instance.
// <https://github.com/anza-xyz/agave/issues/2627>

var (
	SolanaConfig = solanaConfig{
		FaucetSolBalance: testvalues.FaucetSolBalance,
	}
)

type solanaConfig struct {
	FaucetSolBalance int64
}

type SolanaLocalnetChain struct {
	OsProcess *os.Process
	RPCClient *rpc.Client
}

func binaryPath() string {
	return "solana-test-validator"
}

func StartLocalnet(context.Context) (SolanaLocalnetChain, error) {
	cmd := exec.Command(binaryPath(), "--reset")
	if err := cmd.Start(); err != nil {
		return SolanaLocalnetChain{}, err
	}

	return SolanaLocalnetChain{
		OsProcess: cmd.Process,
		RPCClient: rpc.New(rpc.LocalNet.RPC),
	}, nil
}

func (s SolanaLocalnetChain) Destroy() error {
	if s.OsProcess != nil {
		if err := s.OsProcess.Kill(); err != nil {
			return err
		}

		if err := os.RemoveAll(testvalues.SolanaLedgerDir); err != nil {
			return err
		}
	}

	return nil
}
