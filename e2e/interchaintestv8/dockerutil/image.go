package dockerutil

import "os"

const (
	// DefaultAttestorImage is the default Docker image for the attestor.
	DefaultAttestorImage = "ghcr.io/cosmos/ibc-attestor:v0.2.0"
	// EnvKeyAttestorImage is the environment variable to override the Docker image.
	EnvKeyAttestorImage = "IBC_ATTESTOR_IMAGE"
)

// GetImage returns the Docker image to use for attestor containers.
// It checks the IBC_EUREKA_IMAGE environment variable first, falling back to DefaultImage.
func GetAttestorImage() string {
	if img := os.Getenv(EnvKeyAttestorImage); img != "" {
		return img
	}
	return DefaultAttestorImage
}
