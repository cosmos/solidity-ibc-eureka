package solana

import (
	"encoding/hex"
	"fmt"

	"github.com/gagliardetto/solana-go"

	"github.com/cosmos/interchaintest/v10/ibc"
)

var _ ibc.Wallet = &SolanaWallet{}

type SolanaWallet struct {
	keyName string
	address []byte // Store as bytes for compatibility
}

func NewWallet(keyName string, address []byte) *SolanaWallet {
	return &SolanaWallet{
		keyName: keyName,
		address: address,
	}
}

func (w *SolanaWallet) KeyName() string {
	return w.keyName
}

func (w *SolanaWallet) FormattedAddress() string {
	// Convert bytes to Solana base58 address
	pubkey := solana.PublicKey{}
	copy(pubkey[:], w.address)
	return pubkey.String()
}

func (w *SolanaWallet) Address() []byte {
	return w.address
}

func (w *SolanaWallet) Mnemonic() string {
	// Solana wallets created with solana-go don't have mnemonics
	// This would need BIP39 implementation
	return ""
}

func (w *SolanaWallet) FormattedAddressWithPrefix(prefix string) string {
	// Solana doesn't use prefixes like Cosmos
	return w.FormattedAddress()
}

func (w *SolanaWallet) Validate() error {
	if len(w.address) != 32 {
		return fmt.Errorf("invalid Solana address length: expected 32, got %d", len(w.address))
	}
	return nil
}

// Helper to convert hex string to Solana address
func AddressFromHex(hexStr string) ([]byte, error) {
	if len(hexStr) >= 2 && hexStr[:2] == "0x" {
		hexStr = hexStr[2:]
	}
	return hex.DecodeString(hexStr)
}
