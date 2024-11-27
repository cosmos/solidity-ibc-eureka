package relayer

import (
	"os"
	"text/template"
)

type ConfigInfo struct {
	TMRPCURL      string
	ICS26Address  string
	ETHRPCURL     string
	PrivateKey    string
	ProofType     string
	SP1PrivateKey string
}

func (c *ConfigInfo) GenerateConfigFile(path string) error {
	tmpl, err := template.ParseFiles("config.tmpl")
	if err != nil {
		return err
	}

	f, err := os.Create(path)
	if err != nil {
		return err
	}

	defer f.Close()

	return tmpl.Execute(f, c)
}
