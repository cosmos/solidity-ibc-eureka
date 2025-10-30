package solana

import (
	"context"
	"encoding/binary"
	"fmt"
	"testing"
	"time"

	bin "github.com/gagliardetto/binary"
	"github.com/stretchr/testify/require"
	"google.golang.org/protobuf/proto"

	"github.com/gagliardetto/solana-go"
	"github.com/gagliardetto/solana-go/rpc"

	ics26_router "github.com/cosmos/solidity-ibc-eureka/packages/go-anchor/ics26router"

	relayertypes "github.com/srdtrk/solidity-ibc-eureka/e2e/v8/types/relayer"
)

func (s *Solana) SubmitChunkedRelayPackets(
	ctx context.Context,
	t *testing.T,
	resp *relayertypes.RelayByTxResponse,
	user *solana.Wallet,
) (solana.Signature, error) {
	t.Helper()

	var batch relayertypes.RelayPacketBatch
	err := proto.Unmarshal(resp.Tx, &batch)
	if err != nil {
		return solana.Signature{}, fmt.Errorf("failed to unmarshal RelayPacketBatch: %w", err)
	}
	if len(batch.Packets) == 0 {
		return solana.Signature{}, fmt.Errorf("no relay packets provided")
	}

	totalStart := time.Now()
	t.Logf("=== Starting Chunked Relay Packets ===")
	t.Logf("Total packets: %d", len(batch.Packets))

	totalChunks := 0
	for _, packet := range batch.Packets {
		totalChunks += len(packet.Chunks)
	}
	t.Logf("Total chunks across all packets: %d", totalChunks)

	type packetResult struct {
		packetIdx      int
		finalSig       solana.Signature
		err            error
		chunksDuration time.Duration
		finalDuration  time.Duration
		totalDuration  time.Duration
	}

	// Process all packets in parallel
	packetResults := make(chan packetResult, len(batch.Packets))

	for packetIdx, packet := range batch.Packets {
		go func(pktIdx int, pkt *relayertypes.PacketTransactions) {
			packetStart := time.Now()
			t.Logf("--- Packet %d: Starting (%d chunks + 1 final tx) ---", pktIdx+1, len(pkt.Chunks))

			type chunkResult struct {
				chunkIdx int
				sig      solana.Signature
				err      error
				duration time.Duration
			}

			// Phase 1: Submit all chunks for this packet in parallel
			chunksStart := time.Now()
			chunkResults := make(chan chunkResult, len(pkt.Chunks))

			for chunkIdx, chunkBytes := range pkt.Chunks {
				go func(chkIdx int, chunkData []byte) {
					chunkStart := time.Now()

					tx, err := solana.TransactionFromDecoder(bin.NewBinDecoder(chunkData))
					if err != nil {
						chunkResults <- chunkResult{
							chunkIdx: chkIdx,
							err:      fmt.Errorf("failed to decode chunk %d: %w", chkIdx, err),
							duration: time.Since(chunkStart),
						}
						return
					}

					recent, err := s.RPCClient.GetLatestBlockhash(ctx, rpc.CommitmentFinalized)
					if err != nil {
						chunkResults <- chunkResult{
							chunkIdx: chkIdx,
							err:      fmt.Errorf("failed to get blockhash for chunk %d: %w", chkIdx, err),
							duration: time.Since(chunkStart),
						}
						return
					}
					tx.Message.RecentBlockhash = recent.Value.Blockhash

					sig, err := s.SignAndBroadcastTx(ctx, tx, user)
					chunkDuration := time.Since(chunkStart)

					if err != nil {
						chunkResults <- chunkResult{
							chunkIdx: chkIdx,
							err:      fmt.Errorf("failed to submit chunk %d: %w", chkIdx, err),
							duration: chunkDuration,
						}
						return
					}

					chunkResults <- chunkResult{
						chunkIdx: chkIdx,
						sig:      sig,
						duration: chunkDuration,
					}
				}(chunkIdx, chunkBytes)
			}

			// Collect all chunk results for this packet
			var chunkErr error
			for i := 0; i < len(pkt.Chunks); i++ {
				result := <-chunkResults
				if result.err != nil {
					chunkErr = result.err
					t.Logf("✗ Packet %d, Chunk %d failed: %v", pktIdx+1, result.chunkIdx+1, result.err)
				} else {
					t.Logf("✓ Packet %d, Chunk %d/%d completed in %v - tx: %s",
						pktIdx+1, result.chunkIdx+1, len(pkt.Chunks), result.duration, result.sig)
				}
			}
			close(chunkResults)
			chunksDuration := time.Since(chunksStart)

			if chunkErr != nil {
				packetResults <- packetResult{
					packetIdx:      pktIdx,
					err:            fmt.Errorf("packet %d chunk upload failed: %w", pktIdx, chunkErr),
					chunksDuration: chunksDuration,
					totalDuration:  time.Since(packetStart),
				}
				return
			}

			t.Logf("--- Packet %d: All %d chunks completed in %v, submitting final tx ---",
				pktIdx+1, len(pkt.Chunks), chunksDuration)

			// Phase 2: Submit final transaction for this packet
			finalStart := time.Now()

			finalTx, err := solana.TransactionFromDecoder(bin.NewBinDecoder(pkt.FinalTx))
			if err != nil {
				packetResults <- packetResult{
					packetIdx:      pktIdx,
					err:            fmt.Errorf("packet %d failed to decode final tx: %w", pktIdx, err),
					chunksDuration: chunksDuration,
					finalDuration:  time.Since(finalStart),
					totalDuration:  time.Since(packetStart),
				}
				return
			}

			recent, err := s.RPCClient.GetLatestBlockhash(ctx, rpc.CommitmentFinalized)
			if err != nil {
				packetResults <- packetResult{
					packetIdx:      pktIdx,
					err:            fmt.Errorf("packet %d failed to get blockhash for final tx: %w", pktIdx, err),
					chunksDuration: chunksDuration,
					finalDuration:  time.Since(finalStart),
					totalDuration:  time.Since(packetStart),
				}
				return
			}
			finalTx.Message.RecentBlockhash = recent.Value.Blockhash

			sig, err := s.SignAndBroadcastTx(ctx, finalTx, user)
			finalDuration := time.Since(finalStart)
			totalDuration := time.Since(packetStart)

			if err != nil {
				packetResults <- packetResult{
					packetIdx:      pktIdx,
					err:            fmt.Errorf("packet %d failed to submit final tx: %w", pktIdx, err),
					chunksDuration: chunksDuration,
					finalDuration:  finalDuration,
					totalDuration:  totalDuration,
				}
				return
			}

			t.Logf("✓ Packet %d: Final tx completed in %v - tx: %s", pktIdx+1, finalDuration, sig)
			t.Logf("--- Packet %d: Complete in %v (chunks: %v, final: %v) ---",
				pktIdx+1, totalDuration, chunksDuration, finalDuration)

			packetResults <- packetResult{
				packetIdx:      pktIdx,
				finalSig:       sig,
				chunksDuration: chunksDuration,
				finalDuration:  finalDuration,
				totalDuration:  totalDuration,
			}
		}(packetIdx, packet)
	}

	// Collect all packet results
	var lastSig solana.Signature
	var totalChunksDuration time.Duration
	var totalFinalsDuration time.Duration

	for i := 0; i < len(batch.Packets); i++ {
		result := <-packetResults
		if result.err != nil {
			close(packetResults)
			return solana.Signature{}, result.err
		}
		lastSig = result.finalSig
		totalChunksDuration += result.chunksDuration
		totalFinalsDuration += result.finalDuration
	}
	close(packetResults)

	totalDuration := time.Since(totalStart)
	avgChunksDuration := totalChunksDuration / time.Duration(len(batch.Packets))
	avgFinalsDuration := totalFinalsDuration / time.Duration(len(batch.Packets))

	t.Logf("=== Chunked Relay Packets Complete ===")
	t.Logf("Total wall time: %v for %d packets (%d total chunks)", totalDuration, len(batch.Packets), totalChunks)
	t.Logf("All packets processed in parallel:")
	t.Logf("  - Avg chunks phase per packet: %v", avgChunksDuration)
	t.Logf("  - Avg final tx per packet: %v", avgFinalsDuration)
	t.Logf("Parallelization: All packets + all chunks within each packet submitted concurrently")
	return lastSig, nil
}

func (s *Solana) DeploySolanaProgram(ctx context.Context, t *testing.T, require *require.Assertions, programName string) solana.PublicKey {
	t.Helper()
	keypairPath := fmt.Sprintf("solana-keypairs/localnet/%s-keypair.json", programName)
	walletPath := "solana-keypairs/localnet/deployer_wallet.json"
	programID, _, err := AnchorDeploy(ctx, "programs/solana", programName, keypairPath, walletPath)
	require.NoError(err, "%s program deployment has failed", programName)
	t.Logf("%s program deployed at: %s", programName, programID.String())

	// Wait for program to be available
	if !s.WaitForProgramAvailability(ctx, programID) {
		t.Logf("Warning: Program %s may not be fully available yet", programID.String())
	}

	return programID
}

func (s *Solana) SubmitChunkedUpdateClient(ctx context.Context, t *testing.T, require *require.Assertions, resp *relayertypes.UpdateClientResponse, user *solana.Wallet) {
	t.Helper()

	var batch relayertypes.TransactionBatch
	err := proto.Unmarshal(resp.Tx, &batch)
	require.NoError(err, "Failed to unmarshal TransactionBatch")
	require.NotEmpty(batch.Txs, "no chunked transactions provided")

	totalStart := time.Now()

	chunkCount := len(batch.Txs) - 1
	t.Logf("=== Starting Chunked Update Client ===")
	t.Logf("Total transactions: %d (%d chunks + 1 assembly)",
		len(batch.Txs),
		chunkCount)

	chunkStart := 0
	chunkEnd := len(batch.Txs) - 1

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

			tx, err := solana.TransactionFromDecoder(bin.NewBinDecoder(batch.Txs[idx]))
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

	tx, err := solana.TransactionFromDecoder(bin.NewBinDecoder(batch.Txs[len(batch.Txs)-1]))
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
	t.Helper()
	sequenceBytes := make([]byte, 8)
	binary.LittleEndian.PutUint64(sequenceBytes, sequence)
	packetCommitmentPDA, _ := Ics26Router.PacketCommitmentPDA(ics26_router.ProgramID, []byte(clientID), sequenceBytes)

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
	t.Helper()
	commonAccounts := s.CreateIBCAddressLookupTableAccounts(cosmosChainID, gmpPortID, clientID, user.PublicKey())

	altAddress, err := s.CreateAddressLookupTable(ctx, user, commonAccounts)
	require.NoError(err)
	t.Logf("Created and extended ALT %s with %d common accounts", altAddress, len(commonAccounts))

	return altAddress
}
