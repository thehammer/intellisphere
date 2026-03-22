package adapter

import (
	"context"
	"encoding/json"
	"fmt"
	"time"

	"github.com/liushuangls/go-anthropic/v2"

	pb "github.com/thehammer/intellisphere/core/proto/intellisphere/v1"
)

// AnthropicAdapter implements LLMAdapter for the Anthropic Claude API.
type AnthropicAdapter struct {
	client       *anthropic.Client
	defaultModel string
}

// NewAnthropicAdapter creates a new Anthropic adapter.
func NewAnthropicAdapter(config AdapterConfig) (*AnthropicAdapter, error) {
	if config.APIKey == "" {
		return nil, fmt.Errorf("ANTHROPIC_API_KEY is required")
	}

	client := anthropic.NewClient(config.APIKey)
	return &AnthropicAdapter{
		client:       client,
		defaultModel: config.DefaultModel,
	}, nil
}

func (a *AnthropicAdapter) Provider() string {
	return "anthropic"
}

func (a *AnthropicAdapter) Complete(ctx context.Context, req *pb.CompletionRequest) (*pb.CompletionResponse, error) {
	anthropicReq := a.buildRequest(req)

	callCtx, cancel := context.WithTimeout(ctx, 30*time.Second)
	defer cancel()

	resp, err := a.client.CreateMessages(callCtx, anthropicReq)
	if err != nil {
		return nil, fmt.Errorf("anthropic API error: %w", err)
	}

	return convertFromAnthropicResponse(&resp), nil
}

func (a *AnthropicAdapter) CompleteStream(ctx context.Context, req *pb.CompletionRequest) (<-chan *pb.CompletionChunk, <-chan error) {
	chunkCh := make(chan *pb.CompletionChunk, 32)
	errCh := make(chan error, 1)

	go func() {
		defer close(chunkCh)
		defer close(errCh)

		anthropicReq := a.buildRequest(req)

		callCtx, cancel := context.WithTimeout(ctx, 60*time.Second)
		defer cancel()

		streamReq := anthropic.MessagesStreamRequest{
			MessagesRequest: anthropicReq,
			OnContentBlockDelta: func(data anthropic.MessagesEventContentBlockDeltaData) {
				if data.Delta.Text != nil {
					chunk := &pb.CompletionChunk{
						Delta:     *data.Delta.Text,
						ChunkType: pb.ChunkType_CHUNK_TYPE_TEXT_DELTA,
					}
					select {
					case chunkCh <- chunk:
					case <-callCtx.Done():
					}
				}
			},
			OnMessageDelta: func(data anthropic.MessagesEventMessageDeltaData) {
				chunk := &pb.CompletionChunk{
					ChunkType: pb.ChunkType_CHUNK_TYPE_MESSAGE_END,
					Usage: &pb.Usage{
						OutputTokens: int32(data.Usage.OutputTokens),
					},
				}
				// Map stop reason
				if data.Delta.StopReason != "" {
					switch data.Delta.StopReason {
					case anthropic.MessagesStopReasonEndTurn:
						chunk.StopReason = pb.StopReason_STOP_REASON_END_TURN
					case anthropic.MessagesStopReasonToolUse:
						chunk.StopReason = pb.StopReason_STOP_REASON_TOOL_USE
					case anthropic.MessagesStopReasonMaxTokens:
						chunk.StopReason = pb.StopReason_STOP_REASON_MAX_TOKENS
					case anthropic.MessagesStopReasonStopSequence:
						chunk.StopReason = pb.StopReason_STOP_REASON_STOP_SEQUENCE
					}
				}
				select {
				case chunkCh <- chunk:
				case <-callCtx.Done():
				}
			},
		}

		_, err := a.client.CreateMessagesStream(callCtx, streamReq)
		if err != nil {
			errCh <- fmt.Errorf("anthropic streaming error: %w", err)
		}
	}()

	return chunkCh, errCh
}

func (a *AnthropicAdapter) Health(ctx context.Context) (*pb.HealthResponse, error) {
	_, err := a.client.CreateMessages(ctx, anthropic.MessagesRequest{
		Model: anthropic.Model(a.defaultModel),
		Messages: []anthropic.Message{
			{
				Role: anthropic.RoleUser,
				Content: []anthropic.MessageContent{
					{Type: "text", Text: stringPtr("ping")},
				},
			},
		},
		MaxTokens: 1,
	})
	if err != nil {
		return &pb.HealthResponse{
			Healthy:  false,
			Provider: "anthropic",
			Message:  fmt.Sprintf("API check failed: %v", err),
		}, nil
	}

	return &pb.HealthResponse{
		Healthy:  true,
		Provider: "anthropic",
		Message:  "OK",
	}, nil
}

// buildRequest creates an Anthropic MessagesRequest from a proto CompletionRequest.
func (a *AnthropicAdapter) buildRequest(req *pb.CompletionRequest) anthropic.MessagesRequest {
	model := req.Model
	if model == "" {
		model = a.defaultModel
	}

	messages := make([]anthropic.Message, 0, len(req.Messages))
	for _, msg := range req.Messages {
		messages = append(messages, convertToAnthropicMessage(msg))
	}

	tools := make([]anthropic.ToolDefinition, 0, len(req.Tools))
	for _, t := range req.Tools {
		tools = append(tools, anthropic.ToolDefinition{
			Name:        t.Name,
			Description: t.Description,
			InputSchema: jsonToInputSchema(t.InputSchemaJson),
		})
	}

	anthropicReq := anthropic.MessagesRequest{
		Model:     anthropic.Model(model),
		Messages:  messages,
		MaxTokens: int(req.MaxTokens),
		System:    req.System,
	}
	if req.Temperature > 0 {
		temp := float32(req.Temperature)
		anthropicReq.Temperature = &temp
	}
	if len(tools) > 0 {
		anthropicReq.Tools = tools
	}
	if len(req.StopSequences) > 0 {
		anthropicReq.StopSequences = req.StopSequences
	}

	return anthropicReq
}

// --- Conversion helpers ---

func convertToAnthropicMessage(msg *pb.Message) anthropic.Message {
	role := anthropic.RoleUser
	if msg.Role == "assistant" {
		role = anthropic.RoleAssistant
	}

	content := []anthropic.MessageContent{}

	if msg.Content != "" {
		content = append(content, anthropic.MessageContent{
			Type: "text",
			Text: stringPtr(msg.Content),
		})
	}

	// Tool results (user role messages containing tool results)
	for _, tr := range msg.ToolResults {
		toolUseID := tr.CallId
		mc := anthropic.MessageContent{
			Type: "tool_result",
			MessageContentToolResult: &anthropic.MessageContentToolResult{
				ToolUseID: &toolUseID,
				Content: []anthropic.MessageContent{
					{Type: "text", Text: stringPtr(tr.ResultJson)},
				},
			},
		}
		if tr.IsError {
			isErr := true
			mc.MessageContentToolResult.IsError = &isErr
		}
		content = append(content, mc)
	}

	// Tool calls from assistant
	for _, tc := range msg.ToolCalls {
		content = append(content, anthropic.MessageContent{
			Type: "tool_use",
			MessageContentToolUse: &anthropic.MessageContentToolUse{
				ID:    tc.CallId,
				Name:  tc.Name,
				Input: json.RawMessage(tc.ArgumentsJson),
			},
		})
	}

	return anthropic.Message{
		Role:    role,
		Content: content,
	}
}

func convertFromAnthropicResponse(resp *anthropic.MessagesResponse) *pb.CompletionResponse {
	result := &pb.CompletionResponse{
		Model: string(resp.Model),
		Usage: &pb.Usage{
			InputTokens:  int32(resp.Usage.InputTokens),
			OutputTokens: int32(resp.Usage.OutputTokens),
		},
	}

	switch resp.StopReason {
	case anthropic.MessagesStopReasonEndTurn:
		result.StopReason = pb.StopReason_STOP_REASON_END_TURN
	case anthropic.MessagesStopReasonToolUse:
		result.StopReason = pb.StopReason_STOP_REASON_TOOL_USE
	case anthropic.MessagesStopReasonMaxTokens:
		result.StopReason = pb.StopReason_STOP_REASON_MAX_TOKENS
	case anthropic.MessagesStopReasonStopSequence:
		result.StopReason = pb.StopReason_STOP_REASON_STOP_SEQUENCE
	}

	for _, block := range resp.Content {
		switch block.Type {
		case "text":
			if block.Text != nil {
				result.Content += *block.Text
			}
		case "tool_use":
			if block.MessageContentToolUse != nil {
				result.ToolCalls = append(result.ToolCalls, &pb.ToolCall{
					CallId:        block.ID,
					Name:          block.Name,
					ArgumentsJson: string(block.Input),
				})
			}
		}
	}

	return result
}

func stringPtr(s string) *string {
	return &s
}

func jsonToInputSchema(jsonStr string) any {
	// Parse the JSON schema string into a generic map for the Anthropic SDK.
	var schema map[string]any
	if err := json.Unmarshal([]byte(jsonStr), &schema); err != nil {
		// Fallback: return a minimal object schema
		return map[string]any{"type": "object"}
	}
	return schema
}
