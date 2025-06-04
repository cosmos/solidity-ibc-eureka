package wasm

import (
	"encoding/json"
	"errors"
	"net/http"
	"strings"

	"golang.org/x/mod/semver"
)

const (
	releaseAPI                  = "https://api.github.com/repos/cosmos/solidity-ibc-eureka/releases"
	wasmEthLightClientTagPrefix = "cw-ics08-wasm-eth-"
)

type Release struct {
	TagName string `json:"tag_name"`
}

func (r Release) BaseDownloadURL() string {
	return "https://github.com/cosmos/solidity-ibc-eureka/releases/download/" + r.TagName
}

func GetLatestEthLightClientRelease() (Release, error) {
	releases, err := GetAllWasmEthLightClientReleases()
	if err != nil {
		return Release{}, err
	}

	var latestRelease Release
	for _, release := range releases {
		if strings.HasPrefix(release.TagName, wasmEthLightClientTagPrefix) {
			latestRelease = release
			break
		}
	}
	if latestRelease.TagName == "" {
		return Release{}, errors.New("no release found with tag prefix " + wasmEthLightClientTagPrefix)
	}

	return latestRelease, nil
}

func GetAllWasmEthLightClientReleases() ([]Release, error) {
	resp, err := http.Get(releaseAPI)
	if err != nil {
		return nil, err
	}
	defer resp.Body.Close()

	if resp.StatusCode != http.StatusOK {
		return nil, errors.New("failed to fetch releases: http status code " + resp.Status)
	}

	var allReleases []Release
	err = json.NewDecoder(resp.Body).Decode(&allReleases)
	if err != nil {
		return nil, err
	}

	// Filter releases to include only those with the eth wasm light client tag prefix
	var filteredReleases []Release
	for _, release := range allReleases {
		if strings.HasPrefix(release.TagName, wasmEthLightClientTagPrefix) {
			filteredReleases = append(filteredReleases, release)
		}
	}

	if len(filteredReleases) == 0 {
		return nil, errors.New("no eth light client releases found")
	}

	return filteredReleases, nil
}

func GetAllEthLightClientReleasesFromVersion(version string) ([]Release, error) {
	if !semver.IsValid(version) {
		return nil, errors.New("invalid version format: " + version)
	}

	releases, err := GetAllWasmEthLightClientReleases()
	if err != nil {
		return nil, err
	}

	var filteredReleases []Release
	for _, release := range releases {
		releaseSemver := strings.TrimPrefix(release.TagName, wasmEthLightClientTagPrefix)
		if !semver.IsValid(releaseSemver) {
			return nil, errors.New("invalid semver tag in release: " + release.TagName)
		}

		// Include only releases that are greater than or equal to the specified version
		if semver.Compare(releaseSemver, version) >= 0 {
			filteredReleases = append(filteredReleases, release)
		}
	}

	if len(filteredReleases) == 0 {
		return nil, errors.New("no releases found after version " + version)
	}

	return filteredReleases, nil
}
