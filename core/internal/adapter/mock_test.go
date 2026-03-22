package adapter

import (
	"context"
	"testing"

	pb "github.com/thehammer/intellisphere/core/proto/intellisphere/v1"
)

func newTestMockAdapter(t *testing.T) *MockAdapter {
	t.Helper()
	a, err := NewMockAdapter(AdapterConfig{})
	if err != nil {
		t.Fatalf("NewMockAdapter: %v", err)
	}
	return a
}

func TestMockAdapter_Provider(t *testing.T) {
	a := newTestMockAdapter(t)
	if got := a.Provider(); got != "mock" {
		t.Errorf("Provider() = %q, want %q", got, "mock")
	}
}

// TestMockAdapter_Complete_SequentialResponses verifies that successive calls
// to Complete return the configured responses in order.
func TestMockAdapter_Complete_SequentialResponses(t *testing.T) {
	a := newTestMockAdapter(t)
	a.LoadResponses([]MockResponse{
		{Content: "first", StopReason: pb.StopReason_STOP_REASON_END_TURN},
		{Content: "second", StopReason: pb.StopReason_STOP_REASON_END_TURN},
		{Content: "third", StopReason: pb.StopReason_STOP_REASON_TOOL_USE},
	})

	ctx := context.Background()
	req := &pb.CompletionRequest{}

	for _, want := range []string{"first", "second", "third"} {
		resp, err := a.Complete(ctx, req)
		if err != nil {
			t.Fatalf("Complete: %v", err)
		}
		if resp.Content != want {
			t.Errorf("Complete() content = %q, want %q", resp.Content, want)
		}
		if resp.Model != "mock" {
			t.Errorf("Complete() model = %q, want %q", resp.Model, "mock")
		}
	}
}

// TestMockAdapter_Complete_FallbackWhenExhausted verifies that calls beyond the
// configured response list return the fallback message instead of an error.
func TestMockAdapter_Complete_FallbackWhenExhausted(t *testing.T) {
	a := newTestMockAdapter(t)
	a.LoadResponses([]MockResponse{
		{Content: "only one", StopReason: pb.StopReason_STOP_REASON_END_TURN},
	})

	ctx := context.Background()
	req := &pb.CompletionRequest{}

	// Consume the single configured response.
	if _, err := a.Complete(ctx, req); err != nil {
		t.Fatalf("first Complete: %v", err)
	}

	// All subsequent calls should return the fallback, never an error.
	for i := 0; i < 3; i++ {
		resp, err := a.Complete(ctx, req)
		if err != nil {
			t.Fatalf("fallback Complete #%d: %v", i+1, err)
		}
		wantContent := "Mock: no more responses configured"
		if resp.Content != wantContent {
			t.Errorf("fallback call #%d: content = %q, want %q", i+1, resp.Content, wantContent)
		}
		if resp.StopReason != pb.StopReason_STOP_REASON_END_TURN {
			t.Errorf("fallback call #%d: stop_reason = %v, want END_TURN", i+1, resp.StopReason)
		}
	}
}

// TestMockAdapter_Complete_ToolCallsInResponse verifies that MockResponse tool
// calls are propagated correctly to the returned CompletionResponse.
func TestMockAdapter_Complete_ToolCallsInResponse(t *testing.T) {
	a := newTestMockAdapter(t)
	a.LoadResponses([]MockResponse{
		{
			Content:    "",
			StopReason: pb.StopReason_STOP_REASON_TOOL_USE,
			ToolCalls: []*pb.ToolCall{
				{CallId: "call-1", Name: "get_weather", ArgumentsJson: `{"city":"London"}`},
			},
			Usage: &pb.Usage{InputTokens: 20, OutputTokens: 5},
		},
	})

	resp, err := a.Complete(context.Background(), &pb.CompletionRequest{})
	if err != nil {
		t.Fatalf("Complete: %v", err)
	}
	if resp.StopReason != pb.StopReason_STOP_REASON_TOOL_USE {
		t.Errorf("stop_reason = %v, want TOOL_USE", resp.StopReason)
	}
	if len(resp.ToolCalls) != 1 {
		t.Fatalf("len(tool_calls) = %d, want 1", len(resp.ToolCalls))
	}
	tc := resp.ToolCalls[0]
	if tc.CallId != "call-1" || tc.Name != "get_weather" {
		t.Errorf("tool call = {%q, %q}, want {call-1, get_weather}", tc.CallId, tc.Name)
	}
	if resp.Usage == nil || resp.Usage.InputTokens != 20 {
		t.Errorf("usage not propagated correctly: %+v", resp.Usage)
	}
}

// TestMockAdapter_LoadResponses_ResetsSequence verifies that LoadResponses
// resets the call index so the sequence starts over.
func TestMockAdapter_LoadResponses_ResetsSequence(t *testing.T) {
	a := newTestMockAdapter(t)
	ctx := context.Background()
	req := &pb.CompletionRequest{}

	a.LoadResponses([]MockResponse{
		{Content: "alpha", StopReason: pb.StopReason_STOP_REASON_END_TURN},
	})

	resp, _ := a.Complete(ctx, req)
	if resp.Content != "alpha" {
		t.Fatalf("expected alpha, got %q", resp.Content)
	}

	// Reload with a different set — index must reset to 0.
	a.LoadResponses([]MockResponse{
		{Content: "beta", StopReason: pb.StopReason_STOP_REASON_END_TURN},
		{Content: "gamma", StopReason: pb.StopReason_STOP_REASON_END_TURN},
	})

	for _, want := range []string{"beta", "gamma"} {
		resp, err := a.Complete(ctx, req)
		if err != nil {
			t.Fatalf("Complete after reload: %v", err)
		}
		if resp.Content != want {
			t.Errorf("after reload: content = %q, want %q", resp.Content, want)
		}
	}
}

// TestMockAdapter_CompleteStream_ReturnsChunks verifies that CompleteStream
// emits a text-delta chunk followed by a message-end chunk.
func TestMockAdapter_CompleteStream_ReturnsChunks(t *testing.T) {
	a := newTestMockAdapter(t)
	a.LoadResponses([]MockResponse{
		{
			Content:    "streamed content",
			StopReason: pb.StopReason_STOP_REASON_END_TURN,
			Usage:      &pb.Usage{InputTokens: 10, OutputTokens: 4},
		},
	})

	chunkCh, errCh := a.CompleteStream(context.Background(), &pb.CompletionRequest{})

	var chunks []*pb.CompletionChunk
	for c := range chunkCh {
		chunks = append(chunks, c)
	}

	// Drain error channel.
	for range errCh {
	}

	if len(chunks) != 2 {
		t.Fatalf("received %d chunks, want 2", len(chunks))
	}

	textChunk := chunks[0]
	if textChunk.ChunkType != pb.ChunkType_CHUNK_TYPE_TEXT_DELTA {
		t.Errorf("chunk[0].type = %v, want TEXT_DELTA", textChunk.ChunkType)
	}
	if textChunk.Delta != "streamed content" {
		t.Errorf("chunk[0].delta = %q, want %q", textChunk.Delta, "streamed content")
	}

	endChunk := chunks[1]
	if endChunk.ChunkType != pb.ChunkType_CHUNK_TYPE_MESSAGE_END {
		t.Errorf("chunk[1].type = %v, want MESSAGE_END", endChunk.ChunkType)
	}
	if endChunk.StopReason != pb.StopReason_STOP_REASON_END_TURN {
		t.Errorf("chunk[1].stop_reason = %v, want END_TURN", endChunk.StopReason)
	}
	if endChunk.Usage == nil || endChunk.Usage.InputTokens != 10 {
		t.Errorf("chunk[1].usage not propagated: %+v", endChunk.Usage)
	}
}

// TestMockAdapter_CompleteStream_NoErrorOnSuccess verifies that the error
// channel is closed without sending any error for a successful stream.
func TestMockAdapter_CompleteStream_NoErrorOnSuccess(t *testing.T) {
	a := newTestMockAdapter(t)

	_, errCh := a.CompleteStream(context.Background(), &pb.CompletionRequest{})

	for err := range errCh {
		if err != nil {
			t.Errorf("unexpected error on stream: %v", err)
		}
	}
}

// TestMockAdapter_Health_AlwaysHealthy verifies that Health always reports the
// adapter as healthy regardless of configuration.
func TestMockAdapter_Health_AlwaysHealthy(t *testing.T) {
	a := newTestMockAdapter(t)

	resp, err := a.Health(context.Background())
	if err != nil {
		t.Fatalf("Health: %v", err)
	}
	if !resp.Healthy {
		t.Errorf("Health().Healthy = false, want true")
	}
	if resp.Provider != "mock" {
		t.Errorf("Health().Provider = %q, want %q", resp.Provider, "mock")
	}
	if resp.Message == "" {
		t.Errorf("Health().Message is empty")
	}
}

// TestMockAdapter_DefaultResponse verifies the built-in default response
// is returned when no LoadResponses call has been made.
func TestMockAdapter_DefaultResponse(t *testing.T) {
	a := newTestMockAdapter(t)

	resp, err := a.Complete(context.Background(), &pb.CompletionRequest{})
	if err != nil {
		t.Fatalf("Complete: %v", err)
	}
	if resp.Content == "" {
		t.Errorf("default response content is empty")
	}
	if resp.StopReason != pb.StopReason_STOP_REASON_END_TURN {
		t.Errorf("default stop_reason = %v, want END_TURN", resp.StopReason)
	}
}

// TestMockAdapter_ToProto verifies the MockResponse.ToProto helper sets the
// model field to "mock" and passes fields through unchanged.
func TestMockAdapter_ToProto(t *testing.T) {
	mr := MockResponse{
		Content:    "hello",
		StopReason: pb.StopReason_STOP_REASON_MAX_TOKENS,
		Usage:      &pb.Usage{InputTokens: 3, OutputTokens: 7},
	}
	p := mr.ToProto()
	if p.Model != "mock" {
		t.Errorf("Model = %q, want %q", p.Model, "mock")
	}
	if p.Content != "hello" {
		t.Errorf("Content = %q, want %q", p.Content, "hello")
	}
	if p.StopReason != pb.StopReason_STOP_REASON_MAX_TOKENS {
		t.Errorf("StopReason = %v, want MAX_TOKENS", p.StopReason)
	}
}
