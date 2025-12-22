package solana

import (
	"encoding/base64"
	"strings"

	ics26router "github.com/cosmos/solidity-ibc-eureka/packages/go-anchor/ics26router"
)

// ParseWriteAcknowledgementEventsFromLogs parses WriteAcknowledgementEvent from Solana transaction logs
// using the auto-generated types from packages/go-anchor/ics26router.
func ParseWriteAcknowledgementEventsFromLogs(logs []string) ([]*ics26router.Ics26RouterEventsWriteAcknowledgementEvent, error) {
	var events []*ics26router.Ics26RouterEventsWriteAcknowledgementEvent

	for _, log := range logs {
		if !strings.HasPrefix(log, "Program data: ") {
			continue
		}

		dataStr := strings.TrimPrefix(log, "Program data: ")
		data, err := base64.StdEncoding.DecodeString(dataStr)
		if err != nil {
			continue // Not a valid base64 log, skip
		}

		if len(data) < 8 {
			continue // Too short to be an Anchor event
		}

		// Try to parse as WriteAcknowledgementEvent using auto-generated parser
		event, err := ics26router.ParseEvent_Ics26RouterEventsWriteAcknowledgementEvent(data)
		if err != nil {
			continue // Not a WriteAcknowledgementEvent or failed to parse
		}

		events = append(events, event)
	}

	return events, nil
}
