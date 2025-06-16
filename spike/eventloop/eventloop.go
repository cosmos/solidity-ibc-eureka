package eventloop

import (
	"container/heap"
	"context"
	"fmt"
	"slices"
	"sync"
	"time"

	"github.com/cosmos/solidity-ibc-eureka/attastator"
	"github.com/cosmos/solidity-ibc-eureka/monitor"
)

type EventLoop struct {
	DataRotation       time.Duration
	AttastationStorage map[int64][]attastator.L2State
	timeToHeight       *MinHeap
	mu                 sync.Mutex
}

func New(rotation time.Duration) *EventLoop {
	return &EventLoop{
		DataRotation:       rotation,
		timeToHeight:       NewMinHeap(nil),
		mu:                 sync.Mutex{},
		AttastationStorage: make(map[int64][]attastator.L2State),
	}
}

func (e *EventLoop) Start(ctx context.Context, attInterval, monitorIntarval time.Duration, attastator *attastator.L2Proxy) {
	tkrAtt := time.NewTicker(attInterval)
	tkrMon := time.NewTicker(monitorIntarval)

	go func() {
		defer tkrAtt.Stop()
		defer tkrMon.Stop()

		for {
			select {
			case <-ctx.Done():
				return
			case <-tkrAtt.C:
				state := attastator.QueryL2()
				if state.Err != nil {
					fmt.Printf("Error From Attastator: %v\n", state.Err)
					continue
				}
				e.StoreL2State(state)
			case <-tkrMon.C:
				status := monitor.QueryL2()
				if status != nil {
					fmt.Printf("Error From Monitor: %v\n", status)
				}
			}
		}
	}()
}

func (e *EventLoop) StoreL2State(state attastator.L2State) {
	e.mu.Lock()
	defer e.mu.Unlock()

	e.AttastationStorage[state.Height] = append(e.AttastationStorage[state.Height], state)
	heap.Push(e.timeToHeight, HeightTs{TimeStamp: state.TimeStamp, Height: state.Height})
}

func (e *EventLoop) L2States(h int64) []attastator.L2State {
	e.mu.Lock()
	defer e.mu.Unlock()
	ret := slices.Clone(e.AttastationStorage[h])
	return ret
}
