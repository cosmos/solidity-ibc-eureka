package eventloop_test

import (
	"context"
	"fmt"
	"math/rand"
	"sync/atomic"
	"testing"
	"time"

	"github.com/cosmos/solidity-ibc-eureka/eventloop"
)

func TestEventLoop(t *testing.T) {
	event := func() string {
		if n := rand.Intn(2); n == 0 {
			return "monitor"
		}
		return "att"
	}

	// 1/5 errors.
	shouldError := func() bool {
		return rand.Intn(100) <= 20
	}

	monitorCh := make(chan eventloop.MonitorEvent)
	attastCh := make(chan eventloop.AttastatorEvent)
	var monitorID, attastID atomic.Int32

	ctx, cancel := context.WithCancel(t.Context())
	defer cancel()

	notifications := eventloop.Start(ctx, monitorCh, attastCh)

	sendMsg := func(n int) {
		for range n {
			switch event() {
			case "monitor":
				ev := eventloop.MonitorEvent{
					ID:          monitorID.Load(),
					Sleep:       (time.Millisecond * time.Duration(rand.Intn(7000))),
					SimulateErr: shouldError(),
				}
				monitorCh <- ev
				monitorID.Add(1)
			case "att":
				ev := eventloop.AttastatorEvent{
					ID:          attastID.Load(),
					Sleep:       (time.Millisecond * time.Duration(rand.Intn(7000))),
					SimulateErr: shouldError(),
				}
				attastCh <- ev
				attastID.Add(1)
			}
		}
	}

	// Total 1500 events.
	go sendMsg(100)
	go sendMsg(200)
	go sendMsg(300)
	go sendMsg(400)
	go sendMsg(500)

	var notiCnt int
	for n := range notifications {
		notiCnt++
		fmt.Printf("%-30s Notification: [%d]\n", n.String(), notiCnt)
		if notiCnt == 1500 {
			cancel()
		}
	}
}
