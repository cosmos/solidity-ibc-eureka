package eventloop_test

import (
	"context"
	"fmt"
	"testing"
	"time"

	"github.com/cosmos/solidity-ibc-eureka/attastator"
	"github.com/cosmos/solidity-ibc-eureka/eventloop"
)

func TestEventLoop(t *testing.T) {
	// Careate an event loop with 10 second data retention
	loop := eventloop.New(time.Second * 10)
	ctx, cancel := context.WithCancel(t.Context())

	// Simulate an L2 with 4 second block time.
	att := attastator.New(time.Second*6, 1)
	att.Start(ctx)

	loop.Start(ctx, time.Second, time.Second*4, att)

	tk := time.After(time.Second * 50)
	dump := time.NewTicker(time.Second * 4)

Loop:
	for {
		select {
		case <-tk:
			cancel()
			break Loop
		case <-dump.C:
			allState := loop.DumpData()
			fmt.Printf("\n-----------------------------\nAll States\n")
			pretty(t, allState)
			fmt.Printf("-----------------------------\n\n")
		}
	}
}

func pretty(t *testing.T, data []attastator.L2State) {
	t.Helper()
	for _, d := range data {
		fmt.Printf("Height: %d  Root: %q TimeStamp: %s\n", d.Height, d.StateRoot, d.TimeStamp.String())
	}
}
