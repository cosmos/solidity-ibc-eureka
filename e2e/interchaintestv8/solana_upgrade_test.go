package main

import (
	"context"
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
