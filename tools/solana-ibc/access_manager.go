package main

import (
	"fmt"
	"os"

	"github.com/spf13/cobra"

	solanago "github.com/gagliardetto/solana-go"

	access_manager "github.com/cosmos/solidity-ibc-eureka/packages/go-anchor/accessmanager"
)

var accessManagerCmd = &cobra.Command{
	Use:   "access-manager",
	Short: "Manage AccessManager roles and initialization",
}

var initializeCmd = &cobra.Command{
	Use:   "initialize <cluster-url> <payer-keypair> <admin-pubkey> <access-manager-program-id>",
	Short: "Initialize AccessManager with an admin",
	Args:  cobra.ExactArgs(4),
	Run: func(cmd *cobra.Command, args []string) {
		clusterURL := args[0]
		payerKeypairPath := args[1]
		adminPubkey := solanago.MustPublicKeyFromBase58(args[2])
		accessManagerProgramID := solanago.MustPublicKeyFromBase58(args[3])

		payerWallet := loadWallet(payerKeypairPath)

		accessManagerPda, _, _ := solanago.FindProgramAddress(
			[][]byte{[]byte("access_manager")},
			accessManagerProgramID,
		)

		initIx, err := access_manager.NewInitializeInstruction(
			adminPubkey,
			accessManagerPda,
			payerWallet.PublicKey(),
			solanago.SystemProgramID,
			solanago.SysVarInstructionsPubkey,
		)
		if err != nil {
			fmt.Fprintf(os.Stderr, "Error building initialize instruction: %v\n", err)
			os.Exit(1)
		}

		fmt.Printf("Initializing AccessManager with admin %s...\n", adminPubkey)

		sig := sendTransaction(clusterURL, payerWallet, []solanago.Instruction{initIx})

		fmt.Printf("✅ Transaction sent: %s\n", sig)
		fmt.Println("Waiting for confirmation...")

		if waitForConfirmation(clusterURL, sig) {
			fmt.Printf("✅ AccessManager initialized with admin: %s\n", adminPubkey)
			fmt.Printf("   AccessManager PDA: %s\n", accessManagerPda)
		}
	},
}

var grantCmd = &cobra.Command{
	Use:   "grant <cluster-url> <admin-keypair> <role-id> <account-pubkey> <access-manager-program-id>",
	Short: "Grant a role to an account",
	Args:  cobra.ExactArgs(5),
	Run: func(cmd *cobra.Command, args []string) {
		clusterURL := args[0]
		adminKeypairPath := args[1]
		roleID := parseRoleID(args[2])
		accountPubkey := solanago.MustPublicKeyFromBase58(args[3])
		accessManagerProgramID := solanago.MustPublicKeyFromBase58(args[4])

		adminWallet := loadWallet(adminKeypairPath)

		accessManagerPda, _, _ := solanago.FindProgramAddress(
			[][]byte{[]byte("access_manager")},
			accessManagerProgramID,
		)

		grantRoleIx, err := access_manager.NewGrantRoleInstruction(
			roleID,
			accountPubkey,
			accessManagerPda,
			adminWallet.PublicKey(),
			solanago.SysVarInstructionsPubkey,
		)
		if err != nil {
			fmt.Fprintf(os.Stderr, "Error building grant role instruction: %v\n", err)
			os.Exit(1)
		}

		fmt.Printf("Granting role %d to %s...\n", roleID, accountPubkey)

		sig := sendTransaction(clusterURL, adminWallet, []solanago.Instruction{grantRoleIx})

		fmt.Printf("✅ Transaction sent: %s\n", sig)
		fmt.Println("Waiting for confirmation...")

		if waitForConfirmation(clusterURL, sig) {
			fmt.Printf("✅ Role %d granted to %s\n", roleID, accountPubkey)
		}
	},
}

var revokeCmd = &cobra.Command{
	Use:   "revoke <cluster-url> <admin-keypair> <role-id> <account-pubkey> <access-manager-program-id>",
	Short: "Revoke a role from an account",
	Args:  cobra.ExactArgs(5),
	Run: func(cmd *cobra.Command, args []string) {
		clusterURL := args[0]
		adminKeypairPath := args[1]
		roleID := parseRoleID(args[2])
		accountPubkey := solanago.MustPublicKeyFromBase58(args[3])
		accessManagerProgramID := solanago.MustPublicKeyFromBase58(args[4])

		adminWallet := loadWallet(adminKeypairPath)

		accessManagerPda, _, _ := solanago.FindProgramAddress(
			[][]byte{[]byte("access_manager")},
			accessManagerProgramID,
		)

		revokeRoleIx, err := access_manager.NewRevokeRoleInstruction(
			roleID,
			accountPubkey,
			accessManagerPda,
			adminWallet.PublicKey(),
			solanago.SysVarInstructionsPubkey,
		)
		if err != nil {
			fmt.Fprintf(os.Stderr, "Error building revoke role instruction: %v\n", err)
			os.Exit(1)
		}

		fmt.Printf("Revoking role %d from %s...\n", roleID, accountPubkey)

		sig := sendTransaction(clusterURL, adminWallet, []solanago.Instruction{revokeRoleIx})

		fmt.Printf("✅ Transaction sent: %s\n", sig)
		fmt.Println("Waiting for confirmation...")

		if waitForConfirmation(clusterURL, sig) {
			fmt.Printf("✅ Role %d revoked from %s\n", roleID, accountPubkey)
		}
	},
}

func init() {
	accessManagerCmd.AddCommand(initializeCmd)
	accessManagerCmd.AddCommand(grantCmd)
	accessManagerCmd.AddCommand(revokeCmd)
}
