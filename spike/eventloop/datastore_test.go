package eventloop_test

import (
	"container/heap"
	"testing"
	"time"

	"github.com/cosmos/solidity-ibc-eureka/eventloop"
)

// Helper to compare two time.Time values exactly
func timesEqual(a, b time.Time) bool {
	return a.Equal(b)
}

func TestNewMinHeapAndPeak(t *testing.T) {
	// Prepare deterministic timestamps
	t1 := time.Date(2025, 6, 14, 0, 0, 0, 0, time.UTC) // oldest
	t2 := time.Date(2025, 6, 15, 0, 0, 0, 0, time.UTC)
	t3 := time.Date(2025, 6, 16, 0, 0, 0, 0, time.UTC) // newest

	// Unordered input
	items := []eventloop.HeightTs{
		{TimeStamp: t3, Height: 3},
		{TimeStamp: t1, Height: 1},
		{TimeStamp: t2, Height: 2},
	}

	h := eventloop.NewMinHeap(items)

	// The earliest timestamp should be at the root
	peak := h.Peak()
	if !timesEqual(peak, t1) {
		t.Errorf("Peak() = %v; want %v", peak, t1)
	}
}

func TestPushPopOrder(t *testing.T) {
	// Prepare deterministic timestamps
	t1 := time.Date(2025, 1, 1, 0, 0, 0, 0, time.UTC)
	t2 := time.Date(2025, 2, 1, 0, 0, 0, 0, time.UTC)
	t3 := time.Date(2025, 3, 1, 0, 0, 0, 0, time.UTC)

	// Start with an empty heap
	h := eventloop.NewMinHeap(nil)

	// Push in reverse order
	heap.Push(h, eventloop.HeightTs{TimeStamp: t3, Height: 3})
	heap.Push(h, eventloop.HeightTs{TimeStamp: t1, Height: 1})
	heap.Push(h, eventloop.HeightTs{TimeStamp: t2, Height: 2})

	// After pushes, Peak() should give the oldest (t1)
	if got := h.Peak(); !timesEqual(got, t1) {
		t.Errorf("After pushes, Peak() = %v; want %v", got, t1)
	}

	// Pop in order and verify ascending timestamp / height
	expected := []struct {
		ts     time.Time
		height int64
	}{
		{t1, 1},
		{t2, 2},
		{t3, 3},
	}

	for i, exp := range expected {
		x := heap.Pop(h).(eventloop.HeightTs)
		if !timesEqual(x.TimeStamp, exp.ts) {
			t.Errorf("Pop #%d: timestamp = %v; want %v", i, x.TimeStamp, exp.ts)
		}
		if x.Height != exp.height {
			t.Errorf("Pop #%d: height = %d; want %d", i, x.Height, exp.height)
		}
	}

	// Heap should now be empty
	if h.Len() != 0 {
		t.Errorf("Len() after pops = %d; want 0", h.Len())
	}
}

func TestPeakEmptyHeap(t *testing.T) {
	h := eventloop.NewMinHeap(nil)

	// Peak on empty returns time.Now(); we assert it's within a small window
	before := time.Now()
	got := h.Peak()
	after := time.Now()

	if got.Before(before) || got.After(after) {
		t.Errorf("Peak() on empty = %v; want between %v and %v", got, before, after)
	}
}
