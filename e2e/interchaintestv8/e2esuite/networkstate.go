package e2esuite

import (
	"time"

	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/visualizerclient"
)

func (s *TestSuite) SetArrow(index int, direction string) int {
	var arrowType visualizerclient.ElementType
	if direction == "right" {
		arrowType = visualizerclient.ElementTypeArrowRight
	} else if direction == "left" {
		arrowType = visualizerclient.ElementTypeArrowLeft
	} else {
		panic("Invalid direction")
	}

	s.CurrentNetworkState.Elements[index] = visualizerclient.Element{
		Name:        "arrow",
		ElementType: arrowType,
	}
	s.updateNetworkState()

	return index
}

func (s *TestSuite) RemoveArrows() {
	for i := range s.CurrentNetworkState.Elements {
		if s.CurrentNetworkState.Elements[i].ElementType == visualizerclient.ElementTypeArrowRight ||
			s.CurrentNetworkState.Elements[i].ElementType == visualizerclient.ElementTypeArrowLeft {
			s.CurrentNetworkState.Elements[i] = createSpacer()
		}
	}

	s.updateNetworkState()
}

func (s *TestSuite) AddLightClient(index int, lightClientName string) {
	s.CurrentNetworkState.Elements[index].InnerboxName = lightClientName
	s.updateNetworkState()
}

func (s *TestSuite) AddRelayerToVisualizer() {
	s.CurrentNetworkState.Elements = []visualizerclient.Element{
		s.CurrentNetworkState.Elements[0],
		createSpacer(),
		{
			Name:         "Relayer",
			ElementType:  visualizerclient.ElementTypeBox,
			ElementColor: visualizerclient.ColorGreen,
		},
		createSpacer(),
		s.CurrentNetworkState.Elements[2],
	}

	s.updateNetworkState()

	go func() {
		time.Sleep(5 * time.Second)
		s.CurrentNetworkState.Elements[3].ElementColor = ""
		s.updateNetworkState()
	}()
}

func (s *TestSuite) Focus(index int, statusText string) {
	s.RemoveColors()
	s.RemoveStatusTexts()
	s.CurrentNetworkState.Elements[index].ElementColor = visualizerclient.ColorGreen
	s.CurrentNetworkState.Elements[index].StatusText = statusText
	s.CurrentNetworkState.Elements[index].StatusColor = visualizerclient.ColorGreen
	s.updateNetworkState()
}

func (s *TestSuite) RemoveColors() {
	for i := range s.CurrentNetworkState.Elements {
		s.CurrentNetworkState.Elements[i].ElementColor = ""
		s.CurrentNetworkState.Elements[i].StatusColor = ""
	}

	s.updateNetworkState()
}

func (s *TestSuite) RemoveStatusTexts() {
	for i := range s.CurrentNetworkState.Elements {
		s.CurrentNetworkState.Elements[i].StatusText = ""
	}

	s.updateNetworkState()
}

func (s *TestSuite) setNotStartedNetworkState() {
	s.CurrentNetworkState.Elements = []visualizerclient.Element{
		{
			Name:         "Chain A",
			ElementType:  visualizerclient.ElementTypeBox,
			ElementColor: visualizerclient.ColorRed,
			StatusText:   "Starting up...",
			StatusColor:  visualizerclient.ColorRed,
		},
		{
			Name:        "spacer",
			ElementType: visualizerclient.ElementTypeHorizontalSpacer,
		},
		{
			Name:         "EVM Chain",
			ElementType:  visualizerclient.ElementTypeBox,
			ElementColor: visualizerclient.ColorRed,
			StatusText:   "Starting up...",
			StatusColor:  visualizerclient.ColorRed,
		},
	}

	s.updateNetworkState()
}

func (s *TestSuite) setStartedNetworkState() {
	s.CurrentNetworkState.Elements = []visualizerclient.Element{
		{
			Name:         "Chain A",
			ElementType:  visualizerclient.ElementTypeBox,
			ElementColor: visualizerclient.ColorGreen,
			StatusText:   "Started",
			StatusColor:  visualizerclient.ColorGreen,
		},
		{
			Name:        "spacer",
			ElementType: visualizerclient.ElementTypeHorizontalSpacer,
		},
		{
			Name:         "EVM Chain",
			ElementType:  visualizerclient.ElementTypeBox,
			ElementColor: visualizerclient.ColorGreen,
			StatusText:   "Started",
			StatusColor:  visualizerclient.ColorGreen,
		},
	}

	s.updateNetworkState()
}

func (s *TestSuite) updateNetworkState() {
	s.VisualizerClient.SendNetworkUpdateMessage(s.T().Name(), *s.CurrentNetworkState)
}

func createSpacer() visualizerclient.Element {
	return visualizerclient.Element{
		Name:        "spacer",
		ElementType: visualizerclient.ElementTypeHorizontalSpacer,
	}
}
