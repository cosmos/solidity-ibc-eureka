package wasm

type Release struct {
	TagName string `json:"tag_name"`
}

func (r Release) BaseDownloadURL() string {
	return "https://github.com/cosmos/solidity-ibc-eureka/releases/download/" + r.TagName
}
