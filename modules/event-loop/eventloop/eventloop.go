package eventloop

import (
	"context"
	"errors"
	"fmt"
	"time"
)

type (
	MonitorEvent struct {
		ID          int32
		Sleep       time.Duration
		SimulateErr bool
	}

	AttastatorEvent struct {
		ID          int32
		Sleep       time.Duration // Simulate long running work
		SimulateErr bool
	}

	Notification struct {
		Typ string
		ID  int32
		Msg string
		Err error
	}
)

func newErrNotification(typ, err string, id int32) Notification {
	return Notification{
		Typ: typ,
		ID:  id,
		Err: errors.New(err),
	}
}

func newMsgNotification(typ, msg string, id int32) Notification {
	return Notification{
		Typ: typ,
		ID:  id,
		Msg: msg,
	}
}

func (n Notification) Error() string {
	return n.Err.Error()
}

func (n Notification) String() string {
	msg := n.Msg
	if n.Err != nil {
		msg = n.Error()
	}
	return fmt.Sprintf("[%-10s]: [%-3d] %-30s", n.Typ, n.ID, msg)
}

func Start(ctx context.Context, mCh <-chan MonitorEvent, aCh <-chan AttastatorEvent) <-chan Notification {
	notificationCh := make(chan Notification)

	go func() {
		defer close(notificationCh)

	LOOP:
		for {
			select {
			case <-ctx.Done():
				fmt.Printf("Stopping Server")
				break LOOP
			case mn := <-mCh:
				go handleMonitorEvent(ctx, mn, notificationCh)
			case att := <-aCh:
				go handleAttastatorEvent(ctx, att, notificationCh)
			}
		}
	}()
	return notificationCh
}

func handleMonitorEvent(ctx context.Context, monitorEvent MonitorEvent, notiCh chan<- Notification) {
	select {
	default:
	case <-ctx.Done():
		return
	}
	time.Sleep(monitorEvent.Sleep)

	if monitorEvent.SimulateErr {
		notiCh <- newErrNotification("Monitor", "Error", monitorEvent.ID)
		return
	}

	notiCh <- newMsgNotification("Monitor", fmt.Sprintf("Status - Done. Runtime - %-4d", monitorEvent.Sleep.Milliseconds()), monitorEvent.ID)
}

func handleAttastatorEvent(ctx context.Context, attastatorEvent AttastatorEvent, notiCh chan<- Notification) {
	select {
	default:
	case <-ctx.Done():
		return
	}
	time.Sleep(attastatorEvent.Sleep)

	if attastatorEvent.SimulateErr {
		notiCh <- newErrNotification("Attastator", "Error", attastatorEvent.ID)
		return
	}

	// Simulate a long running work
	notiCh <- newMsgNotification("Attastator", fmt.Sprintf("Status - Done. Runtime - %-4d", attastatorEvent.Sleep.Milliseconds()), attastatorEvent.ID)
}
