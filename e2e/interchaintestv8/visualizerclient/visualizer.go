package visualizerclient

import (
	"bytes"
	"encoding/json"
	"fmt"
	"net/http"
	"time"
)

type VisualizerClient struct {
	client *http.Client
	port   uint
	testID string
}

func NewVisualizerClient(port uint, testName string) *VisualizerClient {
	generatedTestID := fmt.Sprintf("%s_%d", testName, time.Now().UnixNano())
	v := &VisualizerClient{
		client: &http.Client{},
		port:   port,
		testID: generatedTestID,
	}

	v.SendMessage("Visualizer client connected", "setup")

	return v
}

func (v *VisualizerClient) SendMessage(message string, subtestID string) {
	msg := VisualizationData{
		Text:      message,
		TestID:    v.testID,
		SubTestID: subtestID,
		Panel:     PanelDashboard,
	}
	if err := v.sendMessage(msg); err != nil {
		fmt.Println("Failed to send message to visualizer:", err)
	}
}

func (v *VisualizerClient) SendPopupMessage(message string, subtestID string) {
	msg := VisualizationData{
		Text:      message,
		TestID:    v.testID,
		SubTestID: subtestID,
		Panel:     PanelPopup,
	}
	if err := v.sendMessage(msg); err != nil {
		fmt.Println("Failed to send message to visualizer:", err)
	}
}

func (v *VisualizerClient) SendNetworkUpdateMessage(subtestID string, networkState NetworkState) {
	msg := VisualizationData{
		Text:         "Network update",
		TestID:       v.testID,
		SubTestID:    subtestID,
		Panel:        PanelDashboard,
		NetworkState: networkState,
	}

	if err := v.sendMessage(msg); err != nil {
		fmt.Println("Failed to send message to visualizer:", err)
	}
}

func (v *VisualizerClient) sendMessage(msg VisualizationData) error {
	url := fmt.Sprintf("http://localhost:%d/message", v.port)
	jsonData, err := json.Marshal(msg)
	if err != nil {
		return err
	}

	req, err := http.NewRequest("POST", url, bytes.NewBuffer(jsonData))
	if err != nil {
		return err
	}
	req.Header.Set("Content-Type", "application/json")

	resp, err := v.client.Do(req)
	if err != nil {
		return err
	}
	defer resp.Body.Close()

	if resp.StatusCode != http.StatusOK {
		return fmt.Errorf("failed to send message, status code: %d", resp.StatusCode)
	}

	return nil
}
