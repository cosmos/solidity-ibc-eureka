package monitor

import (
	"errors"
	"math/rand"
	"time"
)

func QueryL2() error {
	rnd := rand.New(rand.NewSource(time.Now().UnixNano()))
	// Simulate query to L2 LOL.
	time.Sleep(time.Second)
	if rnd.Int31n(2) == 0 {
		return errors.New("Error monitoring L2 Full node")
	}
	return nil
}
