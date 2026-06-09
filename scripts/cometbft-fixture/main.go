package main

import (
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"strconv"
	"time"

	"github.com/cometbft/cometbft/crypto/secp256k1eth"
	cmtbytes "github.com/cometbft/cometbft/libs/bytes"
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
	Validators           validatorsJSON `json:"validators"`
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
	ValidatorSetHash  string   `json:"validatorSetHash"`
	TrustedHeaderHash string   `json:"trustedHeaderHash"`
	HeaderHash        string   `json:"headerHash"`
	VoteSignBytes     []string `json:"voteSignBytes"`
	RecoveredSigners  []string `json:"recoveredSigners"`
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
		Validators: validatorsJSON{
			Addresses:              addresses,
			PublicKeys:             publicKeys,
			PublicKeyYWitnesses:    publicKeyYWitnesses,
			UncompressedPublicKeys: uncompressedPublicKeys,
			VotingPowers:           powers,
		},
		Expected: expectedJSON{
			ValidatorSetHash:  hexBytes(valSet.Hash()),
			TrustedHeaderHash: hexBytes(trustedHeader.Hash()),
			HeaderHash:        hexBytes(header.Hash()),
			VoteSignBytes:     voteSignBytes,
			RecoveredSigners:  recoveredSigners,
		},
		CometBFTVerification: "light.VerifyAdjacent succeeded with local /Users/gg/code/contrib/cometbft",
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
			return commitJSON{}, nil, nil, err
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
		return commitJSON{}, nil, nil, err
	}
	return commitJSONFromTypes(commit), voteSignBytes, recoveredSigners, nil
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
