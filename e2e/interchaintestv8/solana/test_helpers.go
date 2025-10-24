package solana

import (
	"context"
	"fmt"
	"strings"
	"testing"
	"time"

	bin "github.com/gagliardetto/binary"
	"github.com/stretchr/testify/require"

	"github.com/gagliardetto/solana-go"
	"github.com/gagliardetto/solana-go/rpc"

	relayertypes "github.com/srdtrk/solidity-ibc-eureka/e2e/v8/types/relayer"
)

func (s *Solana) SubmitChunkedRelayPackets(
	ctx context.Context,
	t *testing.T,
	require *require.Assertions,
	resp *relayertypes.RelayByTxResponse,
	user *solana.Wallet,
) solana.Signature {
	t.Helper()
	require.NotEqual(0, len(resp.Txs), "no relay transactions provided")

	totalStart := time.Now()
	t.Logf("=== Starting Chunked Relay Packets ===")
	t.Logf("Total transactions: %d (chunks + final instructions)", len(resp.Txs))

	var lastSig solana.Signature
	for i, txBytes := range resp.Txs {
		txStart := time.Now()

		tx, err := solana.TransactionFromDecoder(bin.NewBinDecoder(txBytes))
		require.NoError(err, "Failed to decode transaction %d", i)

		recent, err := s.RPCClient.GetLatestBlockhash(ctx, rpc.CommitmentFinalized)
		require.NoError(err, "Failed to get latest blockhash for transaction %d", i)
		tx.Message.RecentBlockhash = recent.Value.Blockhash

		sig, err := s.SignAndBroadcastTx(ctx, tx, user)
		require.NoError(err, "Failed to submit transaction %d", i)

		lastSig = sig
		txDuration := time.Since(txStart)
		t.Logf("✓ Transaction %d/%d completed in %v - tx: %s",
			i+1, len(resp.Txs), txDuration, sig)
	}

	totalDuration := time.Since(totalStart)
	avgTxTime := totalDuration / time.Duration(len(resp.Txs))
	t.Logf("=== Chunked Relay Packets Complete ===")
	t.Logf("Total time: %v for %d transactions (avg: %v/tx)",
		totalDuration, len(resp.Txs), avgTxTime)
	t.Logf("NOTE: for simplicity all tx chunks are waiting for finalization and are sent sequentially")
	t.Logf("In real use only final packet tx (recv/ack/timeout) needs to be finalized")
	return lastSig
}

func (s *Solana) SubmitChunkedRelayPacketsExpectingError(
	ctx context.Context,
	t *testing.T,
	require *require.Assertions,
	resp *relayertypes.RelayByTxResponse,
	user *solana.Wallet,
	expectedErrorSubstring string,
) solana.Signature {
	t.Helper()
	require.NotEmpty(resp.Txs, "Expected relay transactions to submit")

	var lastSig solana.Signature
	var encounteredError error

	for i, txBytes := range resp.Txs {
		tx, err := solana.TransactionFromDecoder(bin.NewBinDecoder(txBytes))
		if err != nil {
			require.Fail("Failed to decode transaction", "Transaction %d decode error: %v", i, err)
			return lastSig
		}

		recent, err := s.RPCClient.GetLatestBlockhash(ctx, rpc.CommitmentFinalized)
		if err != nil {
			require.Fail("Failed to get latest blockhash", "Transaction %d blockhash error: %v", i, err)
			return lastSig
		}
		tx.Message.RecentBlockhash = recent.Value.Blockhash

		sig, err := s.SignAndBroadcastTx(ctx, tx, user)
		if err != nil {
			encounteredError = err
			lastSig = sig
			t.Logf("Transaction %d failed as expected: %v", i, err)
			break
		}

		lastSig = sig
		t.Logf("Transaction %d/%d succeeded: %s", i+1, len(resp.Txs), sig)
	}

	require.Error(encounteredError, "Expected transaction to fail but it succeeded")

	if expectedErrorSubstring != "" {
		errorMsg := strings.ToLower(encounteredError.Error())
		expectedLower := strings.ToLower(expectedErrorSubstring)
		require.Contains(errorMsg, expectedLower,
			"Error message should contain expected substring.\nExpected substring: %s\nActual error: %s",
			expectedErrorSubstring, encounteredError.Error())
		t.Logf("Error validation passed: contains '%s'", expectedErrorSubstring)
	}

	return lastSig
}

func (s *Solana) DeploySolanaProgram(ctx context.Context, t *testing.T, require *require.Assertions, programName string) solana.PublicKey {
	keypairPath := fmt.Sprintf("e2e/interchaintestv8/solana/keypairs/%s-keypair.json", programName)
	walletPath := "e2e/interchaintestv8/solana/keypairs/deployer_wallet.json"
	programID, _, err := AnchorDeploy(ctx, "programs/solana", programName, keypairPath, walletPath)
	require.NoError(err, "%s program deployment has failed", programName)
	t.Logf("%s program deployed at: %s", programName, programID.String())
	return programID
}

func (s *Solana) WaitForProgramAvailability(ctx context.Context, t *testing.T, programID solana.PublicKey) bool {
	return s.WaitForProgramAvailabilityWithTimeout(ctx, t, programID, 30)
}

func (s *Solana) WaitForProgramAvailabilityWithTimeout(ctx context.Context, t *testing.T, programID solana.PublicKey, timeoutSeconds int) bool {
	for i := range timeoutSeconds {
		accountInfo, err := s.RPCClient.GetAccountInfo(ctx, programID)
		if err == nil && accountInfo.Value != nil && accountInfo.Value.Executable {
			t.Logf("Program %s is available after %d seconds, owner: %s, executable: %v",
				programID.String(), i+1, accountInfo.Value.Owner.String(), accountInfo.Value.Executable)
			return true
		}
		if i == 0 {
			t.Logf("Waiting for program %s to be available...", programID.String())
		}
		time.Sleep(1 * time.Second)
	}

	t.Logf("Warning: Program %s still not available after %d seconds", programID.String(), timeoutSeconds)
	return false
}

func (s *Solana) SubmitChunkedUpdateClient(ctx context.Context, t *testing.T, require *require.Assertions, resp *relayertypes.UpdateClientResponse, user *solana.Wallet) {
	require.NotEqual(0, len(resp.Txs), "no chunked transactions provided")

	totalStart := time.Now()

	chunkCount := len(resp.Txs) - 1
	t.Logf("=== Starting Chunked Update Client ===")
	t.Logf("Total transactions: %d (%d chunks + 1 assembly)",
		len(resp.Txs),
		chunkCount)

	chunkStart := 0
	chunkEnd := len(resp.Txs) - 1

	type chunkResult struct {
		index    int
		sig      solana.Signature
		err      error
		duration time.Duration
	}

	t.Logf("--- Phase 1: Uploading %d chunks in parallel ---", chunkCount)
	chunksStart := time.Now()
	chunkResults := make(chan chunkResult, chunkEnd-chunkStart)

	for i := chunkStart; i < chunkEnd; i++ {
		go func(idx int) {
			chunkTxStart := time.Now()

			tx, err := solana.TransactionFromDecoder(bin.NewBinDecoder(resp.Txs[idx]))
			if err != nil {
				chunkResults <- chunkResult{
					index:    idx,
					err:      fmt.Errorf("failed to decode chunk %d: %w", idx, err),
					duration: time.Since(chunkTxStart),
				}
				return
			}

			sig, err := s.SignAndBroadcastTxWithOpts(ctx, tx, user, rpc.ConfirmationStatusProcessed)
			chunkDuration := time.Since(chunkTxStart)

			if err != nil {
				chunkResults <- chunkResult{
					index:    idx,
					err:      fmt.Errorf("failed to submit chunk %d: %w", idx, err),
					duration: chunkDuration,
				}
				return
			}

			t.Logf("[Chunk %d timing] total duration: %v",
				idx, chunkDuration)

			chunkResults <- chunkResult{
				index:    idx,
				sig:      sig,
				duration: chunkDuration,
			}
		}(i)
	}

	completedChunks := 0
	for i := 0; i < chunkEnd-chunkStart; i++ {
		result := <-chunkResults
		require.NoError(result.err, "Chunk was not submitted")
		completedChunks++
		t.Logf("✓ Chunk %d/%d uploaded in %v - tx: %s",
			completedChunks, chunkCount, result.duration, result.sig)
	}
	close(chunkResults)

	chunksTotal := time.Since(chunksStart)
	avgChunkTime := chunksTotal / time.Duration(chunkCount)
	t.Logf("--- Phase 1 Complete: All %d chunks uploaded in %v (avg: %v/chunk) ---",
		chunkCount, chunksTotal, avgChunkTime)

	t.Logf("--- Phase 2: Assembling and updating client ---")
	assemblyStart := time.Now()

	tx, err := solana.TransactionFromDecoder(bin.NewBinDecoder(resp.Txs[len(resp.Txs)-1]))
	require.NoError(err, "Failed to decode assembly tx")

	sig, err := s.SignAndBroadcastTxWithConfirmedStatus(ctx, tx, user)
	require.NoError(err)

	assemblyDuration := time.Since(assemblyStart)
	t.Logf("✓ Assembly transaction completed in %v - tx: %s", assemblyDuration, sig)

	totalDuration := time.Since(totalStart)
	t.Logf("=== Chunked Update Client Complete ===")
	t.Logf("Total time: %v", totalDuration)
	t.Logf("  - Chunk upload phase: %v (%d chunks in parallel)", chunksTotal, chunkCount)
	t.Logf("  - Assembly phase: %v", assemblyDuration)
}

func (s *Solana) VerifyPacketCommitmentDeleted(ctx context.Context, t *testing.T, require *require.Assertions, clientID string, sequence uint64) {
	packetCommitmentPDA, _ := RouterPacketCommitmentPDA(clientID, sequence)

	accountInfo, err := s.RPCClient.GetAccountInfo(ctx, packetCommitmentPDA)
	if err != nil {
		t.Logf("Packet commitment deleted (account not found) for client %s, sequence %d", clientID, sequence)
		return
	}

	if accountInfo.Value == nil || accountInfo.Value.Lamports == 0 {
		t.Logf("Packet commitment deleted (account closed) for client %s, sequence %d", clientID, sequence)
		return
	}

	require.Fail("Packet commitment should have been deleted after acknowledgment",
		"Account %s still exists with %d lamports", packetCommitmentPDA.String(), accountInfo.Value.Lamports)
}

func (s *Solana) CreateIBCAddressLookupTable(ctx context.Context, t *testing.T, require *require.Assertions, user *solana.Wallet, cosmosChainID string, gmpPortID string, clientID string) solana.PublicKey {
	commonAccounts := s.CreateIBCAddressLookupTableAccounts(cosmosChainID, gmpPortID, clientID, user.PublicKey())

	altAddress, err := s.CreateAddressLookupTable(ctx, user, commonAccounts)
	require.NoError(err)
	t.Logf("Created and extended ALT %s with %d common accounts", altAddress, len(commonAccounts))

	return altAddress
}
