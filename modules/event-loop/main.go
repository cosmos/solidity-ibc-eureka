package main

import (
	"encoding/json"
	"fmt"
	"log"
	"net/http"
	"sync"
	"time"
)

const (
	ChannelSize      = 1000
	ConcurrencyLimit = 24
)

// Event types
type Event interface{ isEvent() }
type MonitoringEvent struct{ Data MonitoringData }

func (MonitoringEvent) isEvent() {}

type AttestorEvent struct{ Data AttestorData }

func (AttestorEvent) isEvent() {}

type MonitoringData struct{}
type AttestorData struct{}

// Dummy interfaces (swap in real implementations)
type DummyMonitorer interface {
	GetMonitoringResults() error
}
type DummyAttestor interface {
	GetL2Data() error
}
type PrintMonitorer struct{}

func (PrintMonitorer) GetMonitoringResults() error {
	fmt.Println("[PrintMonitorer] fetching monitoring results…")
	return nil
}

type PrintAttestor struct{}

func (PrintAttestor) GetL2Data() error {
	fmt.Println("[PrintAttestor] fetching L2 data…")
	return nil
}

// Your server, holding channels and config
type Server struct {
	port      int
	monTokens chan struct{}
	attTokens chan struct{}
	events    chan Event
}

func NewServer(port int) *Server {
	return &Server{
		port:      port,
		monTokens: make(chan struct{}, ChannelSize),
		attTokens: make(chan struct{}, ChannelSize),
		events:    make(chan Event, ChannelSize),
	}
}

func (s *Server) Start(mon DummyMonitorer, att DummyAttestor) {
	// 1) HTTP endpoints
	http.HandleFunc("/monitoring", func(w http.ResponseWriter, r *http.Request) {
		select {
		case s.monTokens <- struct{}{}:
			json.NewEncoder(w).Encode(map[string]string{"message": "monitoring event created"})
		default:
			http.Error(w, "server busy", http.StatusServiceUnavailable)
		}
	})
	http.HandleFunc("/l2", func(w http.ResponseWriter, r *http.Request) {
		select {
		case s.attTokens <- struct{}{}:
			json.NewEncoder(w).Encode(map[string]string{"message": "L2 endpoint up and running"})
		default:
			http.Error(w, "server busy", http.StatusServiceUnavailable)
		}
	})

	// 2) Background services
	go s.monitoringService(mon)
	go s.attestorService(att)

	// 3) Worker‐pool dispatch
	var wg sync.WaitGroup
	sem := make(chan struct{}, ConcurrencyLimit)

	go func() {
		for ev := range s.events {
			sem <- struct{}{} // acquire slot
			wg.Add(1)
			go func(evt Event) {
				defer wg.Done()
				defer func() { <-sem }() // release slot

				// “processing” switch
				switch e := evt.(type) {
				case MonitoringEvent:
					time.Sleep(time.Duration(time.Duration.Seconds(3)))
					log.Printf("Processed MonitoringEvent: %+v\n", e.Data)
				case AttestorEvent:
					time.Sleep(time.Duration(time.Duration.Seconds(1)))
					log.Printf("Processed AttestorEvent: %+v\n", e.Data)
				default:
					log.Printf("Unknown event: %+v\n", e)
				}
			}(ev)
		}
	}()

	// 4) Run HTTP server
	addr := fmt.Sprintf(":%d", s.port)
	log.Printf("Listening on %s\n", addr)
	if err := http.ListenAndServe(addr, nil); err != nil {
		log.Fatal(err)
	}

	// Wait for all in‐flight events if we ever close s.events
	wg.Wait()
}

func (s *Server) monitoringService(mon DummyMonitorer) {
	log.Println("Monitoring service started")
	for range s.monTokens {
		// simulate I/O or compute
		if err := mon.GetMonitoringResults(); err != nil {
			log.Println("monitor error:", err)
		}
		s.events <- MonitoringEvent{Data: MonitoringData{}}
	}
}

func (s *Server) attestorService(att DummyAttestor) {
	log.Println("Attestor service started")
	for range s.attTokens {
		if err := att.GetL2Data(); err != nil {
			log.Println("attestor error:", err)
		}
		s.events <- AttestorEvent{Data: AttestorData{}}
	}
}

func main() {

	server := NewServer(8080)
	server.Start(PrintMonitorer{}, PrintAttestor{})
}
