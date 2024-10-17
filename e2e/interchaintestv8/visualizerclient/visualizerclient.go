package visualizerclient

import (
	"bytes"
	"encoding/json"
	"fmt"
	"net/http"
	"time"
)

type VisulizationPanel string

const (
	Dashboard VisulizationPanel = "dashboard"
	Stdin     VisulizationPanel = "stdin"
)

type VisualizationData struct {
	Text      string            `json:"text"`
	TestID    string            `json:"test_id"`
	SubTestID string            `json:"sub_test_id"`
	Panel     VisulizationPanel `json:"visulization_panel"`
}

type VisualizerClient struct {
	client *http.Client
	port   uint
	active bool
	testID string
}

func NewVisualizerClient(port uint, testName string) *VisualizerClient {
	generatedTestID := fmt.Sprintf("%s_%d", testName, time.Now().UnixNano())
	v := &VisualizerClient{
		client: &http.Client{},
		port:   port,
		active: true,
		testID: generatedTestID,
	}

	// Send an initial message to check if the server is available
	testMsg := VisualizationData{
		Text:   "Visualizer client connected",
		TestID: v.testID,
	}
	if err := v.sendMessage(testMsg); err != nil {
		fmt.Println("Visualizer client not available, disabling")
		v.active = false
	}

	return v
}

func (v *VisualizerClient) SendMessage(message string, subtestID string) {
	msg := VisualizationData{
		Text:      message,
		TestID:    v.testID,
		SubTestID: subtestID,
		Panel:     Dashboard,
	}
	if err := v.sendMessage(msg); err != nil {
		fmt.Println("Failed to send message to visualizer:", err)
	}
}

func (v *VisualizerClient) sendMessage(msg VisualizationData) error {
	if !v.active {
		return nil
	}

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
