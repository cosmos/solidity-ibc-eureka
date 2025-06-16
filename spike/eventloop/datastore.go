package eventloop

import (
	"container/heap"
	"context"
	"fmt"
	"slices"
	"time"

	"github.com/cosmos/solidity-ibc-eureka/attastator"
)

// HeightTs combines a timestamp and a height.
type HeightTs struct {
	TimeStamp time.Time
	Height    int64
}

// MinHeap implements a min-heap of HeightTs based on TimeStamp.
type MinHeap []HeightTs

func (h MinHeap) Len() int            { return len(h) }
func (h MinHeap) Less(i, j int) bool  { return h[i].TimeStamp.Before(h[j].TimeStamp) }
func (h MinHeap) Swap(i, j int)       { h[i], h[j] = h[j], h[i] }
func (h *MinHeap) Push(x interface{}) { *h = append(*h, x.(HeightTs)) }
func (h *MinHeap) Pop() interface{} {
	old := *h
	n := len(old)
	x := old[n-1]
	*h = old[:n-1]
	return x
}

func (h *MinHeap) Peak() time.Time {
	if h.Len() == 0 {
		return time.Now()
	}
	return (*h)[0].TimeStamp
}

// NewMinHeap initializes a heap with the given items.
func NewMinHeap(items []HeightTs) *MinHeap {
	h := MinHeap(items)
	heap.Init(&h)
	return &h
}

func (e *EventLoop) DataRotationService(ctx context.Context) {
	tkr := time.NewTicker(e.DataRotation)
	go func() {
		defer tkr.Stop()
		for {
			select {
			case <-ctx.Done():
				return
			case <-tkr.C:
				e.mu.Lock()
				for ts := e.timeToHeight.Peak(); time.Since(ts) >= e.DataRotation; ts = e.timeToHeight.Peak() {
					select {
					case <-ctx.Done():
						return
					default:
					}
					top := heap.Pop(e.timeToHeight).(HeightTs)
					recents := make([]attastator.L2State, 0)
					for _, st := range e.AttastationStorage[top.Height] {
						if time.Since(st.TimeStamp) >= e.DataRotation {
							fmt.Printf("Discarding: %s\n", st.StateRoot)
							continue
						}
						recents = append(recents, st)
					}
					if len(recents) == 0 {
						delete(e.AttastationStorage, top.Height)
					} else {
						e.AttastationStorage[top.Height] = recents
					}
				}
				e.mu.Unlock()
			}
		}
	}()
}

// For testing
func (e *EventLoop) DumpData() []attastator.L2State {
	e.mu.Lock()
	defer e.mu.Unlock()
	ret := make([]attastator.L2State, 0)
	for _, v := range e.AttastationStorage {
		ret = append(ret, v...)
	}
	slices.SortFunc(ret, func(a, b attastator.L2State) int {
		cmp := a.TimeStamp.Compare(b.TimeStamp)
		if cmp != 0 {
			return cmp
		}
		if a.Height < b.Height {
			return -1
		}
		return 1
	})
	return ret
}
