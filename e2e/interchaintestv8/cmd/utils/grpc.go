package utils

import (
	"fmt"

	"google.golang.org/grpc"
	"google.golang.org/grpc/credentials"
)

func GetTLSGRPC(addr string) (*grpc.ClientConn, error) {
	creds := credentials.NewTLS(nil)

	// Establish a secure connection with the gRPC server
	conn, err := grpc.Dial(addr, grpc.
		WithTransportCredentials(creds))
	if err != nil {
		return nil, fmt.Errorf("failed to connect to grpc client with addr: %s: %w", addr, err)
	}

	return conn, nil
}

func GetNonTLSGRPC(addr string) (*grpc.ClientConn, error) {
	// Establish a connection with the gRPC server
	conn, err := grpc.Dial(addr, grpc.WithInsecure())
	if err != nil {
		return nil, fmt.Errorf("failed to connect to grpc client with addr: %s: %w", addr, err)
	}

	return conn, nil
}
