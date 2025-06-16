package eventloop

import (
	"context"
	"fmt"
	"sync"
	"time"

	"github.com/cosmos/solidity-ibc-eureka/attastator"
	"github.com/cosmos/solidity-ibc-eureka/monitor"
)

type EventLoop struct {
	DataRotation       time.Duration
	AttastationStorage map[int64]attastator.L2State
	timeToHeight       *MinHeap
	mu                 sync.Mutex
}

func New(rotation time.Duration) *EventLoop {
	return &EventLoop{
		DataRotation:       rotation,
		timeToHeight:       NewMinHeap(nil),
		mu:                 sync.Mutex{},
		AttastationStorage: make(map[int64]attastator.L2State),
	}
}

func (e *EventLoop) Start(ctx context.Context, attInterval, monitorIntarval time.Duration, attastator *attastator.Server) {
	e.DataRotationService(ctx)
	tkrAtt := time.NewTicker(attInterval)
	tkrMon := time.NewTicker(monitorIntarval)

	go func() {
		defer tkrAtt.Stop()
		defer tkrMon.Stop()

	Loop:
		for {
			select {
			case <-ctx.Done():
				return
			case <-tkrAtt.C:
				state := attastator.QueryL2()
				if state.Err != nil {
					fmt.Printf("Error From Attastator: %v\n", state.Err)
					continue Loop
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

	e.AttastationStorage[state.Height] = state
	e.timeToHeight.Push(HeightTs{TimeStamp: state.TimeStamp, Height: state.Height})
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
				for ts := e.timeToHeight.Peak(); time.Since(ts) >= e.DataRotation; {
					top := e.timeToHeight.Pop().(HeightTs)
					delete(e.AttastationStorage, top.Height)
				}
				e.mu.Unlock()
			}
		}
	}()
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
