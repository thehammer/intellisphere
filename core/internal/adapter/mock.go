package adapter

import (
	"context"
	"sync"

	pb "github.com/thehammer/intellisphere/core/proto/intellisphere/v1"
)

// MockAdapter returns deterministic responses for testing.
// Responses are returned in sequence. Useful for testing multi-turn tool loops.
type MockAdapter struct {
	responses []MockResponse
	mu        sync.Mutex
	callIndex int
}

// MockResponse is a pre-configured response for the mock adapter.
type MockResponse struct {
	Content    string
	ToolCalls  []*pb.ToolCall
	StopReason pb.StopReason
	Usage      *pb.Usage
}

func (r *MockResponse) ToProto() *pb.CompletionResponse {
	return &pb.CompletionResponse{
		Content:    r.Content,
		ToolCalls:  r.ToolCalls,
		StopReason: r.StopReason,
		Usage:      r.Usage,
		Model:      "mock",
	}
}

// NewMockAdapter creates a mock adapter. Responses can be loaded via LoadResponses.
func NewMockAdapter(_ AdapterConfig) (*MockAdapter, error) {
	return &MockAdapter{
		responses: []MockResponse{
			{
				Content:    "Hello! I'm a mock LLM response.",
				StopReason: pb.StopReason_STOP_REASON_END_TURN,
				Usage:      &pb.Usage{InputTokens: 10, OutputTokens: 8},
			},
		},
	}, nil
}

func (m *MockAdapter) Provider() string {
	return "mock"
}

// LoadResponses sets the response sequence for the mock adapter.
func (m *MockAdapter) LoadResponses(responses []MockResponse) {
	m.mu.Lock()
	defer m.mu.Unlock()
	m.responses = responses
	m.callIndex = 0
}

func (m *MockAdapter) Complete(_ context.Context, _ *pb.CompletionRequest) (*pb.CompletionResponse, error) {
	m.mu.Lock()
	defer m.mu.Unlock()

	if m.callIndex >= len(m.responses) {
		return &pb.CompletionResponse{
			Content:    "Mock: no more responses configured",
			StopReason: pb.StopReason_STOP_REASON_END_TURN,
			Usage:      &pb.Usage{InputTokens: 5, OutputTokens: 6},
			Model:      "mock",
		}, nil
	}

	resp := m.responses[m.callIndex].ToProto()
	m.callIndex++
	return resp, nil
}

func (m *MockAdapter) CompleteStream(_ context.Context, req *pb.CompletionRequest) (<-chan *pb.CompletionChunk, <-chan error) {
	chunkCh := make(chan *pb.CompletionChunk, 8)
	errCh := make(chan error, 1)

	go func() {
		defer close(chunkCh)
		defer close(errCh)

		resp, _ := m.Complete(context.Background(), req)

		// Stream the response as a single text delta + message end
		chunkCh <- &pb.CompletionChunk{
			Delta:     resp.Content,
			ChunkType: pb.ChunkType_CHUNK_TYPE_TEXT_DELTA,
		}
		chunkCh <- &pb.CompletionChunk{
			StopReason: resp.StopReason,
			Usage:      resp.Usage,
			ChunkType:  pb.ChunkType_CHUNK_TYPE_MESSAGE_END,
		}
	}()

	return chunkCh, errCh
}

func (m *MockAdapter) Health(_ context.Context) (*pb.HealthResponse, error) {
	return &pb.HealthResponse{
		Healthy:  true,
		Provider: "mock",
		Message:  "Mock adapter is always healthy",
	}, nil
}
