package dockerutil

import (
	"context"
	"fmt"

	"github.com/docker/docker/api/types/volume"
)

func (d *Docker) CreateVolume(ctx context.Context) (volume.Volume, error) {

	v, err := d.Client.VolumeCreate(ctx, volume.CreateOptions{
		Labels: map[string]string{RunLabel: d.RunLabelValue},
	})
	if err != nil {
		return volume.Volume{}, fmt.Errorf("Failed to create volume: %w", err)
	}

	return v, nil
}

func (d *Docker) Volumes(ctx context.Context) ([]*volume.Volume, error) {
	vs, err := d.Client.VolumeList(ctx, volume.ListOptions{
		Filters: d.RunLabelFilter(),
	})
	if err != nil {
		return nil, fmt.Errorf("failed to list volumes: %w", err)
	}

	return vs.Volumes, nil
}
