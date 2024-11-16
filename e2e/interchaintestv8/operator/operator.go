package operator

import (
	"bytes"
	"encoding/base64"
	"encoding/hex"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"os"
	"os/exec"
	"strconv"
	"strings"

	"github.com/ethereum/go-ethereum/accounts/abi"

	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/types/ics26router"
)

// membershipFixture is a struct that contains the membership proof and proof height
type membershipFixture struct {
	// hex encoded height
	ProofHeight string `json:"proofHeight"`
	// hex encoded proof
	MembershipProof string `json:"membershipProof"`
}

func BinaryPath() string {
	return "operator"
}

// RunGenesis is a function that runs the genesis script to generate genesis.json
func RunGenesis(args ...string) error {
	args = append([]string{"genesis"}, args...)
	// nolint:gosec
	cmd := exec.Command(BinaryPath(), args...)
	cmd.Stdout = os.Stdout
	cmd.Stderr = os.Stderr
	return cmd.Run()
}

// StartOperator is a function that runs the operator
func StartOperator(args ...string) error {
	args = append([]string{"start"}, args...)
	// nolint:gosec
	cmd := exec.Command(BinaryPath(), args...)
	cmd.Stdout = os.Stdout
	cmd.Stderr = os.Stderr
	return cmd.Run()
}

// UpdateClientAndMembershipProof is a function that generates an update client and membership proof
func UpdateClientAndMembershipProof(trusted_height, target_height uint64, paths string, args ...string) (*ics26router.IICS02ClientMsgsHeight, []byte, error) {
	args = append([]string{"fixtures", "update-client-and-membership", "--trusted-block", strconv.FormatUint(trusted_height, 10), "--target-block", strconv.FormatUint(target_height, 10), "--key-paths", paths}, args...)
	// nolint:gosec
	cmd := exec.Command(BinaryPath(), args...)
	stdout, err := execOperatorCommand(cmd)
	if err != nil {
		return nil, nil, err
	}

	// eliminate non-json characters
	jsonStartIdx := strings.Index(string(stdout), "{")
	if jsonStartIdx == -1 {
		panic("no json found in output")
	}
	stdout = stdout[jsonStartIdx:]

	var membership membershipFixture
	err = json.Unmarshal(stdout, &membership)
	if err != nil {
		return nil, nil, err
	}

	heightBz, err := hex.DecodeString(membership.ProofHeight)
	if err != nil {
		return nil, nil, err
	}

	heightType, err := abi.NewType("tuple", "IICS02ClientMsgsHeight", []abi.ArgumentMarshaling{
		{Name: "revisionNumber", Type: "uint32"},
		{Name: "revisionHeight", Type: "uint32"},
	})
	if err != nil {
		return nil, nil, err
	}

	heightArgs := abi.Arguments{
		{Type: heightType, Name: "param_one"},
	}

	// abi encoding
	heightI, err := heightArgs.Unpack(heightBz)
	if err != nil {
		return nil, nil, err
	}

	height := abi.ConvertType(heightI[0], new(ics26router.IICS02ClientMsgsHeight)).(*ics26router.IICS02ClientMsgsHeight)

	if height.RevisionHeight != uint32(target_height) {
		return nil, nil, errors.New("heights do not match")
	}

	proofBz, err := hex.DecodeString(membership.MembershipProof)
	if err != nil {
		return nil, nil, err
	}

	return height, proofBz, nil
}

func execOperatorCommand(c *exec.Cmd) ([]byte, error) {
	var outBuf bytes.Buffer

	// Create a MultiWriter to write to both os.Stdout and the buffer
	multiWriter := io.MultiWriter(os.Stdout, &outBuf)

	// Set the command's stdout and stderror to the MultiWriter
	c.Stdout = multiWriter
	c.Stderr = multiWriter

	// Run the command
	if err := c.Run(); err != nil {
		return nil, fmt.Errorf("operator command '%s' failed: %s", strings.Join(c.Args, " "), outBuf.String())
	}

	return outBuf.Bytes(), nil
}

// ToBase64KeyPaths is a function that takes a list of key paths and returns a base64 encoded string
// that the operator can use to generate a membership proof
func ToBase64KeyPaths(paths ...[][]byte) string {
	var keyPaths []string
	for _, path := range paths {
		if len(path) != 2 {
			panic("path must have 2 elements")
		}
		keyPaths = append(keyPaths, base64.StdEncoding.EncodeToString(path[0])+"\\"+base64.StdEncoding.EncodeToString(path[1]))
	}
	return strings.Join(keyPaths, ",")
}
