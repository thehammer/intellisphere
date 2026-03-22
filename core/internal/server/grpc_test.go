package server

import (
	"context"
	"errors"
	"testing"

	"go.uber.org/zap"
	"google.golang.org/grpc"
	"google.golang.org/grpc/codes"
	"google.golang.org/grpc/status"

	"github.com/thehammer/intellisphere/core/internal/adapter"
	pb "github.com/thehammer/intellisphere/core/proto/intellisphere/v1"
)

// --- helpers ---

func newTestServer(t *testing.T, responses []adapter.MockResponse) (*LLMCoreServer, *adapter.MockAdapter) {
	t.Helper()
	mock, err := adapter.NewMockAdapter(adapter.AdapterConfig{})
	if err != nil {
		t.Fatalf("NewMockAdapter: %v", err)
	}
	if responses != nil {
		mock.LoadResponses(responses)
	}
	srv := NewLLMCoreServer(mock, zap.NewNop())
	return srv, mock
}

// fakeStream implements grpc.ServerStreamingServer[pb.CompletionChunk] so we
// can test CompleteStream without a real gRPC transport.
type fakeStream struct {
	grpc.ServerStream
	ctx    context.Context
	chunks []*pb.CompletionChunk
	sendFn func(*pb.CompletionChunk) error // nil means default (append)
}

func newFakeStream(ctx context.Context) *fakeStream {
	return &fakeStream{ctx: ctx}
}

func (f *fakeStream) Send(chunk *pb.CompletionChunk) error {
	if f.sendFn != nil {
		return f.sendFn(chunk)
	}
	f.chunks = append(f.chunks, chunk)
	return nil
}

func (f *fakeStream) Context() context.Context {
	return f.ctx
}

// SetDeadline / RecvMsg etc. are provided by the embedded grpc.ServerStream
// (zero value), so they panic if called — that is acceptable for these tests.

// --- Complete tests ---

func TestLLMCoreServer_Complete_SimpleMessage(t *testing.T) {
	srv, _ := newTestServer(t, []adapter.MockResponse{
		{Content: "Hello!", StopReason: pb.StopReason_STOP_REASON_END_TURN, Usage: &pb.Usage{InputTokens: 5, OutputTokens: 3}},
	})

	req := &pb.CompletionRequest{
		Model: "mock",
		Messages: []*pb.Message{
			{Role: "user", Content: "Hi there"},
		},
		Metadata: &pb.RequestMetadata{RequestId: "req-001"},
	}

	resp, err := srv.Complete(context.Background(), req)
	if err != nil {
		t.Fatalf("Complete: %v", err)
	}
	if resp.Content != "Hello!" {
		t.Errorf("content = %q, want %q", resp.Content, "Hello!")
	}
	if resp.StopReason != pb.StopReason_STOP_REASON_END_TURN {
		t.Errorf("stop_reason = %v, want END_TURN", resp.StopReason)
	}
}

func TestLLMCoreServer_Complete_NoMetadata(t *testing.T) {
	// Ensure the server handles a nil Metadata field without panicking.
	srv, _ := newTestServer(t, nil)

	req := &pb.CompletionRequest{
		Messages: []*pb.Message{{Role: "user", Content: "hello"}},
	}
	resp, err := srv.Complete(context.Background(), req)
	if err != nil {
		t.Fatalf("Complete: %v", err)
	}
	if resp == nil {
		t.Fatal("response is nil")
	}
}

func TestLLMCoreServer_Complete_WithToolCalls(t *testing.T) {
	srv, _ := newTestServer(t, []adapter.MockResponse{
		{
			Content:    "",
			StopReason: pb.StopReason_STOP_REASON_TOOL_USE,
			ToolCalls: []*pb.ToolCall{
				{CallId: "c1", Name: "search", ArgumentsJson: `{"q":"go testing"}`},
			},
		},
	})

	resp, err := srv.Complete(context.Background(), &pb.CompletionRequest{
		Messages: []*pb.Message{{Role: "user", Content: "Search for something"}},
	})
	if err != nil {
		t.Fatalf("Complete: %v", err)
	}
	if resp.StopReason != pb.StopReason_STOP_REASON_TOOL_USE {
		t.Errorf("stop_reason = %v, want TOOL_USE", resp.StopReason)
	}
	if len(resp.ToolCalls) != 1 {
		t.Fatalf("len(tool_calls) = %d, want 1", len(resp.ToolCalls))
	}
	if resp.ToolCalls[0].Name != "search" {
		t.Errorf("tool_calls[0].name = %q, want %q", resp.ToolCalls[0].Name, "search")
	}
}

func TestLLMCoreServer_Complete_AdapterErrorMapped(t *testing.T) {
	// Use an adapter that always errors.  We can achieve this by wrapping the
	// server with a custom failing adapter rather than the mock.
	srv := NewLLMCoreServer(&alwaysErrorAdapter{msg: "rate limit exceeded"}, zap.NewNop())

	_, err := srv.Complete(context.Background(), &pb.CompletionRequest{
		Messages: []*pb.Message{{Role: "user", Content: "hi"}},
	})
	if err == nil {
		t.Fatal("expected error, got nil")
	}
	st, ok := status.FromError(err)
	if !ok {
		t.Fatalf("error is not a gRPC status: %v", err)
	}
	if st.Code() != codes.ResourceExhausted {
		t.Errorf("code = %v, want ResourceExhausted", st.Code())
	}
}

// --- CompleteStream tests ---

func TestLLMCoreServer_CompleteStream_SendsChunks(t *testing.T) {
	srv, _ := newTestServer(t, []adapter.MockResponse{
		{Content: "streaming response", StopReason: pb.StopReason_STOP_REASON_END_TURN},
	})

	stream := newFakeStream(context.Background())
	err := srv.CompleteStream(&pb.CompletionRequest{
		Messages: []*pb.Message{{Role: "user", Content: "stream this"}},
	}, stream)
	if err != nil {
		t.Fatalf("CompleteStream: %v", err)
	}

	if len(stream.chunks) != 2 {
		t.Fatalf("received %d chunks, want 2", len(stream.chunks))
	}
	if stream.chunks[0].ChunkType != pb.ChunkType_CHUNK_TYPE_TEXT_DELTA {
		t.Errorf("chunk[0].type = %v, want TEXT_DELTA", stream.chunks[0].ChunkType)
	}
	if stream.chunks[0].Delta != "streaming response" {
		t.Errorf("chunk[0].delta = %q, want %q", stream.chunks[0].Delta, "streaming response")
	}
	if stream.chunks[1].ChunkType != pb.ChunkType_CHUNK_TYPE_MESSAGE_END {
		t.Errorf("chunk[1].type = %v, want MESSAGE_END", stream.chunks[1].ChunkType)
	}
}

func TestLLMCoreServer_CompleteStream_SendErrorMapsToInternal(t *testing.T) {
	srv, _ := newTestServer(t, nil)

	stream := newFakeStream(context.Background())
	stream.sendFn = func(_ *pb.CompletionChunk) error {
		return errors.New("transport closed")
	}

	err := srv.CompleteStream(&pb.CompletionRequest{
		Messages: []*pb.Message{{Role: "user", Content: "hi"}},
	}, stream)
	if err == nil {
		t.Fatal("expected error from send failure, got nil")
	}
	st, ok := status.FromError(err)
	if !ok {
		t.Fatalf("not a gRPC status: %v", err)
	}
	if st.Code() != codes.Internal {
		t.Errorf("code = %v, want Internal", st.Code())
	}
}

func TestLLMCoreServer_CompleteStream_CancelledContext(t *testing.T) {
	srv, _ := newTestServer(t, nil)

	ctx, cancel := context.WithCancel(context.Background())
	cancel() // cancel immediately

	stream := newFakeStream(ctx)
	err := srv.CompleteStream(&pb.CompletionRequest{
		Messages: []*pb.Message{{Role: "user", Content: "hi"}},
	}, stream)
	// The mock adapter finishes synchronously, so the stream may have already
	// completed before the cancelled context is checked.  We accept both nil
	// and a Canceled status error here.
	if err != nil {
		st, ok := status.FromError(err)
		if !ok {
			t.Fatalf("unexpected non-status error: %v", err)
		}
		if st.Code() != codes.Canceled {
			t.Errorf("code = %v, want Canceled", st.Code())
		}
	}
}

// --- Health tests ---

func TestLLMCoreServer_Health(t *testing.T) {
	srv, _ := newTestServer(t, nil)

	resp, err := srv.Health(context.Background(), &pb.HealthRequest{})
	if err != nil {
		t.Fatalf("Health: %v", err)
	}
	if !resp.Healthy {
		t.Errorf("Healthy = false, want true")
	}
	if resp.Provider != "mock" {
		t.Errorf("Provider = %q, want %q", resp.Provider, "mock")
	}
}

// --- mapAdapterError tests ---

func TestMapAdapterError(t *testing.T) {
	cases := []struct {
		name     string
		errMsg   string
		wantCode codes.Code
	}{
		{"rate limit keyword", "rate limit exceeded", codes.ResourceExhausted},
		{"429 status code", "HTTP 429 too many requests", codes.ResourceExhausted},
		{"unauthorized keyword", "unauthorized access", codes.Unauthenticated},
		{"401 status code", "HTTP 401 from server", codes.Unauthenticated},
		{"invalid api key", "invalid api key provided", codes.Unauthenticated},
		{"timeout keyword", "request timeout after 30s", codes.DeadlineExceeded},
		{"deadline keyword", "context deadline exceeded", codes.DeadlineExceeded},
		{"500 status code", "HTTP 500 internal server error", codes.Unavailable},
		{"502 status code", "received 502 bad gateway", codes.Unavailable},
		{"503 status code", "service 503 unavailable", codes.Unavailable},
		{"unknown error", "something unexpected happened", codes.Internal},
	}

	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			err := mapAdapterError(errors.New(tc.errMsg))
			st, ok := status.FromError(err)
			if !ok {
				t.Fatalf("mapAdapterError did not return a gRPC status: %v", err)
			}
			if st.Code() != tc.wantCode {
				t.Errorf("code = %v, want %v (input: %q)", st.Code(), tc.wantCode, tc.errMsg)
			}
		})
	}
}

// --- test doubles ---

// alwaysErrorAdapter is a minimal LLMAdapter that always returns an error.
type alwaysErrorAdapter struct {
	msg string
}

func (a *alwaysErrorAdapter) Complete(_ context.Context, _ *pb.CompletionRequest) (*pb.CompletionResponse, error) {
	return nil, errors.New(a.msg)
}

func (a *alwaysErrorAdapter) CompleteStream(_ context.Context, _ *pb.CompletionRequest) (<-chan *pb.CompletionChunk, <-chan error) {
	chunkCh := make(chan *pb.CompletionChunk)
	errCh := make(chan error, 1)
	close(chunkCh)
	errCh <- errors.New(a.msg)
	close(errCh)
	return chunkCh, errCh
}

func (a *alwaysErrorAdapter) Health(_ context.Context) (*pb.HealthResponse, error) {
	return nil, errors.New(a.msg)
}

func (a *alwaysErrorAdapter) Provider() string { return "always-error" }
