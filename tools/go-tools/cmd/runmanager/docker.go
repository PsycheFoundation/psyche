package main

import (
	"bufio"
	"context"
	"encoding/json"
	"fmt"
	"io"
	"os"

	"github.com/docker/docker/api/types/container"
	"github.com/docker/docker/api/types/image"
	"github.com/docker/docker/client"
)

type DockerManager struct {
	client *client.Client
}

func NewDockerManager() (*DockerManager, error) {
	cli, err := client.NewClientWithOpts(client.FromEnv, client.WithAPIVersionNegotiation())
	if err != nil {
		return nil, fmt.Errorf("failed to create Docker client: %w", err)
	}

	return &DockerManager{client: cli}, nil
}

func (d *DockerManager) Close() error {
	return d.client.Close()
}

func (d *DockerManager) PullImage(ctx context.Context, imageName string) error {
	fmt.Printf("Pulling image: %s\n", imageName)

	reader, err := d.client.ImagePull(ctx, imageName, image.PullOptions{})
	if err != nil {
		return fmt.Errorf("failed to pull image: %w", err)
	}
	defer reader.Close()

	// Parse the JSON stream to detect errors
	scanner := bufio.NewScanner(reader)
	for scanner.Scan() {
		line := scanner.Bytes()
		fmt.Println(string(line))

		// Parse JSON to check for errors
		var response map[string]interface{}
		if err := json.Unmarshal(line, &response); err != nil {
			continue
		}

		// Check for error in the response
		if errorDetail, ok := response["errorDetail"]; ok {
			if errorMap, ok := errorDetail.(map[string]interface{}); ok {
				if msg, ok := errorMap["message"].(string); ok {
					return fmt.Errorf("image pull failed: %s", msg)
				}
			}
			return fmt.Errorf("image pull failed with unknown error")
		}
	}

	if err := scanner.Err(); err != nil {
		return fmt.Errorf("failed to read pull output: %w", err)
	}

	fmt.Printf("Successfully pulled image: %s\n", imageName)
	return nil
}

func (d *DockerManager) RunContainer(ctx context.Context, imageName string, envVars []string) (string, error) {
	fmt.Printf("Creating container from image: %s\n", imageName)
	fmt.Printf("Environment variables: %v\n", envVars)

	config := &container.Config{
		Image: imageName,
		Env:   envVars,
		Tty:   false,
	}

	hostConfig := &container.HostConfig{
		// For GPU support:
		// Resources: container.Resources{
		// 	DeviceRequests: []container.DeviceRequest{
		// 		{
		// 			Count:        -1, // all GPUs
		// 			Capabilities: [][]string{{"gpu"}},
		// 		},
		// 	},
		// },
	}

	resp, err := d.client.ContainerCreate(ctx, config, hostConfig, nil, nil, "")
	if err != nil {
		return "", fmt.Errorf("failed to create container: %w", err)
	}
	containerID := resp.ID
	fmt.Printf("Created container: %s\n", containerID)

	if err := d.client.ContainerStart(ctx, containerID, container.StartOptions{}); err != nil {
		return "", fmt.Errorf("failed to start container: %w", err)
	}

	fmt.Printf("Started container: %s\n", containerID)
	return containerID, nil
}

// Streams log to stdout
func (d *DockerManager) StreamLogs(ctx context.Context, containerID string) error {
	options := container.LogsOptions{
		ShowStdout: true,
		ShowStderr: true,
		Follow:     true,
	}

	reader, err := d.client.ContainerLogs(ctx, containerID, options)
	if err != nil {
		return fmt.Errorf("failed to get container logs: %w", err)
	}
	defer reader.Close()

	_, err = io.Copy(os.Stdout, reader)
	return err
}
