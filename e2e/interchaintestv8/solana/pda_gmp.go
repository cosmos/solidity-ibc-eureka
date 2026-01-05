package solana

// GMP PDA helpers - manually maintained because the GMP account PDA
// requires SHA256 hashing of sender address, which can't be auto-generated.

import (
	"crypto/sha256"
	"fmt"

	solanago "github.com/gagliardetto/solana-go"
)

func (ics27GmpPDAs) GmpAccountPDA(programID solanago.PublicKey, clientId []byte, sender []byte, salt []byte) (solanago.PublicKey, uint8) {
	senderHash := sha256.Sum256(sender)
	pda, bump, err := solanago.FindProgramAddress(
		[][]byte{[]byte("gmp_account"), clientId, senderHash[:], salt},
		programID,
	)
	if err != nil {
		panic(fmt.Sprintf("failed to derive Ics27Gmp.GmpAccountPDA PDA: %v", err))
	}
	return pda, bump
}
