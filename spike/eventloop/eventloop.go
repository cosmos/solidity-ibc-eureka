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

func (e *EventLoop) Start(ctx context.Context, attInterval, monitorIntarval time.Duration, attastator *attastator.Server) {
	e.DataRotationService(ctx)
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
				// fmt.Printf("Querying: ")
				state := attastator.QueryL2()
				// fmt.Printf("Got back: %s\n", state.StateRoot)
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

	// fmt.Printf("Before Data Add: %v\n\n", e.AttastationStorage)
	e.AttastationStorage[state.Height] = append(e.AttastationStorage[state.Height], state)
	heap.Push(e.timeToHeight, HeightTs{TimeStamp: state.TimeStamp, Height: state.Height})
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

func (e *EventLoop) L2State(h int64) []attastator.L2State {
	e.mu.Lock()
	defer e.mu.Unlock()
	ret := slices.Clone(e.AttastationStorage[h])
	return ret
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

// func Start(ctx context.Context, attServers []*attastator.Server, workers int) {
// 	// Start N daemons TODO: Later

// 	eventCh := make(chan AttsEv)
// 	defer close(eventCh)

// 	go func() {
// 		for i, a := range attServers {
// 			i := i
// 			go func(a *attastator.Server) {
// 				tkr := time.NewTicker(time.Second * time.Duration(i+1))
// 				defer tkr.Stop()
// 				for {
// 					select {
// 					case <-ctx.Done():
// 						return
// 					case <-tkr.C:
// 						events := a.Query()
// 						for _, e := range events {
// 							select {
// 							case <-ctx.Done():
// 								return
// 							case eventCh <- AttsEv{ID: e.ID, Busy: e.Busy, AttasServerID: a.ID}:
// 							}
// 						}
// 					}
// 				}
// 			}(a)
// 		}
// 	}()
// 	for ev := range eventCh {
// 		fmt.Println(ev.String())
// 	}
// }

// func attasEvProcessor(ctx context.Context, pulse time.Duration, eventCh <-chan AttsEv) (<-chan string, <-chan string, <-chan error) {
// 	id := ctx.Value("id").(string)
// 	hbCh := make(chan string)
// 	notiCh := make(chan string)
// 	errCh := make(chan error)
// 	go func() {
// 		defer close(hbCh)
// 		defer close(notiCh)
// 		defer close(errCh)
// 		for {
// 			select {
// 			case <-ctx.Done():
// 				return
// 			case e, ok := <-eventCh:
// 				if !ok {
// 					return
// 				}
// 				go func(e AttsEv, errCh chan error) {
// 					if e.Busy < 0 {
// 						panic(fmt.Sprintf("Processor: [%s] Panicked on Event: [%d]", id, e.ID))
// 					}
// 				}(e, errCh)
// 			}
// 		}
// 	}()
// 	return hbCh, notiCh, errCh
// }

// func demon(timeout time.Duration, processor eventProcessor) eventProcessor {
// 	return func(ctx context.Context, pulse time.Duration, eventCh <-chan AttsEv) (<-chan string, <-chan string, <-chan error) {
// 		var hbCh <-chan string
// 		var notiCh <-chan string
// 		var errCh <-chan error

// 		go func() {
// 			restart := func(ctx context.Context) context.CancelFunc {
// 				processorCtx, cancel := context.WithCancel(ctx)
// 				hbCh, notiCh, errCh = processor(processorCtx, pulse, eventCh)
// 				return cancel
// 			}

// 			processCancel := restart(ctx)
// 			timeoutCh := time.NewTicker(timeout)
// 			defer timeoutCh.Stop()

// 		loop:
// 			for {
// 				timeoutCh.Reset(timeout)
// 				for {
// 					select {
// 					case <-ctx.Done():
// 						return
// 					case <-hbCh:
// 						continue loop
// 					case <-timeoutCh.C:
// 						processCancel()
// 						processCancel = restart(ctx)
// 					case err := <-errCh:
// 						fmt.Printf("Handle Error on demon Or move upward. Err: [%s]", err.Error())
// 					}
// 				}
// 			}

// 		}()

// 		return hbCh, notiCh, errCh
// 	}
// }
