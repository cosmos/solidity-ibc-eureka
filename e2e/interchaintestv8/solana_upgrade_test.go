package main

import (
	"context"
	"fmt"
	"os"
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
	ADMIN_ROLE    = uint64(0)
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
// PDA, we enable role-based upgrades where only accounts with ADMIN_ROLE can upgrade.
//
// TEST FLOW:
// 1. Create an upgrader wallet and grant it ADMIN_ROLE
// 2. Derive required PDAs (program data account, upgrade authority PDA)
// 3. Transfer program upgrade authority from deployer to AccessManager's PDA (one-time setup)
// 4. Write new program bytecode to a buffer account
// 5. Transfer buffer authority to match program upgrade authority (security requirement)
// 6. Call AccessManager.upgrade_program() with the upgrader wallet (has ADMIN_ROLE)
//   - AccessManager checks role membership
//   - AccessManager calls BPFLoaderUpgradeable.upgrade via invoke_signed with PDA signature
//   - BPF Loader verifies authorities match and replaces bytecode
//
// 7. Verify unauthorized accounts cannot upgrade (negative test)
//
// SECURITY MODEL:
// - Role-based access: Only ADMIN_ROLE can trigger upgrades (AccessManager enforcement)
// - Authority matching: Buffer authority must equal program upgrade authority (BPF Loader enforcement)
// - CPI protection: Upgrade cannot be called via CPI (instructions sysvar check)
// - PDA verification: Upgrade authority PDA seeds are validated (Anchor constraints)
func (s *IbcEurekaSolanaUpgradeTestSuite) Test_ProgramUpgrade_Via_AccessManager() {
	ctx := context.Background()

	s.SetupSuite(ctx)

	s.Require().True(s.Run("Setup: Create upgrader wallet", func() {
		var err error
		s.UpgraderWallet, err = s.Solana.Chain.CreateAndFundWallet()
		s.Require().NoError(err, "failed to create and fund upgrader wallet")
	}))

	s.Require().True(s.Run("Setup: Grant ADMIN_ROLE to upgrader wallet", func() {
		accessControlAccount, _ := solana.AccessManager.AccessManagerPDA(access_manager.ProgramID)

		grantAdminRoleInstruction, err := access_manager.NewGrantRoleInstruction(
			ADMIN_ROLE,
			s.UpgraderWallet.PublicKey(),
			accessControlAccount,
			s.SolanaRelayer.PublicKey(),
			solanago.SysVarInstructionsPubkey,
		)
		s.Require().NoError(err, "failed to build grant ADMIN_ROLE instruction")

		tx, err := s.Solana.Chain.NewTransactionFromInstructions(
			s.SolanaRelayer.PublicKey(),
			grantAdminRoleInstruction,
		)
		s.Require().NoError(err, "failed to create grant role transaction")

		_, err = s.Solana.Chain.SignAndBroadcastTxWithRetry(ctx, tx, rpc.CommitmentConfirmed, s.SolanaRelayer)
		s.Require().NoError(err, "failed to grant ADMIN_ROLE")
	}))

	targetProgramID := ics26_router.ProgramID

	var programDataAccount solanago.PublicKey
	var upgradeAuthorityPDA solanago.PublicKey

	s.Require().True(s.Run("Derive upgrade authority and program data accounts", func() {
		var err error

		programDataAccount, err = solana.GetProgramDataAddress(targetProgramID)
		s.Require().NoError(err, "failed to derive program data address")

		upgradeAuthorityPDA, _ = solana.AccessManager.UpgradeAuthorityWithArgSeedPDA(
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
			s.Solana.Chain.RPCURL,
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
			s.Solana.Chain.RPCURL,
		)
		s.Require().NoError(err, "failed to write program buffer")
		s.Require().NotEqual(solanago.PublicKey{}, bufferAccount, "buffer account should not be empty")

		// Transfer buffer authority to match program upgrade authority (security requirement)
		err = solana.SetBufferAuthority(
			ctx,
			bufferAccount,
			upgradeAuthorityPDA,
			deployerPath,
			s.Solana.Chain.RPCURL,
		)
		s.Require().NoError(err, "failed to transfer buffer authority to upgrade authority PDA")
	}))

	s.Require().True(s.Run("Upgrade program via AccessManager with ADMIN_ROLE", func() {
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

		tx, err := s.Solana.Chain.NewTransactionFromInstructions(
			s.UpgraderWallet.PublicKey(),
			computeBudgetIx,
			upgradeProgramInstruction,
		)
		s.Require().NoError(err, "failed to create upgrade transaction")

		sig, err := s.Solana.Chain.SignAndBroadcastTxWithRetry(ctx, tx, rpc.CommitmentConfirmed, s.UpgraderWallet)
		s.Require().NoError(err, "program upgrade should succeed with ADMIN_ROLE")
		s.Require().NotEqual(solanago.Signature{}, sig, "upgrade signature should not be empty")
	}))

	s.Require().True(s.Run("Unauthorized account cannot upgrade program", func() {
		unauthorizedWallet, err := s.Solana.Chain.CreateAndFundWallet()
		s.Require().NoError(err, "failed to create unauthorized wallet")

		unauthorizedBuffer, err := solana.WriteProgramBuffer(
			ctx,
			programSoFile,
			deployerPath,
			s.Solana.Chain.RPCURL,
		)
		s.Require().NoError(err, "failed to write buffer for unauthorized test")

		err = solana.SetBufferAuthority(
			ctx,
			unauthorizedBuffer,
			upgradeAuthorityPDA,
			deployerPath,
			s.Solana.Chain.RPCURL,
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

		tx, err := s.Solana.Chain.NewTransactionFromInstructions(
			unauthorizedWallet.PublicKey(),
			computeBudgetIx,
			upgradeProgramInstruction,
		)
		s.Require().NoError(err, "failed to create unauthorized upgrade transaction")

		// Use SignAndBroadcastTxWithOpts for immediate failure without retry (this is a negative test)
		_, err = s.Solana.Chain.SignAndBroadcastTxWithOpts(ctx, tx, rpc.ConfirmationStatusConfirmed, unauthorizedWallet)
		s.Require().Error(err, "upgrade should fail without ADMIN_ROLE")
		s.Require().Contains(err.Error(), "Custom", "should be Unauthorized error")
	}))

	s.Require().True(s.Run("Cannot bypass AccessManager and upgrade directly", func() {
		// This test verifies that after transferring upgrade authority to the AccessManager PDA,
		// the old authority (deployer) cannot bypass AccessManager by calling BPF Loader directly.

		// Create a buffer with deployer as authority
		bypassBuffer, err := solana.WriteProgramBuffer(
			ctx,
			programSoFile,
			deployerPath,
			s.Solana.Chain.RPCURL,
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
			s.Solana.Chain.RPCURL,
		)
		// The direct upgrade should fail because the program's upgrade authority is now the AccessManager PDA
		s.Require().Error(err, "direct upgrade should fail - authority is now AccessManager PDA")
		s.Require().Contains(err.Error(), "does not match authority", "should fail with authority mismatch")
	}))
}

// Test_RevokeAdminRole demonstrates that revoking ADMIN_ROLE from an account prevents upgrades
func (s *IbcEurekaSolanaUpgradeTestSuite) Test_RevokeAdminRole() {
	ctx := context.Background()

	s.SetupSuite(ctx)

	var secondAdmin *solanago.Wallet

	s.Require().True(s.Run("Setup: Create and grant ADMIN_ROLE to second admin", func() {
		var err error
		secondAdmin, err = s.Solana.Chain.CreateAndFundWallet()
		s.Require().NoError(err)

		accessControlAccount, _ := solana.AccessManager.AccessManagerPDA(access_manager.ProgramID)

		grantInstruction, err := access_manager.NewGrantRoleInstruction(
			ADMIN_ROLE,
			secondAdmin.PublicKey(),
			accessControlAccount,
			s.SolanaRelayer.PublicKey(),
			solanago.SysVarInstructionsPubkey,
		)
		s.Require().NoError(err)

		tx, err := s.Solana.Chain.NewTransactionFromInstructions(
			s.SolanaRelayer.PublicKey(),
			grantInstruction,
		)
		s.Require().NoError(err)

		_, err = s.Solana.Chain.SignAndBroadcastTxWithRetry(ctx, tx, rpc.CommitmentConfirmed, s.SolanaRelayer)
		s.Require().NoError(err)
	}))

	s.Require().True(s.Run("Revoke ADMIN_ROLE from second admin", func() {
		accessControlAccount, _ := solana.AccessManager.AccessManagerPDA(access_manager.ProgramID)

		revokeInstruction, err := access_manager.NewRevokeRoleInstruction(
			ADMIN_ROLE,
			secondAdmin.PublicKey(),
			accessControlAccount,
			s.SolanaRelayer.PublicKey(), // Primary admin revokes
			solanago.SysVarInstructionsPubkey,
		)
		s.Require().NoError(err)

		tx, err := s.Solana.Chain.NewTransactionFromInstructions(
			s.SolanaRelayer.PublicKey(),
			revokeInstruction,
		)
		s.Require().NoError(err)

		_, err = s.Solana.Chain.SignAndBroadcastTxWithRetry(ctx, tx, rpc.CommitmentConfirmed, s.SolanaRelayer)
		s.Require().NoError(err, "failed to revoke ADMIN_ROLE")
	}))

	s.Require().True(s.Run("Verify revoked admin cannot upgrade", func() {
		accessControlAccount, _ := solana.AccessManager.AccessManagerPDA(access_manager.ProgramID)
		targetProgramID := ics26_router.ProgramID
		programDataAccount, err := solana.GetProgramDataAddress(targetProgramID)
		s.Require().NoError(err)

		upgradeAuthorityPDA, _ := solana.AccessManager.UpgradeAuthorityWithArgSeedPDA(
			access_manager.ProgramID,
			targetProgramID.Bytes(),
		)

		// Write buffer
		buffer, err := solana.WriteProgramBuffer(
			ctx,
			programSoFile,
			deployerPath,
			s.Solana.Chain.RPCURL,
		)
		s.Require().NoError(err)

		// Transfer buffer authority to upgrade authority PDA
		err = solana.SetBufferAuthority(
			ctx,
			buffer,
			upgradeAuthorityPDA,
			deployerPath,
			s.Solana.Chain.RPCURL,
		)
		s.Require().NoError(err)

		upgradeInstruction, err := access_manager.NewUpgradeProgramInstruction(
			targetProgramID,
			accessControlAccount,
			targetProgramID,
			programDataAccount,
			buffer,
			upgradeAuthorityPDA,
			secondAdmin.PublicKey(),
			secondAdmin.PublicKey(), // Revoked admin
			solanago.SysVarInstructionsPubkey,
			solanago.BPFLoaderUpgradeableProgramID,
			solanago.SysVarRentPubkey,
			solanago.SysVarClockPubkey,
		)
		s.Require().NoError(err)

		computeBudgetIx := solana.NewComputeBudgetInstruction(400_000)

		tx, err := s.Solana.Chain.NewTransactionFromInstructions(
			secondAdmin.PublicKey(),
			computeBudgetIx,
			upgradeInstruction,
		)
		s.Require().NoError(err)

		// Should fail after role revocation
		_, err = s.Solana.Chain.SignAndBroadcastTxWithRetry(ctx, tx, rpc.CommitmentConfirmed, secondAdmin)
		s.Require().Error(err, "upgrade should fail after ADMIN_ROLE revocation")
	}))
}

// Test_TransferUpgradeAuthority demonstrates the two-step authority transfer flow for access manager migration.
//
// SCENARIO:
// When migrating to a new access manager or transferring upgrade control, the current access manager
// needs to relinquish its upgrade authority over managed programs. This uses a two-step propose/accept
// pattern to prevent irreversible mistakes:
//   - Admin proposes the transfer (sets pending state)
//   - New authority accepts the transfer (executes the BPF Loader SetAuthority CPI)
//
// TEST FLOW:
// 1. Grant ADMIN_ROLE to upgrader wallet
// 2. Transfer program upgrade authority from deployer to AccessManager's PDA (standard setup)
// 3. Verify baseline: upgrade via AccessManager works
// 4. Create a new authority keypair
// 5. Propose transfer: admin calls propose_upgrade_authority_transfer
// 6. Accept transfer: new authority calls accept_upgrade_authority_transfer
// 7. Verify new authority can upgrade the program directly
// 8. Verify AccessManager can no longer upgrade the program (negative test)
func (s *IbcEurekaSolanaUpgradeTestSuite) Test_TransferUpgradeAuthority() {
	ctx := context.Background()

	s.SetupSuite(ctx)

	s.Require().True(s.Run("Setup: Create upgrader wallet", func() {
		var err error
		s.UpgraderWallet, err = s.Solana.Chain.CreateAndFundWallet()
		s.Require().NoError(err, "failed to create and fund upgrader wallet")
	}))

	s.Require().True(s.Run("Setup: Grant ADMIN_ROLE to upgrader wallet", func() {
		accessControlAccount, _ := solana.AccessManager.AccessManagerPDA(access_manager.ProgramID)

		grantAdminRoleInstruction, err := access_manager.NewGrantRoleInstruction(
			ADMIN_ROLE,
			s.UpgraderWallet.PublicKey(),
			accessControlAccount,
			s.SolanaRelayer.PublicKey(),
			solanago.SysVarInstructionsPubkey,
		)
		s.Require().NoError(err, "failed to build grant ADMIN_ROLE instruction")

		tx, err := s.Solana.Chain.NewTransactionFromInstructions(
			s.SolanaRelayer.PublicKey(),
			grantAdminRoleInstruction,
		)
		s.Require().NoError(err, "failed to create grant role transaction")

		_, err = s.Solana.Chain.SignAndBroadcastTxWithRetry(ctx, tx, rpc.CommitmentConfirmed, s.SolanaRelayer)
		s.Require().NoError(err, "failed to grant ADMIN_ROLE")
	}))

	targetProgramID := ics26_router.ProgramID

	var programDataAccount solanago.PublicKey
	var upgradeAuthorityPDA solanago.PublicKey

	s.Require().True(s.Run("Derive upgrade authority and program data accounts", func() {
		var err error

		programDataAccount, err = solana.GetProgramDataAddress(targetProgramID)
		s.Require().NoError(err, "failed to derive program data address")

		upgradeAuthorityPDA, _ = solana.AccessManager.UpgradeAuthorityWithArgSeedPDA(
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
			s.Solana.Chain.RPCURL,
		)
		s.Require().NoError(err, "failed to transfer program upgrade authority to AccessManager")
	}))

	s.Require().True(s.Run("Verify baseline: upgrade via AccessManager succeeds", func() {
		bufferAccount, err := solana.WriteProgramBuffer(
			ctx,
			programSoFile,
			deployerPath,
			s.Solana.Chain.RPCURL,
		)
		s.Require().NoError(err, "failed to write program buffer")

		err = solana.SetBufferAuthority(
			ctx,
			bufferAccount,
			upgradeAuthorityPDA,
			deployerPath,
			s.Solana.Chain.RPCURL,
		)
		s.Require().NoError(err, "failed to transfer buffer authority")

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

		tx, err := s.Solana.Chain.NewTransactionFromInstructions(
			s.UpgraderWallet.PublicKey(),
			computeBudgetIx,
			upgradeProgramInstruction,
		)
		s.Require().NoError(err, "failed to create upgrade transaction")

		_, err = s.Solana.Chain.SignAndBroadcastTxWithRetry(ctx, tx, rpc.CommitmentConfirmed, s.UpgraderWallet)
		s.Require().NoError(err, "baseline upgrade via AccessManager should succeed")
	}))

	var newAuthorityWallet *solanago.Wallet
	var newAuthorityKeypairPath string

	s.Require().True(s.Run("Create new authority keypair", func() {
		var err error
		newAuthorityWallet, err = s.Solana.Chain.CreateAndFundWallet()
		s.Require().NoError(err, "failed to create new authority wallet")

		newAuthorityKeypairPath, err = solana.WriteKeypairToTempFile(newAuthorityWallet)
		s.Require().NoError(err, "failed to write new authority keypair to temp file")
	}))

	defer func() {
		if newAuthorityKeypairPath != "" {
			os.Remove(newAuthorityKeypairPath)
		}
	}()

	s.Require().True(s.Run("Propose upgrade authority transfer", func() {
		accessControlAccount, _ := solana.AccessManager.AccessManagerPDA(access_manager.ProgramID)

		proposeIx, err := access_manager.NewProposeUpgradeAuthorityTransferInstruction(
			targetProgramID,
			newAuthorityWallet.PublicKey(),
			accessControlAccount,
			s.UpgraderWallet.PublicKey(),
			solanago.SysVarInstructionsPubkey,
		)
		s.Require().NoError(err, "failed to build propose upgrade authority transfer instruction")

		tx, err := s.Solana.Chain.NewTransactionFromInstructions(
			s.UpgraderWallet.PublicKey(),
			proposeIx,
		)
		s.Require().NoError(err, "failed to create propose transfer transaction")

		_, err = s.Solana.Chain.SignAndBroadcastTxWithRetry(ctx, tx, rpc.CommitmentConfirmed, s.UpgraderWallet)
		s.Require().NoError(err, "propose upgrade authority transfer should succeed")
	}))

	s.Require().True(s.Run("Accept upgrade authority transfer", func() {
		accessControlAccount, _ := solana.AccessManager.AccessManagerPDA(access_manager.ProgramID)

		acceptIx, err := access_manager.NewAcceptUpgradeAuthorityTransferInstruction(
			targetProgramID,
			accessControlAccount,
			programDataAccount,
			upgradeAuthorityPDA,
			newAuthorityWallet.PublicKey(),
			solanago.BPFLoaderUpgradeableProgramID,
		)
		s.Require().NoError(err, "failed to build accept upgrade authority transfer instruction")

		tx, err := s.Solana.Chain.NewTransactionFromInstructions(
			newAuthorityWallet.PublicKey(),
			acceptIx,
		)
		s.Require().NoError(err, "failed to create accept transfer transaction")

		_, err = s.Solana.Chain.SignAndBroadcastTxWithRetry(ctx, tx, rpc.CommitmentConfirmed, newAuthorityWallet)
		s.Require().NoError(err, "accept upgrade authority transfer should succeed")
	}))

	s.Require().True(s.Run("New authority can upgrade program directly", func() {
		// Use deployer to write the buffer (has more SOL for rent)
		bufferAccount, err := solana.WriteProgramBuffer(
			ctx,
			programSoFile,
			deployerPath,
			s.Solana.Chain.RPCURL,
		)
		s.Require().NoError(err, "failed to write program buffer")

		// Transfer buffer authority to the new authority
		err = solana.SetBufferAuthority(
			ctx,
			bufferAccount,
			newAuthorityWallet.PublicKey(),
			deployerPath,
			s.Solana.Chain.RPCURL,
		)
		s.Require().NoError(err, "failed to set buffer authority to new authority")

		err = solana.UpgradeProgramDirect(
			ctx,
			targetProgramID,
			bufferAccount,
			newAuthorityKeypairPath,
			s.Solana.Chain.RPCURL,
		)
		s.Require().NoError(err, "new authority should be able to upgrade program directly")
	}))

	s.Require().True(s.Run("AccessManager can no longer upgrade program", func() {
		bufferAccount, err := solana.WriteProgramBuffer(
			ctx,
			programSoFile,
			deployerPath,
			s.Solana.Chain.RPCURL,
		)
		s.Require().NoError(err, "failed to write program buffer for AM upgrade attempt")

		err = solana.SetBufferAuthority(
			ctx,
			bufferAccount,
			upgradeAuthorityPDA,
			deployerPath,
			s.Solana.Chain.RPCURL,
		)
		s.Require().NoError(err, "failed to set buffer authority to AM PDA")

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
		s.Require().NoError(err, "failed to build upgrade instruction for AM attempt")

		computeBudgetIx := solana.NewComputeBudgetInstruction(400_000)

		tx, err := s.Solana.Chain.NewTransactionFromInstructions(
			s.UpgraderWallet.PublicKey(),
			computeBudgetIx,
			upgradeProgramInstruction,
		)
		s.Require().NoError(err, "failed to create AM upgrade transaction")

		_, err = s.Solana.Chain.SignAndBroadcastTxWithOpts(ctx, tx, rpc.ConfirmationStatusConfirmed, s.UpgraderWallet)
		s.Require().Error(err, "AM upgrade should fail after authority was transferred away")
	}))
}

// withAMProgramID temporarily swaps access_manager.ProgramID to build instructions for a different AM instance.
func withAMProgramID(programID solanago.PublicKey, fn func() (solanago.Instruction, error)) (solanago.Instruction, error) {
	saved := access_manager.ProgramID
	access_manager.ProgramID = programID
	defer func() { access_manager.ProgramID = saved }()
	return fn()
}

// Test_AMtoAM_UpgradeAuthorityMigration demonstrates migrating upgrade authority
// from one AccessManager instance (AM-A) to another (AM-B) using claim_upgrade_authority.
//
// SCENARIO:
// When replacing an AccessManager deployment (e.g. upgrading AM logic), the old AM
// must transfer its upgrade authority over managed programs to the new AM. Since the
// new authority is a PDA (not a keypair), the standard propose/accept flow doesn't
// work -- PDAs can only sign via invoke_signed from their owning program.
//
// claim_upgrade_authority solves this: AM-B CPIs into AM-A's accept_upgrade_authority_transfer
// with its own PDA as the signer.
//
// TEST FLOW:
// 1. Deploy AM-B (test_access_manager) alongside the already-deployed AM-A
// 2. Initialize AM-B with deployer as admin
// 3. Grant ADMIN_ROLE on AM-A to upgrader wallet
// 4. Transfer target program's upgrade authority to AM-A's PDA
// 5. AM-A admin proposes transfer to AM-B's upgrade authority PDA
// 6. Anyone calls AM-B's claim_upgrade_authority (PDA signing = authorization)
// 7. Verify: target program's authority is now AM-B's PDA
// 8. Verify: AM-B admin can upgrade the target program
// 9. Verify: AM-A can no longer upgrade the target program
func (s *IbcEurekaSolanaUpgradeTestSuite) Test_AMtoAM_UpgradeAuthorityMigration() {
	ctx := context.Background()

	s.SetupSuite(ctx)

	// Deploy AM-B (test_access_manager)
	var amBProgramID solanago.PublicKey

	s.Require().True(s.Run("Deploy AM-B (test_access_manager)", func() {
		var err error
		amBKeypairPath := fmt.Sprintf("%s/test_access_manager-keypair.json", keypairDir)
		amBProgramID, err = s.Solana.Chain.DeploySolanaProgramAsync(ctx, "test_access_manager", amBKeypairPath, deployerPath)
		s.Require().NoError(err, "failed to deploy test_access_manager")
		s.T().Logf("AM-B deployed at: %s", amBProgramID)
	}))

	// Initialize AM-B
	s.Require().True(s.Run("Initialize AM-B", func() {
		deployerWallet, err := solana.LoadDeployerWallet(deployerPath)
		s.Require().NoError(err)

		amBAccessManagerPDA, _ := solana.AccessManager.AccessManagerPDA(amBProgramID)
		amBProgramDataPDA, err := solana.GetProgramDataAddress(amBProgramID)
		s.Require().NoError(err)

		initIx, err := withAMProgramID(amBProgramID, func() (solanago.Instruction, error) {
			return access_manager.NewInitializeInstruction(
				s.SolanaRelayer.PublicKey(),
				amBAccessManagerPDA,
				s.SolanaRelayer.PublicKey(),
				solanago.SystemProgramID,
				solanago.SysVarInstructionsPubkey,
				amBProgramDataPDA,
				solana.DeployerPubkey,
			)
		})
		s.Require().NoError(err, "failed to build AM-B initialize instruction")

		tx, err := s.Solana.Chain.NewTransactionFromInstructions(s.SolanaRelayer.PublicKey(), initIx)
		s.Require().NoError(err)

		_, err = s.Solana.Chain.SignAndBroadcastTxWithRetryAndTimeout(ctx, tx, rpc.CommitmentConfirmed, 30, s.SolanaRelayer, deployerWallet)
		s.Require().NoError(err, "failed to initialize AM-B")
		s.T().Log("AM-B initialized")
	}))

	// Setup upgrader wallet with ADMIN_ROLE on AM-A
	s.Require().True(s.Run("Setup: Create upgrader wallet and grant ADMIN_ROLE on AM-A", func() {
		var err error
		s.UpgraderWallet, err = s.Solana.Chain.CreateAndFundWallet()
		s.Require().NoError(err)

		accessControlAccount, _ := solana.AccessManager.AccessManagerPDA(access_manager.ProgramID)

		grantIx, err := access_manager.NewGrantRoleInstruction(
			ADMIN_ROLE,
			s.UpgraderWallet.PublicKey(),
			accessControlAccount,
			s.SolanaRelayer.PublicKey(),
			solanago.SysVarInstructionsPubkey,
		)
		s.Require().NoError(err)

		tx, err := s.Solana.Chain.NewTransactionFromInstructions(s.SolanaRelayer.PublicKey(), grantIx)
		s.Require().NoError(err)

		_, err = s.Solana.Chain.SignAndBroadcastTxWithRetry(ctx, tx, rpc.CommitmentConfirmed, s.SolanaRelayer)
		s.Require().NoError(err, "failed to grant ADMIN_ROLE on AM-A")
	}))

	targetProgramID := ics26_router.ProgramID

	var programDataAccount solanago.PublicKey
	var amAUpgradeAuthorityPDA solanago.PublicKey
	var amBUpgradeAuthorityPDA solanago.PublicKey

	s.Require().True(s.Run("Derive PDAs", func() {
		var err error

		programDataAccount, err = solana.GetProgramDataAddress(targetProgramID)
		s.Require().NoError(err)

		amAUpgradeAuthorityPDA, _ = solana.AccessManager.UpgradeAuthorityWithArgSeedPDA(
			access_manager.ProgramID,
			targetProgramID.Bytes(),
		)

		amBUpgradeAuthorityPDA, _ = solana.AccessManager.UpgradeAuthorityWithArgSeedPDA(
			amBProgramID,
			targetProgramID.Bytes(),
		)

		s.T().Logf("AM-A upgrade authority PDA: %s", amAUpgradeAuthorityPDA)
		s.T().Logf("AM-B upgrade authority PDA: %s", amBUpgradeAuthorityPDA)
	}))

	// Transfer target program's upgrade authority to AM-A's PDA
	s.Require().True(s.Run("Transfer target program authority to AM-A's PDA", func() {
		err := solana.SetUpgradeAuthority(
			ctx,
			targetProgramID,
			amAUpgradeAuthorityPDA,
			deployerPath,
			s.Solana.Chain.RPCURL,
		)
		s.Require().NoError(err, "failed to transfer authority to AM-A")
	}))

	// AM-A proposes transfer to AM-B's upgrade authority PDA
	s.Require().True(s.Run("AM-A proposes transfer to AM-B's PDA", func() {
		accessControlAccount, _ := solana.AccessManager.AccessManagerPDA(access_manager.ProgramID)

		proposeIx, err := access_manager.NewProposeUpgradeAuthorityTransferInstruction(
			targetProgramID,
			amBUpgradeAuthorityPDA,
			accessControlAccount,
			s.UpgraderWallet.PublicKey(),
			solanago.SysVarInstructionsPubkey,
		)
		s.Require().NoError(err, "failed to build propose instruction")

		tx, err := s.Solana.Chain.NewTransactionFromInstructions(s.UpgraderWallet.PublicKey(), proposeIx)
		s.Require().NoError(err)

		_, err = s.Solana.Chain.SignAndBroadcastTxWithRetry(ctx, tx, rpc.CommitmentConfirmed, s.UpgraderWallet)
		s.Require().NoError(err, "propose transfer to AM-B should succeed")
	}))

	// AM-B claims upgrade authority via CPI into AM-A's accept.
	// Use a random wallet (not an admin) to demonstrate the claim is permissionless --
	// PDA signing is the only authorization required.
	s.Require().True(s.Run("AM-B claims upgrade authority (permissionless)", func() {
		randomWallet, err := s.Solana.Chain.CreateAndFundWallet()
		s.Require().NoError(err, "failed to create random wallet")

		amAProgramID := access_manager.ProgramID
		amAAccessManagerPDA, _ := solana.AccessManager.AccessManagerPDA(amAProgramID)

		claimIx, err := withAMProgramID(amBProgramID, func() (solanago.Instruction, error) {
			return access_manager.NewClaimUpgradeAuthorityInstruction(
				targetProgramID,
				amBUpgradeAuthorityPDA,
				amAAccessManagerPDA,
				programDataAccount,
				amAUpgradeAuthorityPDA,
				amAProgramID,
				solanago.BPFLoaderUpgradeableProgramID,
			)
		})
		s.Require().NoError(err, "failed to build claim instruction")

		computeBudgetIx := solana.NewComputeBudgetInstruction(400_000)

		tx, err := s.Solana.Chain.NewTransactionFromInstructions(
			randomWallet.PublicKey(),
			computeBudgetIx,
			claimIx,
		)
		s.Require().NoError(err)

		_, err = s.Solana.Chain.SignAndBroadcastTxWithRetry(ctx, tx, rpc.CommitmentConfirmed, randomWallet)
		s.Require().NoError(err, "claim should succeed when called by a random non-admin wallet")
	}))

	// Verify: AM-B admin can upgrade the target program
	s.Require().True(s.Run("Grant ADMIN_ROLE on AM-B and verify upgrade works", func() {
		amBAccessManagerPDA, _ := solana.AccessManager.AccessManagerPDA(amBProgramID)

		// Grant ADMIN_ROLE on AM-B to upgrader wallet
		grantIx, err := withAMProgramID(amBProgramID, func() (solanago.Instruction, error) {
			return access_manager.NewGrantRoleInstruction(
				ADMIN_ROLE,
				s.UpgraderWallet.PublicKey(),
				amBAccessManagerPDA,
				s.SolanaRelayer.PublicKey(),
				solanago.SysVarInstructionsPubkey,
			)
		})
		s.Require().NoError(err)

		tx, err := s.Solana.Chain.NewTransactionFromInstructions(s.SolanaRelayer.PublicKey(), grantIx)
		s.Require().NoError(err)

		_, err = s.Solana.Chain.SignAndBroadcastTxWithRetry(ctx, tx, rpc.CommitmentConfirmed, s.SolanaRelayer)
		s.Require().NoError(err, "failed to grant ADMIN_ROLE on AM-B")

		// Write buffer and transfer authority to AM-B's PDA
		bufferAccount, err := solana.WriteProgramBuffer(ctx, programSoFile, deployerPath, s.Solana.Chain.RPCURL)
		s.Require().NoError(err, "failed to write program buffer")

		err = solana.SetBufferAuthority(ctx, bufferAccount, amBUpgradeAuthorityPDA, deployerPath, s.Solana.Chain.RPCURL)
		s.Require().NoError(err, "failed to set buffer authority to AM-B's PDA")

		// Upgrade via AM-B
		upgradeIx, err := withAMProgramID(amBProgramID, func() (solanago.Instruction, error) {
			return access_manager.NewUpgradeProgramInstruction(
				targetProgramID,
				amBAccessManagerPDA,
				targetProgramID,
				programDataAccount,
				bufferAccount,
				amBUpgradeAuthorityPDA,
				s.UpgraderWallet.PublicKey(),
				s.UpgraderWallet.PublicKey(),
				solanago.SysVarInstructionsPubkey,
				solanago.BPFLoaderUpgradeableProgramID,
				solanago.SysVarRentPubkey,
				solanago.SysVarClockPubkey,
			)
		})
		s.Require().NoError(err, "failed to build AM-B upgrade instruction")

		computeBudgetIx := solana.NewComputeBudgetInstruction(400_000)

		tx, err = s.Solana.Chain.NewTransactionFromInstructions(
			s.UpgraderWallet.PublicKey(),
			computeBudgetIx,
			upgradeIx,
		)
		s.Require().NoError(err)

		_, err = s.Solana.Chain.SignAndBroadcastTxWithRetry(ctx, tx, rpc.CommitmentConfirmed, s.UpgraderWallet)
		s.Require().NoError(err, "upgrade via AM-B should succeed after claiming authority")
	}))

	// Verify: AM-A can no longer upgrade
	s.Require().True(s.Run("AM-A can no longer upgrade the target program", func() {
		bufferAccount, err := solana.WriteProgramBuffer(ctx, programSoFile, deployerPath, s.Solana.Chain.RPCURL)
		s.Require().NoError(err)

		err = solana.SetBufferAuthority(ctx, bufferAccount, amAUpgradeAuthorityPDA, deployerPath, s.Solana.Chain.RPCURL)
		s.Require().NoError(err)

		accessControlAccount, _ := solana.AccessManager.AccessManagerPDA(access_manager.ProgramID)

		upgradeIx, err := access_manager.NewUpgradeProgramInstruction(
			targetProgramID,
			accessControlAccount,
			targetProgramID,
			programDataAccount,
			bufferAccount,
			amAUpgradeAuthorityPDA,
			s.UpgraderWallet.PublicKey(),
			s.UpgraderWallet.PublicKey(),
			solanago.SysVarInstructionsPubkey,
			solanago.BPFLoaderUpgradeableProgramID,
			solanago.SysVarRentPubkey,
			solanago.SysVarClockPubkey,
		)
		s.Require().NoError(err)

		computeBudgetIx := solana.NewComputeBudgetInstruction(400_000)

		tx, err := s.Solana.Chain.NewTransactionFromInstructions(
			s.UpgraderWallet.PublicKey(),
			computeBudgetIx,
			upgradeIx,
		)
		s.Require().NoError(err)

		_, err = s.Solana.Chain.SignAndBroadcastTxWithOpts(ctx, tx, rpc.ConfirmationStatusConfirmed, s.UpgraderWallet)
		s.Require().Error(err, "AM-A upgrade should fail after authority was migrated to AM-B")
	}))
}
