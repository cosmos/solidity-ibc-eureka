package eventloop

import (
	"container/heap"
	"time"
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
		return time.Now().Add(time.Duration(time.Now().Year()))
	}
	return (*h)[0].TimeStamp
}

// NewMinHeap initializes a heap with the given items.
func NewMinHeap(items []HeightTs) *MinHeap {
	h := MinHeap(items)
	heap.Init(&h)
	return &h
}
