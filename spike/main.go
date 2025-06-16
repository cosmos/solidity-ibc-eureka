package main

import (
	"context"
	"time"

	"github.com/cosmos/solidity-ibc-eureka/attastator"
	"github.com/cosmos/solidity-ibc-eureka/eventloop"
)

func main() {
	ctx, cancel := context.WithCancel(context.Background())
	blockTime := time.Second * 5
	attastator := attastator.New(blockTime, 0)
	attastator.Start(ctx)

	eventLoop := eventloop.New(blockTime * 5)
	eventLoop.Start(ctx, time.Second, time.Second*3, attastator)
	eventLoop.DataRotationService(ctx)

	time.Sleep(time.Second * 20)
	cancel()
}
