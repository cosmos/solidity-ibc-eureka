package main

import (
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"errors"
	"fmt"
	"os"
	"path/filepath"
	"strconv"
	"time"

	"github.com/cometbft/cometbft/crypto/secp256k1eth"
	cmtbytes "github.com/cometbft/cometbft/libs/bytes"
	cmtmath "github.com/cometbft/cometbft/libs/math"
	"github.com/cometbft/cometbft/light"
	cmtproto "github.com/cometbft/cometbft/proto/tendermint/types"
	cmtversion "github.com/cometbft/cometbft/proto/tendermint/version"
	cmttypes "github.com/cometbft/cometbft/types"
	"github.com/cometbft/cometbft/version"
	"github.com/decred/dcrd/dcrec/secp256k1/v4"
)

type fixture struct {
	ChainID              string         `json:"chainId"`
	TrustedHeight        uint64         `json:"trustedHeight"`
	RevisionNumber       uint64         `json:"revisionNumber"`
	TrustingPeriod       uint32         `json:"trustingPeriod"`
	UnbondingPeriod      uint32         `json:"unbondingPeriod"`
	MaxClockDrift        uint32         `json:"maxClockDrift"`
	ProofNow             uint64         `json:"proofNow"`
	TrustedConsensus     consensusJSON  `json:"trustedConsensusState"`
	Header               headerJSON     `json:"header"`
	Commit               commitJSON     `json:"commit"`
	TrustedValidators    validatorsJSON `json:"trustedValidators"`
	Validators           validatorsJSON `json:"validators"`
	NextValidators       validatorsJSON `json:"nextValidators"`
	Expected             expectedJSON   `json:"expected"`
	CometBFTVerification string         `json:"cometbftVerification"`
}

type consensusJSON struct {
	Timestamp          uint64 `json:"timestamp"`
	Root               string `json:"root"`
	NextValidatorsHash string `json:"nextValidatorsHash"`
}

type partSetHeaderJSON struct {
	Total uint32 `json:"total"`
	Hash  string `json:"hash"`
}

type blockIDJSON struct {
	Hash          string            `json:"hash"`
	PartSetHeader partSetHeaderJSON `json:"partSetHeader"`
}

type headerJSON struct {
	VersionBlock       uint64      `json:"versionBlock"`
	VersionApp         uint64      `json:"versionApp"`
	ChainID            string      `json:"chainId"`
	Height             uint64      `json:"height"`
	TimeSeconds        uint64      `json:"timeSeconds"`
	TimeNanos          uint32      `json:"timeNanos"`
	LastBlockID        blockIDJSON `json:"lastBlockId"`
	LastCommitHash     string      `json:"lastCommitHash"`
	DataHash           string      `json:"dataHash"`
	ValidatorsHash     string      `json:"validatorsHash"`
	NextValidatorsHash string      `json:"nextValidatorsHash"`
	ConsensusHash      string      `json:"consensusHash"`
	AppHash            string      `json:"appHash"`
	LastResultsHash    string      `json:"lastResultsHash"`
	EvidenceHash       string      `json:"evidenceHash"`
	ProposerAddress    string      `json:"proposerAddress"`
}

type commitJSON struct {
	Height             uint64      `json:"height"`
	Round              uint32      `json:"round"`
	BlockID            blockIDJSON `json:"blockId"`
	BlockIDFlags       []uint64    `json:"blockIdFlags"`
	ValidatorAddresses []string    `json:"validatorAddresses"`
	TimestampSeconds   []uint64    `json:"timestampSeconds"`
	TimestampNanos     []uint32    `json:"timestampNanos"`
	Signatures         []string    `json:"signatures"`
}

type validatorsJSON struct {
	Addresses              []string `json:"addresses"`
	PublicKeys             []string `json:"publicKeys"`
	PublicKeyYWitnesses    []string `json:"publicKeyYWitnesses"`
	UncompressedPublicKeys []string `json:"uncompressedPublicKeys"`
	VotingPowers           []uint64 `json:"votingPowers"`
}

type expectedJSON struct {
	ValidatorSetHash         string   `json:"validatorSetHash"`
	NextValidatorSetHash     string   `json:"nextValidatorSetHash"`
	TrustedValidatorSetHash  string   `json:"trustedValidatorSetHash"`
	TrustedHeaderHash        string   `json:"trustedHeaderHash"`
	HeaderHash               string   `json:"headerHash"`
	VoteSignBytes            []string `json:"voteSignBytes"`
	RecoveredSigners         []string `json:"recoveredSigners"`
	TrustedSignedVotingPower uint64   `json:"trustedSignedVotingPower"`
	TrustedVotingPowerNeeded uint64   `json:"trustedVotingPowerNeeded"`
	NewSignedVotingPower     uint64   `json:"newSignedVotingPower"`
	NewVotingPowerNeeded     uint64   `json:"newVotingPowerNeeded"`
}

type misbehaviourFixture struct {
	ChainID          string               `json:"chainId"`
	TrustedHeight    uint64               `json:"trustedHeight"`
	RevisionNumber   uint64               `json:"revisionNumber"`
	TrustingPeriod   uint32               `json:"trustingPeriod"`
	UnbondingPeriod  uint32               `json:"unbondingPeriod"`
	MaxClockDrift    uint32               `json:"maxClockDrift"`
	ProofNow         uint64               `json:"proofNow"`
	TrustedConsensus consensusJSON        `json:"trustedConsensusState"`
	DoubleSign       misbehaviourUpdate   `json:"doubleSign"`
	TimeViolation    misbehaviourUpdate   `json:"timeViolation"`
	Validators       validatorsJSON       `json:"validators"`
	Expected         misbehaviourExpected `json:"expected"`
}

type misbehaviourUpdate struct {
	TrustedHeight    uint64        `json:"trustedHeight"`
	TrustedConsensus consensusJSON `json:"trustedConsensusState"`
	Header           headerJSON    `json:"header"`
	Commit           commitJSON    `json:"commit"`
}

type misbehaviourExpected struct {
	DoubleSignHeaderHash    string `json:"doubleSignHeaderHash"`
	TimeViolationHeaderHash string `json:"timeViolationHeaderHash"`
}

func main() {
	if len(os.Args) > 1 && os.Args[1] == "misbehaviour" {
		out := filepath.Join("..", "..", "test", "cometbft", "fixtures", "native_misbehaviour_fixture.json")
		if len(os.Args) > 2 {
			out = os.Args[2]
		}
		fix, err := buildMisbehaviourFixture(3)
		if err != nil {
			panic(err)
		}
		writeMisbehaviourFixture(out, fix)
		return
	}

	if len(os.Args) > 1 && os.Args[1] == "skip" {
		out := filepath.Join("..", "..", "test", "cometbft", "fixtures", "native_skipping_update_fixture.json")
		count := 3
		insufficientTrusted := false
		if len(os.Args) > 2 {
			out = os.Args[2]
		}
		if len(os.Args) > 3 {
			var err error
			count, err = strconv.Atoi(os.Args[3])
			if err != nil || count < 1 {
				panic("validator count must be a positive integer")
			}
		}
		if len(os.Args) > 4 {
			insufficientTrusted = os.Args[4] == "insufficient-trusted"
		}

		fix, err := buildSkippingFixture(count, insufficientTrusted)
		if err != nil {
			panic(err)
		}
		writeFixture(out, fix)
		return
	}

	if len(os.Args) > 1 && os.Args[1] == "skip-next" {
		out := filepath.Join("..", "..", "test", "cometbft", "fixtures", "native_skipping_next_update_fixture.json")
		count := 3
		if len(os.Args) > 2 {
			out = os.Args[2]
		}
		if len(os.Args) > 3 {
			var err error
			count, err = strconv.Atoi(os.Args[3])
			if err != nil || count < 1 {
				panic("validator count must be a positive integer")
			}
		}

		fix, err := buildStoredChainedSkippingFixture(count)
		if err != nil {
			panic(err)
		}
		writeFixture(out, fix)
		return
	}

	if len(os.Args) > 1 {
		count := 3
		if len(os.Args) > 2 {
			var err error
			count, err = strconv.Atoi(os.Args[2])
			if err != nil || count < 1 {
				panic("validator count must be a positive integer")
			}
		}

		fix, err := buildFixture(count)
		if err != nil {
			panic(err)
		}
		writeFixture(os.Args[1], fix)
		return
	}

	for _, spec := range []struct {
		out   string
		count int
	}{
		{out: filepath.Join("..", "..", "test", "cometbft", "fixtures", "native_update_fixture.json"), count: 3},
		{out: filepath.Join("..", "..", "test", "cometbft", "fixtures", "native_update_20_validators_fixture.json"), count: 20},
	} {
		fix, err := buildFixture(spec.count)
		if err != nil {
			panic(err)
		}
		writeFixture(spec.out, fix)
	}

	for _, spec := range []struct {
		out                 string
		count               int
		insufficientTrusted bool
	}{
		{
			out:   filepath.Join("..", "..", "test", "cometbft", "fixtures", "native_skipping_update_fixture.json"),
			count: 3,
		},
		{
			out:   filepath.Join("..", "..", "test", "cometbft", "fixtures", "native_skipping_update_20_validators_fixture.json"),
			count: 20,
		},
		{
			out:   filepath.Join("..", "..", "test", "cometbft", "fixtures", "native_skipping_next_update_fixture.json"),
			count: 3,
		},
		{
			out: filepath.Join(
				"..",
				"..",
				"test",
				"cometbft",
				"fixtures",
				"native_skipping_insufficient_trusted_overlap_fixture.json",
			),
			count:               3,
			insufficientTrusted: true,
		},
	} {
		var fix fixture
		var err error
		if filepath.Base(spec.out) == "native_skipping_next_update_fixture.json" {
			fix, err = buildStoredChainedSkippingFixture(spec.count)
		} else {
			fix, err = buildSkippingFixture(spec.count, spec.insufficientTrusted)
		}
		if err != nil {
			panic(err)
		}
		writeFixture(spec.out, fix)
	}

	fix, err := buildMisbehaviourFixture(3)
	if err != nil {
		panic(err)
	}
	writeMisbehaviourFixture(filepath.Join("..", "..", "test", "cometbft", "fixtures", "native_misbehaviour_fixture.json"), fix)
}

func writeFixture(out string, fix fixture) {
	bz, err := json.MarshalIndent(fix, "", "  ")
	if err != nil {
		panic(err)
	}
	bz = append(bz, '\n')

	if err := os.MkdirAll(filepath.Dir(out), 0o755); err != nil {
		panic(err)
	}
	if err := os.WriteFile(out, bz, 0o644); err != nil {
		panic(err)
	}
	fmt.Println(out)
}

func writeMisbehaviourFixture(out string, fix misbehaviourFixture) {
	bz, err := json.MarshalIndent(fix, "", "  ")
	if err != nil {
		panic(err)
	}
	bz = append(bz, '\n')

	if err := os.MkdirAll(filepath.Dir(out), 0o755); err != nil {
		panic(err)
	}
	if err := os.WriteFile(out, bz, 0o644); err != nil {
		panic(err)
	}
	fmt.Println(out)
}

func buildMisbehaviourFixture(validatorCount int) (misbehaviourFixture, error) {
	const chainID = "native-cometbft-1"
	const trustingPeriod = 14 * 24 * 60 * 60
	const unbondingPeriod = 21 * 24 * 60 * 60
	const maxClockDrift = 30

	trustedTime := time.Unix(1_680_220_500, 123_000_000).UTC()
	headerTime := time.Unix(1_680_220_600, 456_000_000).UTC()
	now := time.Unix(1_680_220_800, 0).UTC()

	type pv struct {
		priv secp256k1eth.PrivKey
		val  *cmttypes.Validator
	}
	privVals := []pv{}
	for i := 0; i < validatorCount; i++ {
		seed := fmt.Sprintf("native-cometbft-validator-%d", i+1)
		priv := secp256k1eth.GenPrivKeySecp256k1Eth([]byte(seed))
		val := cmttypes.NewValidator(priv.PubKey(), int64((i+1)*10))
		privVals = append(privVals, pv{priv: priv, val: val})
	}

	validators := make([]*cmttypes.Validator, len(privVals))
	for i, pv := range privVals {
		validators[i] = pv.val
	}
	valSet := cmttypes.NewValidatorSet(validators)
	privByAddress := map[string]secp256k1eth.PrivKey{}
	for _, pv := range privVals {
		privByAddress[string(pv.val.Address)] = pv.priv
	}

	valHash := valSet.Hash()
	trustedHeader := &cmttypes.Header{
		Version:            cmtversion.Consensus{Block: version.BlockProtocol, App: 1},
		ChainID:            chainID,
		Height:             1,
		Time:               trustedTime,
		LastBlockID:        cmttypes.BlockID{},
		LastCommitHash:     hashBytes("trusted-last-commit"),
		DataHash:           hashBytes("trusted-data"),
		ValidatorsHash:     valHash,
		NextValidatorsHash: valHash,
		ConsensusHash:      hashBytes("trusted-consensus"),
		AppHash:            hashBytes("trusted-app"),
		LastResultsHash:    hashBytes("trusted-results"),
		EvidenceHash:       hashBytes("trusted-evidence"),
		ProposerAddress:    valSet.Proposer.Address,
	}
	if err := trustedHeader.ValidateBasic(); err != nil {
		return misbehaviourFixture{}, fmt.Errorf("trusted header invalid: %w", err)
	}

	trustedBlockID := cmttypes.BlockID{
		Hash: trustedHeader.Hash(), PartSetHeader: cmttypes.PartSetHeader{Total: 1, Hash: hashBytes("trusted-part-set")},
	}
	baseHeader := &cmttypes.Header{
		Version:            cmtversion.Consensus{Block: version.BlockProtocol, App: 1},
		ChainID:            chainID,
		Height:             2,
		Time:               headerTime,
		LastBlockID:        trustedBlockID,
		LastCommitHash:     hashBytes("update-last-commit"),
		DataHash:           hashBytes("update-data"),
		ValidatorsHash:     valHash,
		NextValidatorsHash: valHash,
		ConsensusHash:      hashBytes("update-consensus"),
		AppHash:            hashBytes("update-app"),
		LastResultsHash:    hashBytes("update-results"),
		EvidenceHash:       hashBytes("update-evidence"),
		ProposerAddress:    valSet.Proposer.Address,
	}
	if err := baseHeader.ValidateBasic(); err != nil {
		return misbehaviourFixture{}, fmt.Errorf("base update header invalid: %w", err)
	}
	baseBlockID := cmttypes.BlockID{
		Hash: baseHeader.Hash(), PartSetHeader: cmttypes.PartSetHeader{Total: 1, Hash: hashBytes("update-part-set")},
	}
	_, _, _, err := signCommit(chainID, baseHeader, baseBlockID, headerTime, valSet, privByAddress)
	if err != nil {
		return misbehaviourFixture{}, fmt.Errorf("sign base update: %w", err)
	}

	doubleSignHeader := &cmttypes.Header{
		Version:            cmtversion.Consensus{Block: version.BlockProtocol, App: 1},
		ChainID:            chainID,
		Height:             2,
		Time:               headerTime,
		LastBlockID:        trustedBlockID,
		LastCommitHash:     hashBytes("double-sign-last-commit"),
		DataHash:           hashBytes("double-sign-data"),
		ValidatorsHash:     valHash,
		NextValidatorsHash: valHash,
		ConsensusHash:      hashBytes("double-sign-consensus"),
		AppHash:            hashBytes("double-sign-app"),
		LastResultsHash:    hashBytes("double-sign-results"),
		EvidenceHash:       hashBytes("double-sign-evidence"),
		ProposerAddress:    valSet.Proposer.Address,
	}
	if err := doubleSignHeader.ValidateBasic(); err != nil {
		return misbehaviourFixture{}, fmt.Errorf("double-sign header invalid: %w", err)
	}
	doubleSignBlockID := cmttypes.BlockID{
		Hash:          doubleSignHeader.Hash(),
		PartSetHeader: cmttypes.PartSetHeader{Total: 1, Hash: hashBytes("double-sign-part-set")},
	}
	doubleSignCommit, _, _, err := signCommit(chainID, doubleSignHeader, doubleSignBlockID, headerTime, valSet, privByAddress)
	if err != nil {
		return misbehaviourFixture{}, fmt.Errorf("sign double-sign header: %w", err)
	}

	timeViolationHeader := &cmttypes.Header{
		Version:            cmtversion.Consensus{Block: version.BlockProtocol, App: 1},
		ChainID:            chainID,
		Height:             3,
		Time:               headerTime,
		LastBlockID:        baseBlockID,
		LastCommitHash:     hashBytes("time-violation-last-commit"),
		DataHash:           hashBytes("time-violation-data"),
		ValidatorsHash:     valHash,
		NextValidatorsHash: valHash,
		ConsensusHash:      hashBytes("time-violation-consensus"),
		AppHash:            hashBytes("time-violation-app"),
		LastResultsHash:    hashBytes("time-violation-results"),
		EvidenceHash:       hashBytes("time-violation-evidence"),
		ProposerAddress:    valSet.Proposer.Address,
	}
	if err := timeViolationHeader.ValidateBasic(); err != nil {
		return misbehaviourFixture{}, fmt.Errorf("time-violation header invalid: %w", err)
	}
	timeViolationBlockID := cmttypes.BlockID{
		Hash:          timeViolationHeader.Hash(),
		PartSetHeader: cmttypes.PartSetHeader{Total: 1, Hash: hashBytes("time-violation-part-set")},
	}
	timeViolationCommit, _, _, err :=
		signCommit(chainID, timeViolationHeader, timeViolationBlockID, headerTime, valSet, privByAddress)
	if err != nil {
		return misbehaviourFixture{}, fmt.Errorf("sign time-violation header: %w", err)
	}

	addresses := make([]string, len(valSet.Validators))
	publicKeys := make([]string, len(valSet.Validators))
	publicKeyYWitnesses := make([]string, len(valSet.Validators))
	uncompressedPublicKeys := make([]string, len(valSet.Validators))
	powers := make([]uint64, len(valSet.Validators))
	for i, val := range valSet.Validators {
		addresses[i] = addressHex(val.Address)
		publicKeys[i] = hexBytes(val.PubKey.Bytes())
		uncompressed := uncompressedPubKey(privByAddress[string(val.Address)])
		publicKeyYWitnesses[i] = hexBytes(uncompressed[32:])
		uncompressedPublicKeys[i] = hexBytes(uncompressed)
		powers[i] = uint64(val.VotingPower)
	}

	trustedConsensus := consensusJSON{
		Timestamp:          timestampNanos(trustedTime),
		Root:               hexBytes(trustedHeader.AppHash),
		NextValidatorsHash: hexBytes(trustedHeader.NextValidatorsHash),
	}
	baseConsensus := consensusJSON{
		Timestamp:          timestampNanos(headerTime),
		Root:               hexBytes(baseHeader.AppHash),
		NextValidatorsHash: hexBytes(baseHeader.NextValidatorsHash),
	}

	return misbehaviourFixture{
		ChainID:          chainID,
		TrustedHeight:    1,
		RevisionNumber:   0,
		TrustingPeriod:   trustingPeriod,
		UnbondingPeriod:  unbondingPeriod,
		MaxClockDrift:    maxClockDrift,
		ProofNow:         uint64(now.Unix()),
		TrustedConsensus: trustedConsensus,
		DoubleSign: misbehaviourUpdate{
			TrustedHeight:    1,
			TrustedConsensus: trustedConsensus,
			Header:           headerJSONFromTypes(doubleSignHeader),
			Commit:           doubleSignCommit,
		},
		TimeViolation: misbehaviourUpdate{
			TrustedHeight:    2,
			TrustedConsensus: baseConsensus,
			Header:           headerJSONFromTypes(timeViolationHeader),
			Commit:           timeViolationCommit,
		},
		Validators: validatorsJSON{
			Addresses:              addresses,
			PublicKeys:             publicKeys,
			PublicKeyYWitnesses:    publicKeyYWitnesses,
			UncompressedPublicKeys: uncompressedPublicKeys,
			VotingPowers:           powers,
		},
		Expected: misbehaviourExpected{
			DoubleSignHeaderHash:    hexBytes(doubleSignHeader.Hash()),
			TimeViolationHeaderHash: hexBytes(timeViolationHeader.Hash()),
		},
	}, nil
}

func buildFixture(validatorCount int) (fixture, error) {
	const chainID = "native-cometbft-1"
	const trustingPeriod = 14 * 24 * 60 * 60
	const unbondingPeriod = 21 * 24 * 60 * 60
	const maxClockDrift = 30

	trustedTime := time.Unix(1_680_220_500, 123_000_000).UTC()
	headerTime := time.Unix(1_680_220_600, 456_000_000).UTC()
	now := time.Unix(1_680_220_800, 0).UTC()

	type pv struct {
		priv secp256k1eth.PrivKey
		val  *cmttypes.Validator
	}
	privVals := []pv{}
	for i := 0; i < validatorCount; i++ {
		seed := fmt.Sprintf("native-cometbft-validator-%d", i+1)
		priv := secp256k1eth.GenPrivKeySecp256k1Eth([]byte(seed))
		val := cmttypes.NewValidator(priv.PubKey(), int64((i+1)*10))
		privVals = append(privVals, pv{priv: priv, val: val})
	}

	validators := make([]*cmttypes.Validator, len(privVals))
	for i, pv := range privVals {
		validators[i] = pv.val
	}
	valSet := cmttypes.NewValidatorSet(validators)
	privByAddress := map[string]secp256k1eth.PrivKey{}
	for _, pv := range privVals {
		privByAddress[string(pv.val.Address)] = pv.priv
	}

	valHash := valSet.Hash()
	part1 := cmttypes.PartSetHeader{Total: 1, Hash: hashBytes("trusted-part-set")}
	trustedHeader := &cmttypes.Header{
		Version:            cmtversion.Consensus{Block: version.BlockProtocol, App: 1},
		ChainID:            chainID,
		Height:             1,
		Time:               trustedTime,
		LastBlockID:        cmttypes.BlockID{},
		LastCommitHash:     hashBytes("trusted-last-commit"),
		DataHash:           hashBytes("trusted-data"),
		ValidatorsHash:     valHash,
		NextValidatorsHash: valHash,
		ConsensusHash:      hashBytes("trusted-consensus"),
		AppHash:            hashBytes("trusted-app"),
		LastResultsHash:    hashBytes("trusted-results"),
		EvidenceHash:       hashBytes("trusted-evidence"),
		ProposerAddress:    valSet.Proposer.Address,
	}
	if err := trustedHeader.ValidateBasic(); err != nil {
		return fixture{}, fmt.Errorf("trusted header invalid: %w", err)
	}

	trustedBlockID := cmttypes.BlockID{Hash: trustedHeader.Hash(), PartSetHeader: part1}
	part2 := cmttypes.PartSetHeader{Total: 1, Hash: hashBytes("update-part-set")}
	header := &cmttypes.Header{
		Version:            cmtversion.Consensus{Block: version.BlockProtocol, App: 1},
		ChainID:            chainID,
		Height:             2,
		Time:               headerTime,
		LastBlockID:        trustedBlockID,
		LastCommitHash:     hashBytes("update-last-commit"),
		DataHash:           hashBytes("update-data"),
		ValidatorsHash:     valHash,
		NextValidatorsHash: valHash,
		ConsensusHash:      hashBytes("update-consensus"),
		AppHash:            hashBytes("update-app"),
		LastResultsHash:    hashBytes("update-results"),
		EvidenceHash:       hashBytes("update-evidence"),
		ProposerAddress:    valSet.Proposer.Address,
	}
	if err := header.ValidateBasic(); err != nil {
		return fixture{}, fmt.Errorf("update header invalid: %w", err)
	}

	blockID := cmttypes.BlockID{Hash: header.Hash(), PartSetHeader: part2}
	sigs := make([]cmttypes.CommitSig, len(valSet.Validators))
	voteSignBytes := make([]string, len(valSet.Validators))
	recoveredSigners := make([]string, len(valSet.Validators))
	for i, val := range valSet.Validators {
		vote := &cmttypes.Vote{
			Type:             cmtproto.PrecommitType,
			Height:           header.Height,
			Round:            0,
			BlockID:          blockID,
			Timestamp:        headerTime,
			ValidatorAddress: val.Address,
			ValidatorIndex:   int32(i),
		}
		signBytes := cmttypes.VoteSignBytes(chainID, vote.ToProto())
		priv := privByAddress[string(val.Address)]
		sig, err := priv.Sign(signBytes)
		if err != nil {
			return fixture{}, err
		}
		sigs[i] = cmttypes.CommitSig{
			BlockIDFlag:      cmttypes.BlockIDFlagCommit,
			ValidatorAddress: val.Address,
			Timestamp:        headerTime,
			Signature:        sig,
		}
		voteSignBytes[i] = hexBytes(signBytes)
		recoveredSigners[i] = addressHex(val.Address)
	}

	commit := &cmttypes.Commit{Height: header.Height, Round: 0, BlockID: blockID, Signatures: sigs}
	signedHeader := &cmttypes.SignedHeader{Header: header, Commit: commit}
	if err := signedHeader.ValidateBasic(chainID); err != nil {
		return fixture{}, fmt.Errorf("signed header invalid: %w", err)
	}
	if err := light.VerifyAdjacent(
		&cmttypes.SignedHeader{Header: trustedHeader},
		signedHeader,
		valSet,
		time.Duration(trustingPeriod)*time.Second,
		now,
		time.Duration(maxClockDrift)*time.Second,
	); err != nil {
		return fixture{}, fmt.Errorf("cometbft adjacent verification failed: %w", err)
	}

	validatorsJSON := validatorsJSONFromSet(valSet, privByAddress)

	blockIDFlags := make([]uint64, len(commit.Signatures))
	validatorAddresses := make([]string, len(commit.Signatures))
	timestampSeconds := make([]uint64, len(commit.Signatures))
	commitTimestampNanos := make([]uint32, len(commit.Signatures))
	signatures := make([]string, len(commit.Signatures))
	for i, sig := range commit.Signatures {
		blockIDFlags[i] = uint64(sig.BlockIDFlag)
		validatorAddresses[i] = addressHex(sig.ValidatorAddress)
		timestampSeconds[i] = uint64(sig.Timestamp.Unix())
		commitTimestampNanos[i] = uint32(sig.Timestamp.Nanosecond())
		signatures[i] = hexBytes(sig.Signature)
	}

	return fixture{
		ChainID:         chainID,
		TrustedHeight:   1,
		RevisionNumber:  0,
		TrustingPeriod:  trustingPeriod,
		UnbondingPeriod: unbondingPeriod,
		MaxClockDrift:   maxClockDrift,
		ProofNow:        uint64(now.Unix()),
		TrustedConsensus: consensusJSON{
			Timestamp:          timestampNanos(trustedTime),
			Root:               hexBytes(trustedHeader.AppHash),
			NextValidatorsHash: hexBytes(trustedHeader.NextValidatorsHash),
		},
		Header: headerJSON{
			VersionBlock:       header.Version.Block,
			VersionApp:         header.Version.App,
			ChainID:            header.ChainID,
			Height:             uint64(header.Height),
			TimeSeconds:        uint64(header.Time.Unix()),
			TimeNanos:          uint32(header.Time.Nanosecond()),
			LastBlockID:        blockIDJSONFromTypes(header.LastBlockID),
			LastCommitHash:     hexBytes(header.LastCommitHash),
			DataHash:           hexBytes(header.DataHash),
			ValidatorsHash:     hexBytes(header.ValidatorsHash),
			NextValidatorsHash: hexBytes(header.NextValidatorsHash),
			ConsensusHash:      hexBytes(header.ConsensusHash),
			AppHash:            hexBytes(header.AppHash),
			LastResultsHash:    hexBytes(header.LastResultsHash),
			EvidenceHash:       hexBytes(header.EvidenceHash),
			ProposerAddress:    addressHex(header.ProposerAddress),
		},
		Commit: commitJSON{
			Height:             uint64(commit.Height),
			Round:              uint32(commit.Round),
			BlockID:            blockIDJSONFromTypes(commit.BlockID),
			BlockIDFlags:       blockIDFlags,
			ValidatorAddresses: validatorAddresses,
			TimestampSeconds:   timestampSeconds,
			TimestampNanos:     commitTimestampNanos,
			Signatures:         signatures,
		},
		TrustedValidators: validatorsJSON,
		Validators:        validatorsJSON,
		NextValidators:    validatorsJSON,
		Expected: expectedJSON{
			ValidatorSetHash:         hexBytes(valSet.Hash()),
			NextValidatorSetHash:     hexBytes(valSet.Hash()),
			TrustedValidatorSetHash:  hexBytes(valSet.Hash()),
			TrustedHeaderHash:        hexBytes(trustedHeader.Hash()),
			HeaderHash:               hexBytes(header.Hash()),
			VoteSignBytes:            voteSignBytes,
			RecoveredSigners:         recoveredSigners,
			TrustedSignedVotingPower: uint64(valSet.TotalVotingPower()),
			TrustedVotingPowerNeeded: uint64(valSet.TotalVotingPower() / 3),
			NewSignedVotingPower:     uint64(valSet.TotalVotingPower()),
			NewVotingPowerNeeded:     uint64(valSet.TotalVotingPower() * 2 / 3),
		},
		CometBFTVerification: "light.VerifyAdjacent succeeded with local /Users/gg/code/contrib/cometbft",
	}, nil
}

func buildSkippingFixture(validatorCount int, insufficientTrusted bool) (fixture, error) {
	const chainID = "native-cometbft-1"
	const trustingPeriod = 14 * 24 * 60 * 60
	const unbondingPeriod = 21 * 24 * 60 * 60
	const maxClockDrift = 30

	trustedTime := time.Unix(1_680_220_500, 123_000_000).UTC()
	headerTime := time.Unix(1_680_220_900, 456_000_000).UTC()
	now := time.Unix(1_680_221_000, 0).UTC()

	type pv struct {
		priv secp256k1eth.PrivKey
		val  *cmttypes.Validator
	}

	trustedPowers := make([]int64, validatorCount)
	if validatorCount == 3 {
		trustedPowers = []int64{34, 33, 33}
		if insufficientTrusted {
			trustedPowers = []int64{33, 34, 33}
		}
	} else {
		for i := 0; i < validatorCount; i++ {
			trustedPowers[i] = int64((validatorCount - i) * 10)
		}
	}

	trustedPrivVals := make([]pv, validatorCount)
	privByAddress := map[string]secp256k1eth.PrivKey{}
	for i := 0; i < validatorCount; i++ {
		priv := secp256k1eth.GenPrivKeySecp256k1Eth([]byte(fmt.Sprintf("native-cometbft-skip-old-%d", i+1)))
		val := cmttypes.NewValidator(priv.PubKey(), trustedPowers[i])
		trustedPrivVals[i] = pv{priv: priv, val: val}
		privByAddress[string(val.Address)] = priv
	}

	trustedValidators := make([]*cmttypes.Validator, len(trustedPrivVals))
	for i, pv := range trustedPrivVals {
		trustedValidators[i] = pv.val
	}
	trustedValSet := cmttypes.NewValidatorSet(trustedValidators)

	overlap := 1
	if validatorCount > 3 {
		var signed int64
		needed := trustedValSet.TotalVotingPower() / 3
		for overlap < validatorCount && signed <= needed {
			signed += trustedPrivVals[overlap-1].val.VotingPower
			if signed > needed {
				break
			}
			overlap++
		}
	}

	newValidators := make([]*cmttypes.Validator, 0, validatorCount)
	for i := 0; i < overlap; i++ {
		newValidators = append(newValidators, cmttypes.NewValidator(trustedPrivVals[i].priv.PubKey(), trustedPrivVals[i].val.VotingPower))
	}
	for i := overlap; i < validatorCount; i++ {
		priv := secp256k1eth.GenPrivKeySecp256k1Eth([]byte(fmt.Sprintf("native-cometbft-skip-new-%d", i+1)))
		power := trustedPowers[i]
		val := cmttypes.NewValidator(priv.PubKey(), power)
		newValidators = append(newValidators, val)
		privByAddress[string(val.Address)] = priv
	}
	newValSet := cmttypes.NewValidatorSet(newValidators)

	nextValidators := make([]*cmttypes.Validator, 0, validatorCount)
	for i := 0; i < validatorCount; i++ {
		priv := secp256k1eth.GenPrivKeySecp256k1Eth([]byte(fmt.Sprintf("native-cometbft-skip-next-%d", i+1)))
		val := cmttypes.NewValidator(priv.PubKey(), trustedPowers[i])
		nextValidators = append(nextValidators, val)
		privByAddress[string(val.Address)] = priv
	}
	nextValSet := cmttypes.NewValidatorSet(nextValidators)

	trustedHeader := &cmttypes.Header{
		Version:            cmtversion.Consensus{Block: version.BlockProtocol, App: 1},
		ChainID:            chainID,
		Height:             1,
		Time:               trustedTime,
		LastBlockID:        cmttypes.BlockID{},
		LastCommitHash:     hashBytes("skip-trusted-last-commit"),
		DataHash:           hashBytes("skip-trusted-data"),
		ValidatorsHash:     trustedValSet.Hash(),
		NextValidatorsHash: trustedValSet.Hash(),
		ConsensusHash:      hashBytes("skip-trusted-consensus"),
		AppHash:            hashBytes("skip-trusted-app"),
		LastResultsHash:    hashBytes("skip-trusted-results"),
		EvidenceHash:       hashBytes("skip-trusted-evidence"),
		ProposerAddress:    trustedValSet.Proposer.Address,
	}
	if err := trustedHeader.ValidateBasic(); err != nil {
		return fixture{}, fmt.Errorf("trusted skipping header invalid: %w", err)
	}

	header := &cmttypes.Header{
		Version: cmtversion.Consensus{Block: version.BlockProtocol, App: 1},
		ChainID: chainID,
		Height:  4,
		Time:    headerTime,
		LastBlockID: cmttypes.BlockID{
			Hash:          hashBytes("skip-intermediate-block"),
			PartSetHeader: cmttypes.PartSetHeader{Total: 1, Hash: hashBytes("skip-intermediate-part-set")},
		},
		LastCommitHash:     hashBytes("skip-update-last-commit"),
		DataHash:           hashBytes("skip-update-data"),
		ValidatorsHash:     newValSet.Hash(),
		NextValidatorsHash: nextValSet.Hash(),
		ConsensusHash:      hashBytes("skip-update-consensus"),
		AppHash:            hashBytes("skip-update-app"),
		LastResultsHash:    hashBytes("skip-update-results"),
		EvidenceHash:       hashBytes("skip-update-evidence"),
		ProposerAddress:    newValSet.Proposer.Address,
	}
	if err := header.ValidateBasic(); err != nil {
		return fixture{}, fmt.Errorf("skipping update header invalid: %w", err)
	}

	blockID := cmttypes.BlockID{
		Hash: header.Hash(), PartSetHeader: cmttypes.PartSetHeader{Total: 1, Hash: hashBytes("skip-update-part-set")},
	}
	commit, commitTypes, voteSignBytes, recoveredSigners, err :=
		signCommitWithTypes(chainID, header, blockID, headerTime, newValSet, privByAddress)
	if err != nil {
		return fixture{}, fmt.Errorf("sign skipping update: %w", err)
	}

	signedHeader := &cmttypes.SignedHeader{Header: header, Commit: commitTypes}
	if err := light.Verify(
		&cmttypes.SignedHeader{Header: trustedHeader},
		trustedValSet,
		signedHeader,
		newValSet,
		time.Duration(trustingPeriod)*time.Second,
		now,
		time.Duration(maxClockDrift)*time.Second,
		cmtmath.Fraction{Numerator: 1, Denominator: 3},
	); insufficientTrusted {
		var trustErr light.ErrNewValSetCantBeTrusted
		if !errors.As(err, &trustErr) {
			return fixture{}, fmt.Errorf("expected insufficient trusted overlap, got %w", err)
		}
	} else if err != nil {
		return fixture{}, fmt.Errorf("cometbft skipping verification failed: %w", err)
	}

	trustedSignedPower := int64(0)
	for i := 0; i < overlap; i++ {
		trustedSignedPower += trustedPrivVals[i].val.VotingPower
	}

	return fixture{
		ChainID:         chainID,
		TrustedHeight:   1,
		RevisionNumber:  0,
		TrustingPeriod:  trustingPeriod,
		UnbondingPeriod: unbondingPeriod,
		MaxClockDrift:   maxClockDrift,
		ProofNow:        uint64(now.Unix()),
		TrustedConsensus: consensusJSON{
			Timestamp:          timestampNanos(trustedTime),
			Root:               hexBytes(trustedHeader.AppHash),
			NextValidatorsHash: hexBytes(trustedHeader.NextValidatorsHash),
		},
		Header:            headerJSONFromTypes(header),
		Commit:            commit,
		TrustedValidators: validatorsJSONFromSet(trustedValSet, privByAddress),
		Validators:        validatorsJSONFromSet(newValSet, privByAddress),
		NextValidators:    validatorsJSONFromSet(nextValSet, privByAddress),
		Expected: expectedJSON{
			ValidatorSetHash:         hexBytes(newValSet.Hash()),
			NextValidatorSetHash:     hexBytes(nextValSet.Hash()),
			TrustedValidatorSetHash:  hexBytes(trustedValSet.Hash()),
			TrustedHeaderHash:        hexBytes(trustedHeader.Hash()),
			HeaderHash:               hexBytes(header.Hash()),
			VoteSignBytes:            voteSignBytes,
			RecoveredSigners:         recoveredSigners,
			TrustedSignedVotingPower: uint64(trustedSignedPower),
			TrustedVotingPowerNeeded: uint64(trustedValSet.TotalVotingPower() / 3),
			NewSignedVotingPower:     uint64(newValSet.TotalVotingPower()),
			NewVotingPowerNeeded:     uint64(newValSet.TotalVotingPower() * 2 / 3),
		},
		CometBFTVerification: "light.Verify non-adjacent path checked with local /Users/gg/code/contrib/cometbft",
	}, nil
}

func buildStoredChainedSkippingFixture(validatorCount int) (fixture, error) {
	const chainID = "native-cometbft-1"
	const trustingPeriod = 14 * 24 * 60 * 60
	const unbondingPeriod = 21 * 24 * 60 * 60
	const maxClockDrift = 30

	trustedTime := time.Unix(1_680_220_900, 456_000_000).UTC()
	headerTime := time.Unix(1_680_221_300, 789_000_000).UTC()
	now := time.Unix(1_680_221_400, 0).UTC()

	type pv struct {
		priv secp256k1eth.PrivKey
		val  *cmttypes.Validator
	}

	trustedPowers := []int64{34, 33, 33}
	if validatorCount != 3 {
		trustedPowers = make([]int64, validatorCount)
		for i := 0; i < validatorCount; i++ {
			trustedPowers[i] = int64((validatorCount - i) * 10)
		}
	}

	trustedPrivVals := make([]pv, validatorCount)
	privByAddress := map[string]secp256k1eth.PrivKey{}
	for i := 0; i < validatorCount; i++ {
		priv := secp256k1eth.GenPrivKeySecp256k1Eth([]byte(fmt.Sprintf("native-cometbft-skip-next-%d", i+1)))
		val := cmttypes.NewValidator(priv.PubKey(), trustedPowers[i])
		trustedPrivVals[i] = pv{priv: priv, val: val}
		privByAddress[string(val.Address)] = priv
	}

	trustedValidators := make([]*cmttypes.Validator, len(trustedPrivVals))
	for i, pv := range trustedPrivVals {
		trustedValidators[i] = pv.val
	}
	trustedValSet := cmttypes.NewValidatorSet(trustedValidators)

	overlap := 1
	if validatorCount > 3 {
		var signed int64
		needed := trustedValSet.TotalVotingPower() / 3
		for overlap < validatorCount && signed <= needed {
			signed += trustedPrivVals[overlap-1].val.VotingPower
			if signed > needed {
				break
			}
			overlap++
		}
	}

	newValidators := make([]*cmttypes.Validator, 0, validatorCount)
	for i := 0; i < overlap; i++ {
		newValidators = append(newValidators, cmttypes.NewValidator(trustedPrivVals[i].priv.PubKey(), trustedPrivVals[i].val.VotingPower))
	}
	for i := overlap; i < validatorCount; i++ {
		priv := secp256k1eth.GenPrivKeySecp256k1Eth([]byte(fmt.Sprintf("native-cometbft-skip-chain-new-%d", i+1)))
		val := cmttypes.NewValidator(priv.PubKey(), trustedPowers[i])
		newValidators = append(newValidators, val)
		privByAddress[string(val.Address)] = priv
	}
	newValSet := cmttypes.NewValidatorSet(newValidators)

	trustedHeader := &cmttypes.Header{
		Version:            cmtversion.Consensus{Block: version.BlockProtocol, App: 1},
		ChainID:            chainID,
		Height:             4,
		Time:               trustedTime,
		LastBlockID:        cmttypes.BlockID{},
		LastCommitHash:     hashBytes("skip-update-last-commit"),
		DataHash:           hashBytes("skip-update-data"),
		ValidatorsHash:     hashBytes("stored-chain-placeholder-current-validators"),
		NextValidatorsHash: trustedValSet.Hash(),
		ConsensusHash:      hashBytes("skip-update-consensus"),
		AppHash:            hashBytes("skip-update-app"),
		LastResultsHash:    hashBytes("skip-update-results"),
		EvidenceHash:       hashBytes("skip-update-evidence"),
		ProposerAddress:    trustedValSet.Proposer.Address,
	}
	if err := trustedHeader.ValidateBasic(); err != nil {
		return fixture{}, fmt.Errorf("trusted chained skipping header invalid: %w", err)
	}

	header := &cmttypes.Header{
		Version: cmtversion.Consensus{Block: version.BlockProtocol, App: 1},
		ChainID: chainID,
		Height:  7,
		Time:    headerTime,
		LastBlockID: cmttypes.BlockID{
			Hash:          hashBytes("skip-chain-intermediate-block"),
			PartSetHeader: cmttypes.PartSetHeader{Total: 1, Hash: hashBytes("skip-chain-intermediate-part-set")},
		},
		LastCommitHash:     hashBytes("skip-chain-last-commit"),
		DataHash:           hashBytes("skip-chain-data"),
		ValidatorsHash:     newValSet.Hash(),
		NextValidatorsHash: newValSet.Hash(),
		ConsensusHash:      hashBytes("skip-chain-consensus"),
		AppHash:            hashBytes("skip-chain-app"),
		LastResultsHash:    hashBytes("skip-chain-results"),
		EvidenceHash:       hashBytes("skip-chain-evidence"),
		ProposerAddress:    newValSet.Proposer.Address,
	}
	if err := header.ValidateBasic(); err != nil {
		return fixture{}, fmt.Errorf("chained skipping update header invalid: %w", err)
	}

	blockID := cmttypes.BlockID{
		Hash: header.Hash(), PartSetHeader: cmttypes.PartSetHeader{Total: 1, Hash: hashBytes("skip-chain-part-set")},
	}
	commit, commitTypes, voteSignBytes, recoveredSigners, err :=
		signCommitWithTypes(chainID, header, blockID, headerTime, newValSet, privByAddress)
	if err != nil {
		return fixture{}, fmt.Errorf("sign chained skipping update: %w", err)
	}

	signedHeader := &cmttypes.SignedHeader{Header: header, Commit: commitTypes}
	if err := light.Verify(
		&cmttypes.SignedHeader{Header: trustedHeader},
		trustedValSet,
		signedHeader,
		newValSet,
		time.Duration(trustingPeriod)*time.Second,
		now,
		time.Duration(maxClockDrift)*time.Second,
		cmtmath.Fraction{Numerator: 1, Denominator: 3},
	); err != nil {
		return fixture{}, fmt.Errorf("cometbft chained skipping verification failed: %w", err)
	}

	trustedSignedPower := int64(0)
	for i := 0; i < overlap; i++ {
		trustedSignedPower += trustedPrivVals[i].val.VotingPower
	}

	validatorsJSON := validatorsJSONFromSet(newValSet, privByAddress)
	return fixture{
		ChainID:         chainID,
		TrustedHeight:   4,
		RevisionNumber:  0,
		TrustingPeriod:  trustingPeriod,
		UnbondingPeriod: unbondingPeriod,
		MaxClockDrift:   maxClockDrift,
		ProofNow:        uint64(now.Unix()),
		TrustedConsensus: consensusJSON{
			Timestamp:          timestampNanos(trustedTime),
			Root:               hexBytes(trustedHeader.AppHash),
			NextValidatorsHash: hexBytes(trustedHeader.NextValidatorsHash),
		},
		Header:            headerJSONFromTypes(header),
		Commit:            commit,
		TrustedValidators: validatorsJSONFromSet(trustedValSet, privByAddress),
		Validators:        validatorsJSON,
		NextValidators:    validatorsJSON,
		Expected: expectedJSON{
			ValidatorSetHash:         hexBytes(newValSet.Hash()),
			NextValidatorSetHash:     hexBytes(newValSet.Hash()),
			TrustedValidatorSetHash:  hexBytes(trustedValSet.Hash()),
			TrustedHeaderHash:        hexBytes(trustedHeader.Hash()),
			HeaderHash:               hexBytes(header.Hash()),
			VoteSignBytes:            voteSignBytes,
			RecoveredSigners:         recoveredSigners,
			TrustedSignedVotingPower: uint64(trustedSignedPower),
			TrustedVotingPowerNeeded: uint64(trustedValSet.TotalVotingPower() / 3),
			NewSignedVotingPower:     uint64(newValSet.TotalVotingPower()),
			NewVotingPowerNeeded:     uint64(newValSet.TotalVotingPower() * 2 / 3),
		},
		CometBFTVerification: "light.Verify chained non-adjacent path checked with local /Users/gg/code/contrib/cometbft",
	}, nil
}

func hashBytes(label string) cmtbytes.HexBytes {
	sum := sha256.Sum256([]byte(label))
	return sum[:]
}

func signCommit(
	chainID string,
	header *cmttypes.Header,
	blockID cmttypes.BlockID,
	signTime time.Time,
	valSet *cmttypes.ValidatorSet,
	privByAddress map[string]secp256k1eth.PrivKey,
) (commitJSON, []string, []string, error) {
	commit, voteSignBytes, recoveredSigners, err :=
		signCommitTypes(chainID, header, blockID, signTime, valSet, privByAddress)
	if err != nil {
		return commitJSON{}, nil, nil, err
	}
	return commitJSONFromTypes(commit), voteSignBytes, recoveredSigners, nil
}

func signCommitWithTypes(
	chainID string,
	header *cmttypes.Header,
	blockID cmttypes.BlockID,
	signTime time.Time,
	valSet *cmttypes.ValidatorSet,
	privByAddress map[string]secp256k1eth.PrivKey,
) (commitJSON, *cmttypes.Commit, []string, []string, error) {
	commit, voteSignBytes, recoveredSigners, err :=
		signCommitTypes(chainID, header, blockID, signTime, valSet, privByAddress)
	if err != nil {
		return commitJSON{}, nil, nil, nil, err
	}
	return commitJSONFromTypes(commit), commit, voteSignBytes, recoveredSigners, nil
}

func signCommitTypes(
	chainID string,
	header *cmttypes.Header,
	blockID cmttypes.BlockID,
	signTime time.Time,
	valSet *cmttypes.ValidatorSet,
	privByAddress map[string]secp256k1eth.PrivKey,
) (*cmttypes.Commit, []string, []string, error) {
	sigs := make([]cmttypes.CommitSig, len(valSet.Validators))
	voteSignBytes := make([]string, len(valSet.Validators))
	recoveredSigners := make([]string, len(valSet.Validators))
	for i, val := range valSet.Validators {
		vote := &cmttypes.Vote{
			Type:             cmtproto.PrecommitType,
			Height:           header.Height,
			Round:            0,
			BlockID:          blockID,
			Timestamp:        signTime,
			ValidatorAddress: val.Address,
			ValidatorIndex:   int32(i),
		}
		signBytes := cmttypes.VoteSignBytes(chainID, vote.ToProto())
		priv := privByAddress[string(val.Address)]
		sig, err := priv.Sign(signBytes)
		if err != nil {
			return nil, nil, nil, err
		}
		sigs[i] = cmttypes.CommitSig{
			BlockIDFlag:      cmttypes.BlockIDFlagCommit,
			ValidatorAddress: val.Address,
			Timestamp:        signTime,
			Signature:        sig,
		}
		voteSignBytes[i] = hexBytes(signBytes)
		recoveredSigners[i] = addressHex(val.Address)
	}

	commit := &cmttypes.Commit{Height: header.Height, Round: 0, BlockID: blockID, Signatures: sigs}
	signedHeader := &cmttypes.SignedHeader{Header: header, Commit: commit}
	if err := signedHeader.ValidateBasic(chainID); err != nil {
		return nil, nil, nil, err
	}
	return commit, voteSignBytes, recoveredSigners, nil
}

func headerJSONFromTypes(header *cmttypes.Header) headerJSON {
	return headerJSON{
		VersionBlock:       header.Version.Block,
		VersionApp:         header.Version.App,
		ChainID:            header.ChainID,
		Height:             uint64(header.Height),
		TimeSeconds:        uint64(header.Time.Unix()),
		TimeNanos:          uint32(header.Time.Nanosecond()),
		LastBlockID:        blockIDJSONFromTypes(header.LastBlockID),
		LastCommitHash:     hexBytes(header.LastCommitHash),
		DataHash:           hexBytes(header.DataHash),
		ValidatorsHash:     hexBytes(header.ValidatorsHash),
		NextValidatorsHash: hexBytes(header.NextValidatorsHash),
		ConsensusHash:      hexBytes(header.ConsensusHash),
		AppHash:            hexBytes(header.AppHash),
		LastResultsHash:    hexBytes(header.LastResultsHash),
		EvidenceHash:       hexBytes(header.EvidenceHash),
		ProposerAddress:    addressHex(header.ProposerAddress),
	}
}

func commitJSONFromTypes(commit *cmttypes.Commit) commitJSON {
	blockIDFlags := make([]uint64, len(commit.Signatures))
	validatorAddresses := make([]string, len(commit.Signatures))
	timestampSeconds := make([]uint64, len(commit.Signatures))
	commitTimestampNanos := make([]uint32, len(commit.Signatures))
	signatures := make([]string, len(commit.Signatures))
	for i, sig := range commit.Signatures {
		blockIDFlags[i] = uint64(sig.BlockIDFlag)
		validatorAddresses[i] = addressHex(sig.ValidatorAddress)
		timestampSeconds[i] = uint64(sig.Timestamp.Unix())
		commitTimestampNanos[i] = uint32(sig.Timestamp.Nanosecond())
		signatures[i] = hexBytes(sig.Signature)
	}

	return commitJSON{
		Height:             uint64(commit.Height),
		Round:              uint32(commit.Round),
		BlockID:            blockIDJSONFromTypes(commit.BlockID),
		BlockIDFlags:       blockIDFlags,
		ValidatorAddresses: validatorAddresses,
		TimestampSeconds:   timestampSeconds,
		TimestampNanos:     commitTimestampNanos,
		Signatures:         signatures,
	}
}

func validatorsJSONFromSet(
	valSet *cmttypes.ValidatorSet,
	privByAddress map[string]secp256k1eth.PrivKey,
) validatorsJSON {
	addresses := make([]string, len(valSet.Validators))
	publicKeys := make([]string, len(valSet.Validators))
	publicKeyYWitnesses := make([]string, len(valSet.Validators))
	uncompressedPublicKeys := make([]string, len(valSet.Validators))
	powers := make([]uint64, len(valSet.Validators))
	for i, val := range valSet.Validators {
		addresses[i] = addressHex(val.Address)
		publicKeys[i] = hexBytes(val.PubKey.Bytes())
		uncompressed := uncompressedPubKey(privByAddress[string(val.Address)])
		publicKeyYWitnesses[i] = hexBytes(uncompressed[32:])
		uncompressedPublicKeys[i] = hexBytes(uncompressed)
		powers[i] = uint64(val.VotingPower)
	}

	return validatorsJSON{
		Addresses:              addresses,
		PublicKeys:             publicKeys,
		PublicKeyYWitnesses:    publicKeyYWitnesses,
		UncompressedPublicKeys: uncompressedPublicKeys,
		VotingPowers:           powers,
	}
}

func timestampNanos(t time.Time) uint64 {
	return uint64(t.Unix())*1_000_000_000 + uint64(t.Nanosecond())
}

func blockIDJSONFromTypes(blockID cmttypes.BlockID) blockIDJSON {
	return blockIDJSON{
		Hash: hexBytes(blockID.Hash),
		PartSetHeader: partSetHeaderJSON{
			Total: blockID.PartSetHeader.Total,
			Hash:  hexBytes(blockID.PartSetHeader.Hash),
		},
	}
}

func addressHex(bz []byte) string {
	return "0x" + hex.EncodeToString(bz)
}

func hexBytes(bz []byte) string {
	return "0x" + hex.EncodeToString(bz)
}

func uncompressedPubKey(priv secp256k1eth.PrivKey) []byte {
	pub := secp256k1.PrivKeyFromBytes(priv).PubKey().SerializeUncompressed()
	return pub[1:]
}
