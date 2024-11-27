package relayer

import (
	"os"
	"os/exec"

	grpc "google.golang.org/grpc"

	relayertypes "github.com/srdtrk/solidity-ibc-eureka/e2e/v8/types/relayer"
	insecure "google.golang.org/grpc/credentials/insecure"
)

func BinaryPath() string {
	return "./target/release/relayer"
}

func StartRelayer(configPath string) error {
	cmd := exec.Command(BinaryPath(), "start", "--config", configPath)
	cmd.Stdout = os.Stdout
	cmd.Stderr = os.Stderr
	return cmd.Run()
}

func StopRelayer() error {
	return exec.Command("pkill", "-9", "relayer").Run()
}

func defaultGRPCAddress() string {
	return "127.0.0.1:3000"
}

func GetGRPCClient() (relayertypes.RelayerServiceClient, error) {
	conn, err := grpc.NewClient(defaultGRPCAddress(), grpc.WithTransportCredentials(insecure.NewCredentials()))
	if err != nil {
		return nil, err
	}

	return relayertypes.NewRelayerServiceClient(conn), nil
}
