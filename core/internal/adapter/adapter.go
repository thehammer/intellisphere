package adapter

import (
	"context"
	"fmt"

	pb "github.com/thehammer/intellisphere/core/proto/intellisphere/v1"
)

// AdapterConfig holds common configuration for LLM adapters.
type AdapterConfig struct {
	APIKey       string
	DefaultModel string
}

// LLMAdapter is the interface that all LLM provider adapters must implement.
type LLMAdapter interface {
	// Complete sends a completion request and returns a single response.
	Complete(ctx context.Context, req *pb.CompletionRequest) (*pb.CompletionResponse, error)

	// CompleteStream sends a completion request and returns a channel of chunks.
	CompleteStream(ctx context.Context, req *pb.CompletionRequest) (<-chan *pb.CompletionChunk, <-chan error)

	// Health checks the adapter's connectivity to the LLM provider.
	Health(ctx context.Context) (*pb.HealthResponse, error)

	// Provider returns the name of this adapter's provider.
	Provider() string
}

// NewAdapter creates an LLM adapter for the given provider.
func NewAdapter(provider string, config AdapterConfig) (LLMAdapter, error) {
	switch provider {
	case "anthropic":
		return NewAnthropicAdapter(config)
	case "mock":
		return NewMockAdapter(config)
	default:
		return nil, fmt.Errorf("unsupported LLM provider: %s", provider)
	}
}
