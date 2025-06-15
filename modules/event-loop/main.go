package main

import (
	"context"
	"os"
	"os/signal"
	"syscall"

	"github.com/cosmos/solidity-ibc-eureka/eventloop"
)

// TODO: How to take inputs? Http? GRPC?
func main() {
	monitorCh := make(chan eventloop.MonitorEvent)
	attastCh := make(chan eventloop.AttastatorEvent)
	ctx, cancel := context.WithCancel(context.Background())

	eventloop.Start(ctx, monitorCh, attastCh)

	sigChan := make(chan os.Signal, 1)
	signal.Notify(sigChan, syscall.SIGINT, syscall.SIGTERM)
	go func() {
		<-sigChan
		cancel()
	}()

	<-ctx.Done()
}
