package relayer

import (
	"encoding/json"
	"errors"
	"fmt"
	"os"
	"os/exec"
	"strconv"
	"time"

	"github.com/cosmos/gogoproto/proto"
	grpc "google.golang.org/grpc"
	insecure "google.golang.org/grpc/credentials/insecure"

	codectypes "github.com/cosmos/cosmos-sdk/codec/types"
	txtypes "github.com/cosmos/cosmos-sdk/types/tx"

	ibcwasmtypes "github.com/cosmos/ibc-go/modules/light-clients/08-wasm/v10/types"
	clienttypes "github.com/cosmos/ibc-go/v10/modules/core/02-client/types"

	ethereumtypes "github.com/srdtrk/solidity-ibc-eureka/e2e/v8/types/ethereum"
	relayertypes "github.com/srdtrk/solidity-ibc-eureka/e2e/v8/types/relayer"
)

// DefaultRelayerGRPCAddress returns the default gRPC address for the relayer.
func DefaultRelayerGRPCAddress() string {
	return "127.0.0.1:3000"
}

// binaryPath returns the path to the relayer binary.
func binaryPath() string {
	return "relayer"
}

// StartRelayer starts the relayer with the given config file.
func StartRelayer(configPath string) (*os.Process, error) {
	config, err := os.ReadFile(configPath)
	if err != nil {
		return nil, err
	}
	fmt.Printf("Starting relayer with config:\n%s\n", config)

	cmd := exec.Command(binaryPath(), "start", "--config", configPath)
	cmd.Stdout = os.Stdout
	cmd.Stderr = os.Stderr

	// run this command in the background
	err = cmd.Start()
	if err != nil {
		return nil, err
	}

	// wait for the relayer to start
	time.Sleep(5 * time.Second)

	return cmd.Process, nil
}

// GetGRPCClient returns a gRPC client for the relayer.
func GetGRPCClient(addr string) (relayertypes.RelayerServiceClient, error) {
	conn, err := grpc.NewClient(addr, grpc.WithTransportCredentials(insecure.NewCredentials()))
	if err != nil {
		return nil, err
	}

	return relayertypes.NewRelayerServiceClient(conn), nil
}

// GetRelayUpdateSlot extracts the latest update slot from the relay body's update messages.
func GetRelayUpdateSlotForWasmClient(relayBody []byte) (uint64, error) {
	var txBody txtypes.TxBody
	err := proto.Unmarshal(relayBody, &txBody)
	if err != nil {
		return 0, fmt.Errorf("failed to unmarshal relay body: %w", err)
	}

	var updateClientMsgsAny []*codectypes.Any
	for _, msg := range txBody.Messages {
		if msg.TypeUrl == "/ibc.core.client.v1.MsgUpdateClient" {
			updateClientMsgsAny = append(updateClientMsgsAny, msg)
		}
	}
	if len(updateClientMsgsAny) == 0 {
		return 0, errors.New("no update client messages found in relay body")
	}

	var headers []ethereumtypes.Header
	for _, updateClientMsgAny := range updateClientMsgsAny {
		var updateClientMsg clienttypes.MsgUpdateClient
		err = proto.Unmarshal(updateClientMsgAny.Value, &updateClientMsg)
		if err != nil {
			return 0, fmt.Errorf("failed to unmarshal MsgUpdateClient: %w", err)
		}
		var clientMessage ibcwasmtypes.ClientMessage
		err = proto.Unmarshal(updateClientMsg.ClientMessage.Value, &clientMessage)
		if err != nil {
			return 0, fmt.Errorf("failed to unmarshal ClientMessage: %w", err)
		}

		var header ethereumtypes.Header
		err = json.Unmarshal(clientMessage.Data, &header)
		if err != nil {
			return 0, fmt.Errorf("failed to unmarshal header: %w", err)
		}

		headers = append(headers, header)
	}

	latestUpdateSlot := uint64(0)
	for _, header := range headers {
		updateSlot, err := strconv.ParseUint(header.ConsensusUpdate.FinalizedHeader.Beacon.Slot, 10, 64)
		if err != nil {
			return 0, fmt.Errorf("failed to parse slot from header: %w", err)
		}
		latestUpdateSlot = max(latestUpdateSlot, updateSlot)
	}

	return latestUpdateSlot, nil
}
