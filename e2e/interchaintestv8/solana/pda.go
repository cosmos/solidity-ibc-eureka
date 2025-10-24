package solana

import (
	"crypto/sha256"
	"encoding/binary"
	"fmt"

	solanago "github.com/gagliardetto/solana-go"

	ics07_tendermint "github.com/cosmos/solidity-ibc-eureka/packages/go-anchor/ics07tendermint"
	ics26_router "github.com/cosmos/solidity-ibc-eureka/packages/go-anchor/ics26router"
	ics27_gmp "github.com/cosmos/solidity-ibc-eureka/packages/go-anchor/ics27gmp"
)

// Router (ICS26) PDA helpers

func RouterStatePDA() (solanago.PublicKey, uint8) {
	pda, bump, err := solanago.FindProgramAddress(
		[][]byte{[]byte("router_state")},
		ics26_router.ProgramID,
	)
	if err != nil {
		panic(fmt.Sprintf("failed to derive router state PDA: %v", err))
	}
	return pda, bump
}

func RouterIBCAppPDA(portID string) (solanago.PublicKey, uint8) {
	pda, bump, err := solanago.FindProgramAddress(
		[][]byte{[]byte("ibc_app"), []byte(portID)},
		ics26_router.ProgramID,
	)
	if err != nil {
		panic(fmt.Sprintf("failed to derive router IBC app PDA for port %s: %v", portID, err))
	}
	return pda, bump
}

func RouterClientPDA(clientID string) (solanago.PublicKey, uint8) {
	pda, bump, err := solanago.FindProgramAddress(
		[][]byte{[]byte("client"), []byte(clientID)},
		ics26_router.ProgramID,
	)
	if err != nil {
		panic(fmt.Sprintf("failed to derive router client PDA for client %s: %v", clientID, err))
	}
	return pda, bump
}

func RouterClientSequencePDA(clientID string) (solanago.PublicKey, uint8) {
	pda, bump, err := solanago.FindProgramAddress(
		[][]byte{[]byte("client_sequence"), []byte(clientID)},
		ics26_router.ProgramID,
	)
	if err != nil {
		panic(fmt.Sprintf("failed to derive router client sequence PDA for client %s: %v", clientID, err))
	}
	return pda, bump
}

func RouterPacketCommitmentPDA(clientID string, sequence uint64) (solanago.PublicKey, uint8) {
	sequenceBytes := make([]byte, 8)
	binary.LittleEndian.PutUint64(sequenceBytes, sequence)
	pda, bump, err := solanago.FindProgramAddress(
		[][]byte{[]byte("packet_commitment"), []byte(clientID), sequenceBytes},
		ics26_router.ProgramID,
	)
	if err != nil {
		panic(fmt.Sprintf("failed to derive router packet commitment PDA for client %s sequence %d: %v", clientID, sequence, err))
	}
	return pda, bump
}

func RouterClientsPDA(clientID string) (solanago.PublicKey, uint8) {
	pda, bump, err := solanago.FindProgramAddress(
		[][]byte{[]byte("clients"), []byte(clientID)},
		ics26_router.ProgramID,
	)
	if err != nil {
		panic(fmt.Sprintf("failed to derive router clients PDA for client %s: %v", clientID, err))
	}
	return pda, bump
}

// ICS27 GMP PDA helpers

func GMPAppStatePDA(portID string) (solanago.PublicKey, uint8) {
	pda, bump, err := solanago.FindProgramAddress(
		[][]byte{[]byte("app_state"), []byte(portID)},
		ics27_gmp.ProgramID,
	)
	if err != nil {
		panic(fmt.Sprintf("failed to derive GMP app state PDA for port %s: %v", portID, err))
	}
	return pda, bump
}

func GMPRouterCallerPDA() (solanago.PublicKey, uint8) {
	pda, bump, err := solanago.FindProgramAddress(
		[][]byte{[]byte("router_caller")},
		ics27_gmp.ProgramID,
	)
	if err != nil {
		panic(fmt.Sprintf("failed to derive GMP router caller PDA: %v", err))
	}
	return pda, bump
}

func GMPAccountPDA(clientID string, sender string, salt []byte) (solanago.PublicKey, uint8) {
	hasher := sha256.New()
	hasher.Write([]byte(sender))
	senderHash := hasher.Sum(nil)

	pda, bump, err := solanago.FindProgramAddress(
		[][]byte{
			[]byte("gmp_account"),
			[]byte(clientID),
			senderHash,
			salt,
		},
		ics27_gmp.ProgramID,
	)
	if err != nil {
		panic(fmt.Sprintf("failed to derive GMP account PDA for client %s sender %s: %v", clientID, sender, err))
	}
	return pda, bump
}

// GMP Counter App PDA helpers

func CounterAppStatePDA(gmpCounterProgramID solanago.PublicKey) (solanago.PublicKey, uint8) {
	pda, bump, err := solanago.FindProgramAddress(
		[][]byte{[]byte("counter_app_state")},
		gmpCounterProgramID,
	)
	if err != nil {
		panic(fmt.Sprintf("failed to derive counter app state PDA: %v", err))
	}
	return pda, bump
}

func CounterUserCounterPDA(ics27AccountPDA solanago.PublicKey, gmpCounterProgramID solanago.PublicKey) (solanago.PublicKey, uint8) {
	pda, bump, err := solanago.FindProgramAddress(
		[][]byte{[]byte("user_counter"), ics27AccountPDA.Bytes()},
		gmpCounterProgramID,
	)
	if err != nil {
		panic(fmt.Sprintf("failed to derive counter user counter PDA: %v", err))
	}
	return pda, bump
}

// Dummy App PDA helpers

func DummyAppStatePDA(portID string, dummyAppProgramID solanago.PublicKey) (solanago.PublicKey, uint8) {
	pda, bump, err := solanago.FindProgramAddress(
		[][]byte{[]byte("app_state"), []byte(portID)},
		dummyAppProgramID,
	)
	if err != nil {
		panic(fmt.Sprintf("failed to derive dummy app state PDA for port %s: %v", portID, err))
	}
	return pda, bump
}

func DummyAppRouterCallerPDA(dummyAppProgramID solanago.PublicKey) (solanago.PublicKey, uint8) {
	pda, bump, err := solanago.FindProgramAddress(
		[][]byte{[]byte("router_caller")},
		dummyAppProgramID,
	)
	if err != nil {
		panic(fmt.Sprintf("failed to derive dummy app router caller PDA: %v", err))
	}
	return pda, bump
}

func DummyAppEscrowPDA(clientID string, dummyAppProgramID solanago.PublicKey) (solanago.PublicKey, uint8) {
	pda, bump, err := solanago.FindProgramAddress(
		[][]byte{[]byte("escrow"), []byte(clientID)},
		dummyAppProgramID,
	)
	if err != nil {
		panic(fmt.Sprintf("failed to derive dummy app escrow PDA for client %s: %v", clientID, err))
	}
	return pda, bump
}

func DummyAppEscrowStatePDA(clientID string, dummyAppProgramID solanago.PublicKey) (solanago.PublicKey, uint8) {
	pda, bump, err := solanago.FindProgramAddress(
		[][]byte{[]byte("escrow_state"), []byte(clientID)},
		dummyAppProgramID,
	)
	if err != nil {
		panic(fmt.Sprintf("failed to derive dummy app escrow state PDA for client %s: %v", clientID, err))
	}
	return pda, bump
}

// ICS07 Tendermint PDA helpers

func TendermintClientStatePDA(chainID string) (solanago.PublicKey, uint8) {
	pda, bump, err := solanago.FindProgramAddress(
		[][]byte{[]byte("client"), []byte(chainID)},
		ics07_tendermint.ProgramID,
	)
	if err != nil {
		panic(fmt.Sprintf("failed to derive Tendermint client state PDA for chain %s: %v", chainID, err))
	}
	return pda, bump
}

// Address Lookup Table PDA helpers

func AddressLookupTablePDA(authority solanago.PublicKey, slot uint64) (solanago.PublicKey, uint8) {
	slotBytes := make([]byte, 8)
	binary.LittleEndian.PutUint64(slotBytes, slot)
	pda, bump, err := solanago.FindProgramAddress(
		[][]byte{authority.Bytes(), slotBytes},
		solanago.AddressLookupTableProgramID,
	)
	if err != nil {
		panic(fmt.Sprintf("failed to derive address lookup table PDA for authority %s slot %d: %v", authority, slot, err))
	}
	return pda, bump
}
