package visualizerclient

type Color string
type VisulizationPanel string

const (
	PanelDashboard VisulizationPanel = "dashboard"
	PanelRawOutput VisulizationPanel = "rawoutput"
	PanelPopup     VisulizationPanel = "popup"

	ColorRed   Color = "red"
	ColorGreen Color = "green"
	ColorGray  Color = "gray"
)

type VisualizationData struct {
	Text         string            `json:"text"`
	TestID       string            `json:"test_id"`
	SubTestID    string            `json:"sub_test_id"`
	Panel        VisulizationPanel `json:"visulization_panel"`
	NetworkState NetworkState      `json:"network_state"`
}

type NetworkState struct {
	Name     string    `json:"name"`
	Elements []Element `json:"elements"`
}

type ElementType string

const (
	ElementTypeBox              ElementType = "box"
	ElementTypeArrowRight       ElementType = "arrowright"
	ElementTypeArrowLeft        ElementType = "arrowleft"
	ElementTypeHorizontalSpacer ElementType = "horizontalspacer"
)

type Element struct {
	Name         string      `json:"name"`
	ElementType  ElementType `json:"element_type"`
	ElementColor Color       `json:"element_color"`
	InnerboxName string      `json:"innerbox_name"`
	StatusText   string      `json:"status_text"`
	StatusColor  Color       `json:"status_color"`
}
