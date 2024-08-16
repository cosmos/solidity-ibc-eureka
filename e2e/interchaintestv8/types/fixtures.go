package types

import (
	"encoding/hex"
	"encoding/json"
	"os"
	"strings"

	"github.com/ethereum/go-ethereum/accounts/abi"

	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/testvalues"
	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/types/ics26router"
)

// GenericFixture is the fixture to be unmarshalled into a test case in Solidity tests
type GenericFixture struct {
	// Hex encoded bytes for sp1 genesis fixture
	Sp1GenesisFixture string `json:"sp1_genesis_fixture"`
	// Hex encoded bytes to be fed into the router contract
	Msg string `json:"msgs"`
	// The contract address of the ERC20 token
	Erc20Address string `json:"erc20_address"`
}

func generateFixture(erc20Address, methodName string, msg any) (GenericFixture, error) {
	genesisBz, err := os.ReadFile(testvalues.Sp1GenesisFilePath)
	if err != nil {
		return GenericFixture{}, err
	}

	ics26Abi, err := abi.JSON(strings.NewReader(ics26router.ContractMetaData.ABI))
	if err != nil {
		return GenericFixture{}, err
	}

	msgBz, err := ics26Abi.Pack(methodName, msg)
	if err != nil {
		return GenericFixture{}, err
	}

	// Generate the fixture
	fixture := GenericFixture{
		Sp1GenesisFixture: hex.EncodeToString(genesisBz),
		Msg:               hex.EncodeToString(msgBz),
		Erc20Address:      erc20Address,
	}
	return fixture, nil
}

// GenerateAndSaveFixture generates a fixture and saves it to a file
func GenerateAndSaveFixture(fileName, erc20Address, methodName string, msg any) error {
	fixture, err := generateFixture(erc20Address, methodName, msg)
	if err != nil {
		return err
	}

	fixtureBz, err := json.Marshal(fixture)
	if err != nil {
		return err
	}

	filePath := testvalues.FixturesDir + fileName
	// nolint:gosec
	return os.WriteFile(filePath, fixtureBz, 0o644)
}
