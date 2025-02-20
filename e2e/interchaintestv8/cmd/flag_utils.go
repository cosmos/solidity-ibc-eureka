package main

import (
	"fmt"

	"github.com/spf13/cobra"
	"google.golang.org/grpc"
	"google.golang.org/grpc/credentials/insecure"
)

func GetCosmosGRPC(cmd *cobra.Command) (*grpc.ClientConn, error) {
	cosmosGrpcAddress, _ := cmd.Flags().GetString(FlagCosmosGRPC)
	if cosmosGrpcAddress == "" {
		return nil, fmt.Errorf("cosmos-grpc flag not set")
	}
	grpcConn, err := grpc.Dial(
		cosmosGrpcAddress,
		grpc.WithTransportCredentials(insecure.NewCredentials()),
	)
	if err != nil {
		return nil, err
	}

	return grpcConn, nil
}
