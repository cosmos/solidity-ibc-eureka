package wasm

const (
	releaseAPI                       = "https://api.github.com/repos/cosmos/solidity-ibc-eureka/releases"
	wasmEthLightClientTagPrefix      = "cw-ics08-wasm-eth-"
	wasmAttestorLightClientTagPrefix = "cw-ics08-wasm-attestor-"
)

type Release struct {
	TagName string `json:"tag_name"`
}

func (r Release) BaseDownloadURL() string {
	return "https://github.com/cosmos/solidity-ibc-eureka/releases/download/" + r.TagName
}
