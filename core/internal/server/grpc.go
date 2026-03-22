package server

import (
	"context"

	"go.uber.org/zap"
	"google.golang.org/grpc"
	"google.golang.org/grpc/codes"
	"google.golang.org/grpc/status"

	"github.com/thehammer/intellisphere/core/internal/adapter"
	pb "github.com/thehammer/intellisphere/core/proto/intellisphere/v1"
)

// LLMCoreServer implements the LLMCore gRPC service.
type LLMCoreServer struct {
	pb.UnimplementedLLMCoreServer
	adapter adapter.LLMAdapter
	logger  *zap.Logger
}

// NewLLMCoreServer creates a new LLMCoreServer.
func NewLLMCoreServer(llmAdapter adapter.LLMAdapter, logger *zap.Logger) *LLMCoreServer {
	return &LLMCoreServer{
		adapter: llmAdapter,
		logger:  logger,
	}
}

// RegisterLLMCoreServer registers the service with a gRPC server.
func RegisterLLMCoreServer(s *grpc.Server, srv *LLMCoreServer) {
	pb.RegisterLLMCoreServer(s, srv)
}

func (s *LLMCoreServer) Complete(ctx context.Context, req *pb.CompletionRequest) (*pb.CompletionResponse, error) {
	requestID := ""
	if req.Metadata != nil {
		requestID = req.Metadata.RequestId
	}
	s.logger.Info("Completion request",
		zap.String("request_id", requestID),
		zap.String("model", req.Model),
		zap.Int("message_count", len(req.Messages)),
	)

	resp, err := s.adapter.Complete(ctx, req)
	if err != nil {
		s.logger.Error("Completion failed", zap.String("request_id", requestID), zap.Error(err))
		return nil, mapAdapterError(err)
	}

	s.logger.Info("Completion succeeded",
		zap.String("request_id", requestID),
		zap.String("stop_reason", resp.StopReason.String()),
		zap.Int("tool_calls", len(resp.ToolCalls)),
	)
	return resp, nil
}

func (s *LLMCoreServer) CompleteStream(req *pb.CompletionRequest, stream pb.LLMCore_CompleteStreamServer) error {
	requestID := ""
	if req.Metadata != nil {
		requestID = req.Metadata.RequestId
	}
	s.logger.Info("Stream completion request",
		zap.String("request_id", requestID),
		zap.String("model", req.Model),
	)

	chunkCh, errCh := s.adapter.CompleteStream(stream.Context(), req)

	for {
		select {
		case chunk, ok := <-chunkCh:
			if !ok {
				return nil
			}
			if err := stream.Send(chunk); err != nil {
				s.logger.Error("Stream send failed", zap.String("request_id", requestID), zap.Error(err))
				return status.Error(codes.Internal, "failed to send stream chunk")
			}
		case err, ok := <-errCh:
			if ok && err != nil {
				s.logger.Error("Stream error", zap.String("request_id", requestID), zap.Error(err))
				return mapAdapterError(err)
			}
		case <-stream.Context().Done():
			return status.Error(codes.Canceled, "client disconnected")
		}
	}
}

func (s *LLMCoreServer) Health(ctx context.Context, _ *pb.HealthRequest) (*pb.HealthResponse, error) {
	return s.adapter.Health(ctx)
}

// mapAdapterError converts adapter errors to appropriate gRPC status codes.
func mapAdapterError(err error) error {
	errMsg := err.Error()

	// Map common error patterns to gRPC codes
	switch {
	case contains(errMsg, "rate limit") || contains(errMsg, "429"):
		return status.Error(codes.ResourceExhausted, errMsg)
	case contains(errMsg, "unauthorized") || contains(errMsg, "401") || contains(errMsg, "invalid api key"):
		return status.Error(codes.Unauthenticated, errMsg)
	case contains(errMsg, "timeout") || contains(errMsg, "deadline"):
		return status.Error(codes.DeadlineExceeded, errMsg)
	case contains(errMsg, "500") || contains(errMsg, "502") || contains(errMsg, "503"):
		return status.Error(codes.Unavailable, errMsg)
	default:
		return status.Error(codes.Internal, errMsg)
	}
}

func contains(s, substr string) bool {
	return len(s) >= len(substr) && (s == substr || len(s) > 0 && containsLower(s, substr))
}

func containsLower(s, substr string) bool {
	for i := 0; i <= len(s)-len(substr); i++ {
		if s[i:i+len(substr)] == substr {
			return true
		}
	}
	return false
}
