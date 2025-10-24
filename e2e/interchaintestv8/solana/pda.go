package solana

import (
	"crypto/sha256"
	"encoding/binary"

	solanago "github.com/gagliardetto/solana-go"

	ics07_tendermint "github.com/cosmos/solidity-ibc-eureka/packages/go-anchor/ics07tendermint"
	ics26_router "github.com/cosmos/solidity-ibc-eureka/packages/go-anchor/ics26router"
	ics27_gmp "github.com/cosmos/solidity-ibc-eureka/packages/go-anchor/ics27gmp"
)

// Router (ICS26) PDA helpers

func RouterStatePDA() (solanago.PublicKey, uint8, error) {
	return solanago.FindProgramAddress(
		[][]byte{[]byte("router_state")},
		ics26_router.ProgramID,
	)
}

func RouterIBCAppPDA(portID string) (solanago.PublicKey, uint8, error) {
	return solanago.FindProgramAddress(
		[][]byte{[]byte("ibc_app"), []byte(portID)},
		ics26_router.ProgramID,
	)
}

func RouterClientPDA(clientID string) (solanago.PublicKey, uint8, error) {
	return solanago.FindProgramAddress(
		[][]byte{[]byte("client"), []byte(clientID)},
		ics26_router.ProgramID,
	)
}

func RouterClientSequencePDA(clientID string) (solanago.PublicKey, uint8, error) {
	return solanago.FindProgramAddress(
		[][]byte{[]byte("client_sequence"), []byte(clientID)},
		ics26_router.ProgramID,
	)
}

func RouterPacketCommitmentPDA(clientID string, sequence uint64) (solanago.PublicKey, uint8, error) {
	sequenceBytes := make([]byte, 8)
	binary.BigEndian.PutUint64(sequenceBytes, sequence)
	return solanago.FindProgramAddress(
		[][]byte{[]byte("packet_commitment"), []byte(clientID), sequenceBytes},
		ics26_router.ProgramID,
	)
}

func RouterClientsPDA(clientID string) (solanago.PublicKey, uint8, error) {
	return solanago.FindProgramAddress(
		[][]byte{[]byte("clients"), []byte(clientID)},
		ics26_router.ProgramID,
	)
}

// ICS27 GMP PDA helpers

func GMPAppStatePDA(portID string) (solanago.PublicKey, uint8, error) {
	return solanago.FindProgramAddress(
		[][]byte{[]byte("app_state"), []byte(portID)},
		ics27_gmp.ProgramID,
	)
}

func GMPRouterCallerPDA() (solanago.PublicKey, uint8, error) {
	return solanago.FindProgramAddress(
		[][]byte{[]byte("router_caller")},
		ics27_gmp.ProgramID,
	)
}

func GMPAccountPDA(clientID string, sender string, salt []byte) (solanago.PublicKey, uint8, error) {
	hasher := sha256.New()
	hasher.Write([]byte(sender))
	senderHash := hasher.Sum(nil)

	return solanago.FindProgramAddress(
		[][]byte{
			[]byte("gmp_account"),
			[]byte(clientID),
			senderHash,
			salt,
		},
		ics27_gmp.ProgramID,
	)
}

// GMP Counter App PDA helpers

func CounterAppStatePDA(gmpCounterProgramID solanago.PublicKey) (solanago.PublicKey, uint8, error) {
	return solanago.FindProgramAddress(
		[][]byte{[]byte("counter_app_state")},
		gmpCounterProgramID,
	)
}

func CounterUserCounterPDA(ics27AccountPDA solanago.PublicKey, gmpCounterProgramID solanago.PublicKey) (solanago.PublicKey, uint8, error) {
	return solanago.FindProgramAddress(
		[][]byte{[]byte("user_counter"), ics27AccountPDA.Bytes()},
		gmpCounterProgramID,
	)
}

// Dummy App PDA helpers

func DummyAppStatePDA(portID string, dummyAppProgramID solanago.PublicKey) (solanago.PublicKey, uint8, error) {
	return solanago.FindProgramAddress(
		[][]byte{[]byte("app_state"), []byte(portID)},
		dummyAppProgramID,
	)
}

func DummyAppRouterCallerPDA(dummyAppProgramID solanago.PublicKey) (solanago.PublicKey, uint8, error) {
	return solanago.FindProgramAddress(
		[][]byte{[]byte("router_caller")},
		dummyAppProgramID,
	)
}

func DummyAppEscrowPDA(clientID string, dummyAppProgramID solanago.PublicKey) (solanago.PublicKey, uint8, error) {
	return solanago.FindProgramAddress(
		[][]byte{[]byte("escrow"), []byte(clientID)},
		dummyAppProgramID,
	)
}

func DummyAppEscrowStatePDA(clientID string, dummyAppProgramID solanago.PublicKey) (solanago.PublicKey, uint8, error) {
	return solanago.FindProgramAddress(
		[][]byte{[]byte("escrow_state"), []byte(clientID)},
		dummyAppProgramID,
	)
}

// ICS07 Tendermint PDA helpers

func TendermintClientStatePDA(chainID string) (solanago.PublicKey, uint8, error) {
	return solanago.FindProgramAddress(
		[][]byte{[]byte("client"), []byte(chainID)},
		ics07_tendermint.ProgramID,
	)
}

// Address Lookup Table PDA helpers

func AddressLookupTablePDA(authority solanago.PublicKey, slot uint64) (solanago.PublicKey, uint8, error) {
	slotBytes := make([]byte, 8)
	binary.LittleEndian.PutUint64(slotBytes, slot)
	return solanago.FindProgramAddress(
		[][]byte{authority.Bytes(), slotBytes},
		solanago.AddressLookupTableProgramID,
	)
}
