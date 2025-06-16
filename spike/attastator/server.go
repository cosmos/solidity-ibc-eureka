package attastator

import (
	"context"
	cryptorand "crypto/rand"
	"fmt"
	"math/rand"
	"sync"
	"time"
)

type Server struct {
	MonotonicHeight int64
	BlockTime       time.Duration
	mu              sync.Mutex
}

type L2State struct {
	Height    int64
	StateRoot string
	Signature []byte
	TimeStamp time.Time
	Err       error
}

func New(blockTime time.Duration, initialHeight int64) *Server {
	return &Server{
		MonotonicHeight: initialHeight,
		BlockTime:       blockTime,
		mu:              sync.Mutex{},
	}
}

func (s *Server) Start(ctx context.Context) {
	tkr := time.NewTicker(s.BlockTime)
	go func() {
		defer tkr.Stop()
		for {
			select {
			case <-tkr.C:
				s.mu.Lock()
				s.MonotonicHeight++
				s.mu.Unlock()
			case <-ctx.Done():
				return
			}
		}
	}()
}

func (s *Server) QueryL2() (state L2State) {
	rnd := rand.New(rand.NewSource(time.Now().UnixNano()))
	defer func() {
		if r := recover(); r != nil {
			state.Err = fmt.Errorf("panic: %v", r)
		}
	}()

	// Simulate Query to L2 and attastation
	busyLoop := rnd.Int63n(10000000000)

	cnt := int64(0)
	// Simulate Long running attastation Process
	for range busyLoop {
		cnt++
	}
	if busyLoop < 3000000000 {
		panic("AAAAAAA")
	}

	s.mu.Lock()
	h := s.MonotonicHeight
	s.mu.Unlock()

	state = L2State{
		Height:    h,
		StateRoot: RandString(32),
		Signature: RandomBytes(5),
		TimeStamp: time.Now(),
		Err:       nil,
	}
	return state
}

// letters is the allowed character set for the generated string.
const letters = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789"

// RandString generates a random string of length n.
// It is fast but not safe for cryptographic purposes.
func RandString(n int) string {
	rand := rand.New(rand.NewSource(time.Now().UnixNano()))
	b := make([]byte, n)
	for i := range b {
		b[i] = letters[rand.Intn(len(letters))]
	}
	return string(b)
}

// RandomBytes returns a slice of n random bytes using crypto/rand.
func RandomBytes(n int) []byte {
	b := make([]byte, n)
	if _, err := cryptorand.Read(b); err != nil {
		panic(err)
	}
	return b
}
