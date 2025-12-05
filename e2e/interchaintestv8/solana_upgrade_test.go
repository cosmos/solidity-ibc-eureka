package main

import (
	"context"
	"testing"

	"github.com/stretchr/testify/suite"

	solanago "github.com/gagliardetto/solana-go"
	"github.com/gagliardetto/solana-go/rpc"

	access_manager "github.com/cosmos/solidity-ibc-eureka/packages/go-anchor/accessmanager"
	ics26_router "github.com/cosmos/solidity-ibc-eureka/packages/go-anchor/ics26router"

	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/solana"
)

const (
	keypairDir    = "solana-keypairs/localnet"
	deployerPath  = keypairDir + "/deployer_wallet.json"
	programSoFile = "programs/solana/target/deploy/ics26_router.so"
)

// IbcEurekaSolanaUpgradeTestSuite tests program upgradability via AccessManager
type IbcEurekaSolanaUpgradeTestSuite struct {
	IbcEurekaSolanaTestSuite

	UpgraderWallet *solanago.Wallet
}

func TestWithIbcEurekaSolanaUpgradeTestSuite(t *testing.T) {
	s := &IbcEurekaSolanaUpgradeTestSuite{}
	suite.Run(t, s)
}

// Test_ProgramUpgrade_Via_AccessManager demonstrates the complete upgrade flow with role-based access control.
//
// BACKGROUND:
// Solana's BPF Loader Upgradeable uses a two-account system:
// - Program Account: Executable with fixed address (what users call)
// - ProgramData Account: Contains bytecode and upgrade authority metadata
//
// The upgrade authority controls who can upgrade the program. By setting it to an AccessManager-controlled
// PDA, we enable role-based upgrades where only accounts with UPGRADER_ROLE can upgrade.
//
// TEST FLOW:
// 1. Grant UPGRADER_ROLE to a test wallet
// 2. Derive required PDAs (program data account, upgrade authority PDA)
// 3. Transfer program upgrade authority from deployer to AccessManager's PDA (one-time setup)
// 4. Write new program bytecode to a buffer account
// 5. Transfer buffer authority to match program upgrade authority (security requirement)
// 6. Call AccessManager.upgrade_program() with UPGRADER_ROLE
//   - AccessManager checks role membership
//   - AccessManager calls BPFLoaderUpgradeable.upgrade via invoke_signed with PDA signature
//   - BPF Loader verifies authorities match and replaces bytecode
//
// 7. Verify unauthorized accounts cannot upgrade (negative test)
//
// SECURITY MODEL:
// - Role-based access: Only UPGRADER_ROLE can trigger upgrades (AccessManager enforcement)
// - Authority matching: Buffer authority must equal program upgrade authority (BPF Loader enforcement)
// - CPI protection: Upgrade cannot be called via CPI (instructions sysvar check)
// - PDA verification: Upgrade authority PDA seeds are validated (Anchor constraints)
func (s *IbcEurekaSolanaUpgradeTestSuite) Test_ProgramUpgrade_Via_AccessManager() {
	ctx := context.Background()

	s.UseMockWasmClient = true
	s.SetupSuite(ctx)

	s.Require().True(s.Run("Setup: Create upgrader wallet", func() {
		var err error
		s.UpgraderWallet, err = s.SolanaChain.CreateAndFundWallet()
		s.Require().NoError(err, "failed to create and fund upgrader wallet")
	}))

	s.Require().True(s.Run("Setup: Grant UPGRADER_ROLE to upgrader wallet", func() {
		accessControlAccount, _ := solana.AccessManager.AccessManagerPDA(access_manager.ProgramID)
		const UPGRADER_ROLE = uint64(8)

		grantUpgraderRoleInstruction, err := access_manager.NewGrantRoleInstruction(
			UPGRADER_ROLE,
			s.UpgraderWallet.PublicKey(),
			accessControlAccount,
			s.SolanaRelayer.PublicKey(),
			solanago.SysVarInstructionsPubkey,
		)
		s.Require().NoError(err, "failed to build grant UPGRADER_ROLE instruction")

		tx, err := s.SolanaChain.NewTransactionFromInstructions(
			s.SolanaRelayer.PublicKey(),
			grantUpgraderRoleInstruction,
		)
		s.Require().NoError(err, "failed to create grant role transaction")

		_, err = s.SolanaChain.SignAndBroadcastTxWithRetry(ctx, tx, rpc.CommitmentConfirmed, s.SolanaRelayer)
		s.Require().NoError(err, "failed to grant UPGRADER_ROLE")
	}))

	targetProgramID := ics26_router.ProgramID

	var programDataAccount solanago.PublicKey
	var upgradeAuthorityPDA solanago.PublicKey

	s.Require().True(s.Run("Derive upgrade authority and program data accounts", func() {
		var err error

		programDataAccount, err = solana.GetProgramDataAddress(targetProgramID)
		s.Require().NoError(err, "failed to derive program data address")

		upgradeAuthorityPDA, _ = solana.AccessManager.UpgradeAuthorityPDA(
			access_manager.ProgramID,
			targetProgramID.Bytes(),
		)
	}))

	s.Require().True(s.Run("Transfer program upgrade authority to AccessManager", func() {
		err := solana.SetUpgradeAuthority(
			ctx,
			targetProgramID,
			upgradeAuthorityPDA,
			deployerPath,
			s.SolanaChain.RPCURL,
		)
		s.Require().NoError(err, "failed to transfer program upgrade authority to AccessManager")
	}))

	var bufferAccount solanago.PublicKey

	s.Require().True(s.Run("Write new program binary to buffer and transfer authority", func() {
		var err error

		// For this test, we use the same binary to focus on the upgrade mechanism
		// and access control, which is the primary goal of this test suite.
		bufferAccount, err = solana.WriteProgramBuffer(
			ctx,
			programSoFile,
			deployerPath,
			s.SolanaChain.RPCURL,
		)
		s.Require().NoError(err, "failed to write program buffer")
		s.Require().NotEqual(solanago.PublicKey{}, bufferAccount, "buffer account should not be empty")

		// Transfer buffer authority to match program upgrade authority (security requirement)
		err = solana.SetBufferAuthority(
			ctx,
			bufferAccount,
			upgradeAuthorityPDA,
			deployerPath,
			s.SolanaChain.RPCURL,
		)
		s.Require().NoError(err, "failed to transfer buffer authority to upgrade authority PDA")
	}))

	s.Require().True(s.Run("Upgrade program via AccessManager with UPGRADER_ROLE", func() {
		accessControlAccount, _ := solana.AccessManager.AccessManagerPDA(access_manager.ProgramID)

		upgradeProgramInstruction, err := access_manager.NewUpgradeProgramInstruction(
			targetProgramID,
			accessControlAccount,
			targetProgramID,
			programDataAccount,
			bufferAccount,
			upgradeAuthorityPDA,
			s.UpgraderWallet.PublicKey(),
			s.UpgraderWallet.PublicKey(),
			solanago.SysVarInstructionsPubkey,
			solanago.BPFLoaderUpgradeableProgramID,
			solanago.SysVarRentPubkey,
			solanago.SysVarClockPubkey,
		)
		s.Require().NoError(err, "failed to build upgrade program instruction")

		computeBudgetIx := solana.NewComputeBudgetInstruction(400_000)

		tx, err := s.SolanaChain.NewTransactionFromInstructions(
			s.UpgraderWallet.PublicKey(),
			computeBudgetIx,
			upgradeProgramInstruction,
		)
		s.Require().NoError(err, "failed to create upgrade transaction")

		sig, err := s.SolanaChain.SignAndBroadcastTxWithRetry(ctx, tx, rpc.CommitmentConfirmed, s.UpgraderWallet)
		s.Require().NoError(err, "program upgrade should succeed with UPGRADER_ROLE")
		s.Require().NotEqual(solanago.Signature{}, sig, "upgrade signature should not be empty")
	}))

	s.Require().True(s.Run("Unauthorized account cannot upgrade program", func() {
		unauthorizedWallet, err := s.SolanaChain.CreateAndFundWallet()
		s.Require().NoError(err, "failed to create unauthorized wallet")

		const keypairDir = "solana-keypairs/localnet"
		const deployerPath = keypairDir + "/deployer_wallet.json"
		const programSoFile = "programs/solana/target/deploy/ics26_router.so"

		unauthorizedBuffer, err := solana.WriteProgramBuffer(
			ctx,
			programSoFile,
			deployerPath,
			s.SolanaChain.RPCURL,
		)
		s.Require().NoError(err, "failed to write buffer for unauthorized test")

		err = solana.SetBufferAuthority(
			ctx,
			unauthorizedBuffer,
			upgradeAuthorityPDA,
			deployerPath,
			s.SolanaChain.RPCURL,
		)
		s.Require().NoError(err, "failed to transfer buffer authority for unauthorized test")

		accessControlAccount, _ := solana.AccessManager.AccessManagerPDA(access_manager.ProgramID)

		upgradeProgramInstruction, err := access_manager.NewUpgradeProgramInstruction(
			targetProgramID,
			accessControlAccount,
			targetProgramID,
			programDataAccount,
			unauthorizedBuffer,
			upgradeAuthorityPDA,
			unauthorizedWallet.PublicKey(),
			unauthorizedWallet.PublicKey(),
			solanago.SysVarInstructionsPubkey,
			solanago.BPFLoaderUpgradeableProgramID,
			solanago.SysVarRentPubkey,
			solanago.SysVarClockPubkey,
		)
		s.Require().NoError(err, "failed to build upgrade instruction for unauthorized test")

		computeBudgetIx := solana.NewComputeBudgetInstruction(400_000)

		tx, err := s.SolanaChain.NewTransactionFromInstructions(
			unauthorizedWallet.PublicKey(),
			computeBudgetIx,
			upgradeProgramInstruction,
		)
		s.Require().NoError(err, "failed to create unauthorized upgrade transaction")

		// Use SignAndBroadcastTxWithOpts for immediate failure without retry (this is a negative test)
		_, err = s.SolanaChain.SignAndBroadcastTxWithOpts(ctx, tx, rpc.ConfirmationStatusConfirmed, unauthorizedWallet)
		s.Require().Error(err, "upgrade should fail without UPGRADER_ROLE")
		s.Require().Contains(err.Error(), "Custom", "should be Unauthorized error")
	}))

	s.Require().True(s.Run("Cannot bypass AccessManager and upgrade directly", func() {
		// This test verifies that after transferring upgrade authority to the AccessManager PDA,
		// the old authority (deployer) cannot bypass AccessManager by calling BPF Loader directly.

		const keypairDir = "solana-keypairs/localnet"
		const deployerPath = keypairDir + "/deployer_wallet.json"
		const programSoFile = "programs/solana/target/deploy/ics26_router.so"

		// Create a buffer with deployer as authority
		bypassBuffer, err := solana.WriteProgramBuffer(
			ctx,
			programSoFile,
			deployerPath,
			s.SolanaChain.RPCURL,
		)
		s.Require().NoError(err, "failed to write buffer for bypass test")

		// Attempt to upgrade directly using BPF Loader (bypassing AccessManager)
		// This should fail because the program's upgrade authority is now the AccessManager PDA,
		// not the deployer wallet
		err = solana.UpgradeProgramDirect(
			ctx,
			targetProgramID,
			bypassBuffer,
			deployerPath,
			s.SolanaChain.RPCURL,
		)
		// The direct upgrade should fail because the program's upgrade authority is now the AccessManager PDA
		s.Require().Error(err, "direct upgrade should fail - authority is now AccessManager PDA")
		s.Require().Contains(err.Error(), "does not match authority", "should fail with authority mismatch")
	}))
}

// Test_RevokeUpgraderRole demonstrates revoking upgrade permissions
func (s *IbcEurekaSolanaUpgradeTestSuite) Test_RevokeUpgraderRole() {
	ctx := context.Background()

	// Enable mock WASM client to avoid relayer unimplemented panic
	s.UseMockWasmClient = true

	s.SetupSuite(ctx)

	var upgraderWallet *solanago.Wallet

	s.Require().True(s.Run("Setup: Create and grant UPGRADER_ROLE", func() {
		var err error
		upgraderWallet, err = s.SolanaChain.CreateAndFundWallet()
		s.Require().NoError(err)

		accessControlAccount, _ := solana.AccessManager.AccessManagerPDA(access_manager.ProgramID)
		const UPGRADER_ROLE = uint64(8)

		grantInstruction, err := access_manager.NewGrantRoleInstruction(
			UPGRADER_ROLE,
			upgraderWallet.PublicKey(),
			accessControlAccount,
			s.SolanaRelayer.PublicKey(),
			solanago.SysVarInstructionsPubkey,
		)
		s.Require().NoError(err)

		tx, err := s.SolanaChain.NewTransactionFromInstructions(
			s.SolanaRelayer.PublicKey(),
			grantInstruction,
		)
		s.Require().NoError(err)

		_, err = s.SolanaChain.SignAndBroadcastTxWithRetry(ctx, tx, rpc.CommitmentConfirmed, s.SolanaRelayer)
		s.Require().NoError(err)
	}))

	s.Require().True(s.Run("Revoke UPGRADER_ROLE", func() {
		accessControlAccount, _ := solana.AccessManager.AccessManagerPDA(access_manager.ProgramID)
		const UPGRADER_ROLE = uint64(8)

		revokeInstruction, err := access_manager.NewRevokeRoleInstruction(
			UPGRADER_ROLE,
			upgraderWallet.PublicKey(),
			accessControlAccount,
			s.SolanaRelayer.PublicKey(), // Admin revokes
			solanago.SysVarInstructionsPubkey,
		)
		s.Require().NoError(err)

		tx, err := s.SolanaChain.NewTransactionFromInstructions(
			s.SolanaRelayer.PublicKey(),
			revokeInstruction,
		)
		s.Require().NoError(err)

		_, err = s.SolanaChain.SignAndBroadcastTxWithRetry(ctx, tx, rpc.CommitmentConfirmed, s.SolanaRelayer)
		s.Require().NoError(err, "failed to revoke UPGRADER_ROLE")
	}))

	s.Require().True(s.Run("Verify revoked account cannot upgrade", func() {
		const keypairDir = "solana-keypairs/localnet"
		const deployerPath = keypairDir + "/deployer_wallet.json"
		const programSoFile = "programs/solana/target/deploy/ics26_router.so"

		accessControlAccount, _ := solana.AccessManager.AccessManagerPDA(access_manager.ProgramID)
		targetProgramID := ics26_router.ProgramID
		programDataAccount, err := solana.GetProgramDataAddress(targetProgramID)
		s.Require().NoError(err)

		upgradeAuthorityPDA, _ := solana.AccessManager.UpgradeAuthorityPDA(
			access_manager.ProgramID,
			targetProgramID.Bytes(),
		)

		// Write buffer
		buffer, err := solana.WriteProgramBuffer(
			ctx,
			programSoFile,
			deployerPath,
			s.SolanaChain.RPCURL,
		)
		s.Require().NoError(err)

		// Transfer buffer authority to upgrade authority PDA
		err = solana.SetBufferAuthority(
			ctx,
			buffer,
			upgradeAuthorityPDA,
			deployerPath,
			s.SolanaChain.RPCURL,
		)
		s.Require().NoError(err)

		upgradeInstruction, err := access_manager.NewUpgradeProgramInstruction(
			targetProgramID,
			accessControlAccount,
			targetProgramID,
			programDataAccount,
			buffer,
			upgradeAuthorityPDA,
			upgraderWallet.PublicKey(),
			upgraderWallet.PublicKey(), // Revoked account
			solanago.SysVarInstructionsPubkey,
			solanago.BPFLoaderUpgradeableProgramID,
			solanago.SysVarRentPubkey,
			solanago.SysVarClockPubkey,
		)
		s.Require().NoError(err)

		computeBudgetIx := solana.NewComputeBudgetInstruction(400_000)

		tx, err := s.SolanaChain.NewTransactionFromInstructions(
			upgraderWallet.PublicKey(),
			computeBudgetIx,
			upgradeInstruction,
		)
		s.Require().NoError(err)

		// Should fail after role revocation
		_, err = s.SolanaChain.SignAndBroadcastTxWithRetry(ctx, tx, rpc.CommitmentConfirmed, upgraderWallet)
		s.Require().Error(err, "upgrade should fail after role revocation")
	}))
}
