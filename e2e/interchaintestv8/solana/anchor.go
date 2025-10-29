package solana

import (
	"bytes"
	"context"
	"fmt"
	"io"
	"os"
	"os/exec"
	"path/filepath"
	"regexp"

	"github.com/gagliardetto/solana-go"
)

// deployerAddress must match `deployer_wallet.json`
const deployerAddress = "8ntLtUdGwBaXfFPCrNis9MWsKMdEUYyonwuw7NQwhs5z"

// DeployerPubkey is the public key of the deployer wallet used for deploying all Solana programs.
var DeployerPubkey = solana.MustPublicKeyFromBase58(deployerAddress)

func DeploySolanaProgram(ctx context.Context, programSoFile, programKeypairFile, payerKeypairFile, rpcURL string) (solana.PublicKey, solana.Signature, error) {
	absProgramFile, err := filepath.Abs(programSoFile)
	if err != nil {
		return solana.PublicKey{}, solana.Signature{}, fmt.Errorf("failed to get absolute path for program file: %w", err)
	}

	absKeypairFile, err := filepath.Abs(programKeypairFile)
	if err != nil {
		return solana.PublicKey{}, solana.Signature{}, fmt.Errorf("failed to get absolute path for program keypair file: %w", err)
	}

	absPayerFile, err := filepath.Abs(payerKeypairFile)
	if err != nil {
		return solana.PublicKey{}, solana.Signature{}, fmt.Errorf("failed to get absolute path for payer keypair file: %w", err)
	}

	cmd := exec.Command(
		"solana", "program", "deploy",
		"--url", rpcURL, // Specify RPC URL to avoid using default Solana config
		"--program-id", absKeypairFile,
		"--keypair", absPayerFile,
		"--upgrade-authority", absPayerFile,
		"--use-rpc", // Use RPC transport instead of QUIC to avoid connection issues during parallel deployments
		absProgramFile,
	)

	cmd.Env = os.Environ()

	var stdoutBuf bytes.Buffer
	multiWriter := io.MultiWriter(os.Stdout, &stdoutBuf)

	cmd.Stdout = multiWriter
	cmd.Stderr = os.Stderr

	if err := cmd.Run(); err != nil {
		fmt.Println("Error deploy command", cmd.Args, err)
		return solana.PublicKey{}, solana.Signature{}, err
	}

	stdoutBytes := stdoutBuf.Bytes()

	return getProgramIDAndSignatureFromSolanaDeploy(stdoutBytes)
}

func AnchorDeploy(ctx context.Context, dir, programName, programKeypairFile, walletFile string, args ...string) (solana.PublicKey, solana.Signature, error) {
	absWalletFile, err := filepath.Abs(walletFile)
	if err != nil {
		return solana.PublicKey{}, solana.Signature{}, fmt.Errorf("failed to get absolute path for wallet file: %w", err)
	}

	absKeypairFile, err := filepath.Abs(programKeypairFile)
	if err != nil {
		return solana.PublicKey{}, solana.Signature{}, fmt.Errorf("failed to get absolute path for program keypair file: %w", err)
	}

	args = append(args, "deploy", "-p", programName, "--provider.wallet", absWalletFile, "--program-keypair", absKeypairFile)
	cmd := exec.Command(
		"anchor", args...,
	)

	cmd.Dir = dir
	cmd.Env = os.Environ()

	var stdoutBuf bytes.Buffer

	multiWriter := io.MultiWriter(os.Stdout, &stdoutBuf)

	cmd.Stdout = multiWriter
	cmd.Stderr = os.Stderr

	if err := cmd.Run(); err != nil {
		fmt.Println("Error deploy command", cmd.Args, err)
		return solana.PublicKey{}, solana.Signature{}, err
	}

	// Get the output as byte slices
	stdoutBytes := stdoutBuf.Bytes()

	return getProgramIDAndSignatureFromAnchorDeploy(stdoutBytes)
}

// Parses raw Solana CLI deploy output and extracts Program ID and Signature.
func getProgramIDAndSignatureFromSolanaDeploy(stdout []byte) (solana.PublicKey, solana.Signature, error) {
	outputStr := string(stdout)

	programIDRe := regexp.MustCompile(`(?m)Program Id:\s+([1-9A-HJ-NP-Za-km-z]{32,44})`)
	signatureRe := regexp.MustCompile(`(?m)Signature:\s+([1-9A-HJ-NP-Za-km-z]{32,88})`)

	programIDMatch := programIDRe.FindStringSubmatch(outputStr)
	signatureMatch := signatureRe.FindStringSubmatch(outputStr)

	if len(programIDMatch) < 2 {
		return solana.PublicKey{}, solana.Signature{}, fmt.Errorf("program id not found in output")
	}
	if len(signatureMatch) < 2 {
		return solana.PublicKey{}, solana.Signature{}, fmt.Errorf("signature not found in output")
	}

	programID, err := solana.PublicKeyFromBase58(programIDMatch[1])
	if err != nil {
		return solana.PublicKey{}, solana.Signature{}, fmt.Errorf("invalid Program Id: %w", err)
	}

	signature, err := solana.SignatureFromBase58(signatureMatch[1])
	if err != nil {
		return solana.PublicKey{}, solana.Signature{}, fmt.Errorf("invalid Signature: %w", err)
	}

	return programID, signature, nil
}

// Parses raw Anchor CLI deploy output and extracts Program ID and Signature.
func getProgramIDAndSignatureFromAnchorDeploy(stdout []byte) (solana.PublicKey, solana.Signature, error) {
	outputStr := string(stdout)

	programIDRe := regexp.MustCompile(`(?m)^Program Id:\s+([1-9A-HJ-NP-Za-km-z]{32,44})$`)
	signatureRe := regexp.MustCompile(`(?m)^Signature:\s+([1-9A-HJ-NP-Za-km-z]{32,88})$`)

	programIDMatch := programIDRe.FindStringSubmatch(outputStr)
	signatureMatch := signatureRe.FindStringSubmatch(outputStr)

	if len(programIDMatch) < 2 {
		return solana.PublicKey{}, solana.Signature{}, fmt.Errorf("program id not found")
	}
	if len(signatureMatch) < 2 {
		return solana.PublicKey{}, solana.Signature{}, fmt.Errorf("signature not found")
	}

	programID, err := solana.PublicKeyFromBase58(programIDMatch[1])
	if err != nil {
		return solana.PublicKey{}, solana.Signature{}, fmt.Errorf("invalid Program Id: %w", err)
	}

	signature, err := solana.SignatureFromBase58(signatureMatch[1])
	if err != nil {
		return solana.PublicKey{}, solana.Signature{}, fmt.Errorf("invalid Signature: %w", err)
	}

	return programID, signature, nil
}
