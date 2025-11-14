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

					recent, err := s.RPCClient.GetLatestBlockhash(ctx, rpc.CommitmentConfirmed)
					if err != nil {
						chunkResults <- chunkResult{
							chunkIdx: chkIdx,
							err:      fmt.Errorf("failed to get blockhash for chunk %d: %w", chkIdx, err),
							duration: time.Since(chunkStart),
						}
						return
					}
					tx.Message.RecentBlockhash = recent.Value.Blockhash

					sig, err := s.SignAndBroadcastTxWithOpts(ctx, tx, rpc.ConfirmationStatusConfirmed, user)
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

			recent, err := s.RPCClient.GetLatestBlockhash(ctx, rpc.CommitmentConfirmed)
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

			// Use confirmed commitment - relayer and verification both read with confirmed commitment
			sig, err := s.SignAndBroadcastTxWithOpts(ctx, finalTx, rpc.ConfirmationStatusConfirmed, user)
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

			t.Logf("✓ Packet %d: Final tx completed and finalized in %v - tx: %s", pktIdx+1, finalDuration, sig)
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

// DeploySolanaProgramAsync deploys a program using solana CLI
func (s *Solana) DeploySolanaProgramAsync(ctx context.Context, programName, keypairPath, payerKeypairPath string) (solana.PublicKey, error) {
	programSoFile := fmt.Sprintf("programs/solana/target/deploy/%s.so", programName)
	programID, _, err := DeploySolanaProgram(ctx, programSoFile, keypairPath, payerKeypairPath, s.RPCURL)
	if err != nil {
		return solana.PublicKey{}, fmt.Errorf("%s program deployment has failed: %w", programName, err)
	}

	if !s.WaitForProgramAvailability(ctx, programID) {
		return solana.PublicKey{}, fmt.Errorf("program %s failed to become available", programName)
	}

	return programID, nil
}

func (s *Solana) SubmitChunkedUpdateClient(ctx context.Context, t *testing.T, require *require.Assertions, resp *relayertypes.UpdateClientResponse, user *solana.Wallet) {
	t.Helper()

	var batch relayertypes.TransactionBatch
	err := proto.Unmarshal(resp.Tx, &batch)
	require.NoError(err, "Failed to unmarshal TransactionBatch")
	require.NotEmpty(batch.Txs, "no chunked transactions provided")

	totalStart := time.Now()

	// New transaction order: alt_create, alt_extend_batches..., SEPARATOR (empty), chunks..., assembly
	// Layout:
	// - batch.Txs[0]: alt_create
	// - batch.Txs[1..N]: alt_extend_txs (N batches)
	// - batch.Txs[N+1]: empty separator (len == 0)
	// - batch.Txs[N+2..M]: chunks
	// - batch.Txs[M]: assembly

	t.Logf("=== Starting Chunked Update Client ===")
	t.Logf("Total transactions: %d", len(batch.Txs))

	// Find the separator (empty transaction) to determine where ALT extensions end
	separatorIdx := -1
	for i := 1; i < len(batch.Txs); i++ {
		if len(batch.Txs[i]) == 0 {
			separatorIdx = i
			break
		}
	}
	require.NotEqual(-1, separatorIdx, "Failed to find separator transaction")

	altExtendCount := separatorIdx - 1 // Subtract 1 for alt_create at index 0

	// Phase 1: Submit ALT creation and extension transactions
	t.Logf("--- Phase 1: Creating and extending ALT for assembly transaction ---")
	altPhaseStart := time.Now()

	// Submit ALT creation transaction (always index 0)
	t.Logf("Submitting ALT creation transaction...")
	altCreateTx, err := solana.TransactionFromDecoder(bin.NewBinDecoder(batch.Txs[0]))
	require.NoError(err, "Failed to decode ALT creation tx")

	altCreateSig, err := s.SignAndBroadcastTxWithOpts(ctx, altCreateTx, rpc.ConfirmationStatusConfirmed, user)
	require.NoError(err, "Failed to submit ALT creation tx")
	t.Logf("✓ ALT creation tx submitted: %s", altCreateSig)

	// Submit ALT extension transactions sequentially
	t.Logf("Submitting %d ALT extension transactions...", altExtendCount)
	for i := range altExtendCount {
		extendIdx := 1 + i
		altExtendTx, err := solana.TransactionFromDecoder(bin.NewBinDecoder(batch.Txs[extendIdx]))
		require.NoError(err, "Failed to decode ALT extension tx %d", i+1)

		altExtendSig, err := s.SignAndBroadcastTxWithOpts(ctx, altExtendTx, rpc.ConfirmationStatusConfirmed, user)
		require.NoError(err, "Failed to submit ALT extension tx %d", i+1)
		t.Logf("✓ ALT extension tx %d/%d submitted: %s", i+1, altExtendCount, altExtendSig)
	}

	// Wait for ALT to activate (requires at least 1 slot)
	t.Logf("Waiting for ALT to activate (next slot)...")
	currentSlot, err := s.RPCClient.GetSlot(ctx, rpc.CommitmentConfirmed)
	require.NoError(err, "Failed to get current slot")

	targetSlot := currentSlot + 1
	for {
		slot, err := s.RPCClient.GetSlot(ctx, rpc.CommitmentConfirmed)
		require.NoError(err, "Failed to poll slot")
		if slot >= targetSlot {
			t.Logf("✓ ALT activated at slot %d (waited for slot %d)", slot, targetSlot)
			break
		}
		time.Sleep(100 * time.Millisecond)
	}

	altPhaseDuration := time.Since(altPhaseStart)
	t.Logf("--- Phase 1 Complete: ALT ready in %v ---", altPhaseDuration)

	// Phase 2: Upload chunks in parallel
	// Skip separator at separatorIdx
	chunkStart := separatorIdx + 1
	chunkEnd := len(batch.Txs) - 1 // Last tx is assembly
	chunkCount := chunkEnd - chunkStart

	type chunkResult struct {
		index        int
		sig          solana.Signature
		err          error
		duration     time.Duration
		computeUnits uint64
		fee          uint64
	}

	t.Logf("--- Phase 2: Uploading %d chunks in parallel ---", chunkCount)
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

			sig, err := s.SignAndBroadcastTxWithOpts(ctx, tx, rpc.ConfirmationStatusConfirmed, user)
			chunkDuration := time.Since(chunkTxStart)

			if err != nil {
				chunkResults <- chunkResult{
					index:    idx,
					err:      fmt.Errorf("failed to submit chunk %d: %w", idx, err),
					duration: chunkDuration,
				}
				return
			}

			// Fetch transaction details for gas tracking
			var computeUnits, fee uint64
			version := uint64(0)
			txDetails, err := s.RPCClient.GetTransaction(ctx, sig, &rpc.GetTransactionOpts{
				Commitment:                     rpc.CommitmentConfirmed,
				MaxSupportedTransactionVersion: &version,
			})
			if err == nil && txDetails != nil && txDetails.Meta != nil {
				if txDetails.Meta.ComputeUnitsConsumed != nil {
					computeUnits = *txDetails.Meta.ComputeUnitsConsumed
				}
				fee = txDetails.Meta.Fee
			}

			t.Logf("[Chunk %d timing] total duration: %v, compute units: %d, fee: %d lamports",
				idx, chunkDuration, computeUnits, fee)

			chunkResults <- chunkResult{
				index:        idx,
				sig:          sig,
				duration:     chunkDuration,
				computeUnits: computeUnits,
				fee:          fee,
			}
		}(i)
	}

	completedChunks := 0
	var totalChunkComputeUnits, totalChunkFees uint64
	for i := 0; i < chunkEnd-chunkStart; i++ {
		result := <-chunkResults
		require.NoError(result.err, "Chunk was not submitted")
		completedChunks++
		totalChunkComputeUnits += result.computeUnits
		totalChunkFees += result.fee
		t.Logf("✓ Chunk %d/%d uploaded in %v - tx: %s (gas: %d CUs, fee: %.9f SOL)",
			completedChunks, chunkCount, result.duration, result.sig,
			result.computeUnits, float64(result.fee)/1e9)
	}
	close(chunkResults)

	chunksTotal := time.Since(chunksStart)
	avgChunkTime := chunksTotal / time.Duration(chunkCount)
	avgChunkComputeUnits := totalChunkComputeUnits / uint64(chunkCount)
	t.Logf("--- Phase 2 Complete: All %d chunks uploaded in %v ---",
		chunkCount, chunksTotal)
	t.Logf("  Average per chunk: %v duration, %d CUs, %.9f SOL",
		avgChunkTime, avgChunkComputeUnits, float64(totalChunkFees)/float64(chunkCount)/1e9)
	t.Logf("  Total chunk gas: %d CUs, %.9f SOL",
		totalChunkComputeUnits, float64(totalChunkFees)/1e9)

	t.Logf("--- Phase 3: Assembling and updating client ---")
	assemblyStart := time.Now()

	tx, err := solana.TransactionFromDecoder(bin.NewBinDecoder(batch.Txs[len(batch.Txs)-1]))
	require.NoError(err, "Failed to decode assembly tx")

	sig, err := s.SignAndBroadcastTxWithOpts(ctx, tx, rpc.ConfirmationStatusConfirmed, user)
	if err != nil {
		t.Logf("Assembly transaction failed, fetching detailed logs...")
		// Try to get the signature from the error to fetch logs
		// Even if submission failed, the transaction may have been sent
		if sig.IsZero() {
			// If we don't have a signature, we can't fetch logs
			t.Logf("No transaction signature available to fetch logs")
		} else {
			// Wait a moment for transaction to be processed
			time.Sleep(500 * time.Millisecond)
			s.LogTransactionDetails(ctx, t, sig, "FAILED Assembly Transaction")
		}
		require.NoError(err, "Assembly transaction failed")
	}

	// Get transaction details to verify UpdateResult and track gas
	var assemblyComputeUnits, assemblyFee uint64
	version := uint64(0)
	txDetails, err := s.RPCClient.GetTransaction(ctx, sig, &rpc.GetTransactionOpts{
		Encoding:                       solana.EncodingBase64,
		Commitment:                     rpc.CommitmentConfirmed,
		MaxSupportedTransactionVersion: &version,
	})
	if err == nil && txDetails != nil && txDetails.Meta != nil {
		// Get gas metrics
		if txDetails.Meta.ComputeUnitsConsumed != nil {
			assemblyComputeUnits = *txDetails.Meta.ComputeUnitsConsumed
		}
		assemblyFee = txDetails.Meta.Fee

		// Check if transaction has return data (UpdateResult)
		returnDataBytes := txDetails.Meta.ReturnData.Data.Content
		if len(returnDataBytes) > 0 {
			// UpdateResult enum: 0=Update, 1=NoOp, 2=Misbehaviour
			// The return data should be the serialized UpdateResult
			t.Logf("✓ Update client returned data: %v (length: %d)", returnDataBytes, len(returnDataBytes))
			// First byte should be 0 for UpdateResult::Update
			if len(returnDataBytes) >= 1 {
				updateResult := returnDataBytes[0]
				switch updateResult {
				case 0:
					t.Logf("✓ UpdateResult: Update (client state updated)")
				case 1:
					t.Logf("✓ UpdateResult: NoOp (consensus state already exists)")
				case 2:
					t.Logf("✗ UpdateResult: Misbehaviour (client frozen)")
					require.NotEqual(2, updateResult, "Unexpected misbehaviour detected")
				default:
					t.Logf("? UpdateResult: Unknown value %d", updateResult)
				}
			}
		}
	}

	assemblyDuration := time.Since(assemblyStart)
	t.Logf("✓ Assembly transaction completed in %v - tx: %s (gas: %d CUs, fee: %.9f SOL)",
		assemblyDuration, sig, assemblyComputeUnits, float64(assemblyFee)/1e9)

	// Log detailed transaction information for debugging
	s.LogTransactionDetails(ctx, t, sig, "SUCCESS: Assembly Transaction")

	totalDuration := time.Since(totalStart)
	totalComputeUnits := totalChunkComputeUnits + assemblyComputeUnits
	totalFees := totalChunkFees + assemblyFee

	t.Logf("=== Chunked Update Client Complete ===")
	t.Logf("Total time: %v", totalDuration)
	t.Logf("  - ALT setup phase: %v", altPhaseDuration)
	t.Logf("  - Chunk upload phase: %v (%d chunks in parallel)", chunksTotal, chunkCount)
	t.Logf("  - Assembly phase: %v", assemblyDuration)
	t.Logf("Total gas consumption:")
	t.Logf("  - Chunks: %d CUs, %.9f SOL", totalChunkComputeUnits, float64(totalChunkFees)/1e9)
	t.Logf("  - Assembly: %d CUs, %.9f SOL", assemblyComputeUnits, float64(assemblyFee)/1e9)
	t.Logf("  - TOTAL: %d CUs, %.9f SOL", totalComputeUnits, float64(totalFees)/1e9)
}

func (s *Solana) VerifyPacketCommitmentDeleted(ctx context.Context, t *testing.T, require *require.Assertions, clientID string, sequence uint64) {
	t.Helper()
	sequenceBytes := make([]byte, 8)
	binary.LittleEndian.PutUint64(sequenceBytes, sequence)
	packetCommitmentPDA, _ := Ics26Router.PacketCommitmentPDA(ics26_router.ProgramID, []byte(clientID), sequenceBytes)

	// Use confirmed commitment to match relayer read commitment level
	accountInfo, err := s.RPCClient.GetAccountInfoWithOpts(ctx, packetCommitmentPDA, &rpc.GetAccountInfoOpts{
		Commitment: rpc.CommitmentConfirmed,
	})
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
