package e2esuite

import (
	"context"
	"fmt"

	"github.com/cosmos/gogoproto/proto"
	"github.com/jhump/protoreflect/grpcreflect"
	"google.golang.org/grpc"
	"google.golang.org/grpc/credentials/insecure"
	pb "google.golang.org/protobuf/proto"
	"google.golang.org/protobuf/types/descriptorpb"

	msgv1 "cosmossdk.io/api/cosmos/msg/v1"

	abci "github.com/cometbft/cometbft/abci/types"

	"github.com/cosmos/interchaintest/v11/chain/cosmos"
)

var queryReqToPath = make(map[string]string)

func populateQueryReqToPath(ctx context.Context, chain *cosmos.CosmosChain) error {
	files, err := queryFileDescriptors(ctx, chain)
	if err != nil {
		return err
	}

	for _, fileDescriptor := range files {
		for _, service := range fileDescriptor.GetService() {
			// Skip services that are annotated with the "cosmos.msg.v1.service" option.
			if ext := pb.GetExtension(service.GetOptions(), msgv1.E_Service); ext != nil && ext.(bool) {
				continue
			}

			for _, method := range service.GetMethod() {
				// trim the first character from input which is a dot
				queryReqToPath[method.GetInputType()[1:]] = "/" + fileDescriptor.GetPackage() + "." + service.GetName() + "/" + method.GetName()
			}
		}
	}

	return nil
}

func ABCIQuery(ctx context.Context, chain *cosmos.CosmosChain, req *abci.RequestQuery) (*abci.ResponseQuery, error) {
	// Create a connection to the gRPC server.
	path := "/cosmos.base.tendermint.v1beta1.Service/ABCIQuery"
	grpcConn, err := grpc.Dial(
		chain.GetHostGRPCAddress(),
		grpc.WithTransportCredentials(insecure.NewCredentials()),
		retryConfig(),
	)
	if err != nil {
		return &abci.ResponseQuery{}, err
	}

	defer grpcConn.Close()

	resp := &abci.ResponseQuery{}
	err = grpcConn.Invoke(ctx, path, req, resp)
	if err != nil {
		return &abci.ResponseQuery{}, err
	}

	return resp, nil
}

// Queries the chain with a query request and deserializes the response to T
func GRPCQuery[T any](ctx context.Context, chain *cosmos.CosmosChain, req proto.Message, opts ...grpc.CallOption) (*T, error) {
	path, ok := queryReqToPath[proto.MessageName(req)]
	if !ok {
		return nil, fmt.Errorf("no path found for %s", proto.MessageName(req))
	}

	// Create a connection to the gRPC server.
	grpcConn, err := grpc.Dial(
		chain.GetHostGRPCAddress(),
		grpc.WithTransportCredentials(insecure.NewCredentials()),
		retryConfig(),
	)
	if err != nil {
		return nil, err
	}

	defer grpcConn.Close()

	resp := new(T)
	err = grpcConn.Invoke(ctx, path, req, resp, opts...)
	if err != nil {
		return nil, err
	}

	return resp, nil
}

// queryFileDescriptors returns the proto file descriptors for every gRPC service
// registered on the chain.
//
// sandbox-ledger does not register the autocli cosmos.reflection.v1.ReflectionService,
// so instead of querying that service directly we use the standard gRPC server
// reflection protocol (which the chain does expose) to enumerate services and
// fetch their declaring file descriptors.
func queryFileDescriptors(ctx context.Context, chain *cosmos.CosmosChain) ([]*descriptorpb.FileDescriptorProto, error) {
	// Create a connection to the gRPC server.
	grpcConn, err := grpc.Dial(
		chain.GetHostGRPCAddress(),
		grpc.WithTransportCredentials(insecure.NewCredentials()),
		retryConfig(),
	)
	if err != nil {
		return nil, err
	}

	defer grpcConn.Close()

	// NewClientAuto negotiates between the v1 and v1alpha server-reflection APIs.
	refClient := grpcreflect.NewClientAuto(ctx, grpcConn)
	defer refClient.Reset()

	services, err := refClient.ListServices()
	if err != nil {
		return nil, fmt.Errorf("failed to list grpc services: %w", err)
	}

	var (
		files []*descriptorpb.FileDescriptorProto
		seen  = make(map[string]struct{})
	)
	for _, service := range services {
		fd, err := refClient.FileContainingSymbol(service)
		if err != nil {
			return nil, fmt.Errorf("failed to resolve descriptor for service %s: %w", service, err)
		}
		if _, ok := seen[fd.GetName()]; ok {
			continue
		}
		seen[fd.GetName()] = struct{}{}
		files = append(files, fd.AsFileDescriptorProto())
	}

	return files, nil
}

func retryConfig() grpc.DialOption {
	policy := `{
            "methodConfig": [{
                "name": [{}],
                "retryPolicy": {
                    "MaxAttempts": 4,
                    "InitialBackoff": ".01s",
                    "MaxBackoff": ".01s",
                    "BackoffMultiplier": 1.0,
                    "RetryableStatusCodes": [
						"CANCELLED",
						"UNKNOWN",
						"DEADLINE_EXCEEDED",
						"NOT_FOUND",
						"ALREADY_EXISTS",
						"PERMISSION_DENIED",
						"RESOURCE_EXHAUSTED",
						"FAILED_PRECONDITION",
						"ABORTED",
						"OUT_OF_RANGE",
						"UNIMPLEMENTED",
						"INTERNAL",
						"UNAVAILABLE",
						"DATA_LOSS",
						"UNAUTHENTICATED"
				    ]
                }
            }]
        }`

	return grpc.WithDefaultServiceConfig(policy)
}
