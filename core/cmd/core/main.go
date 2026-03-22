package main

import (
	"fmt"
	"net"
	"os"
	"os/signal"
	"syscall"

	"github.com/caarlos0/env/v11"
	"go.uber.org/zap"
	"google.golang.org/grpc"
	"google.golang.org/grpc/reflection"

	"github.com/thehammer/intellisphere/core/internal/adapter"
	"github.com/thehammer/intellisphere/core/internal/server"
)

type Config struct {
	Port         int    `env:"CORE_PORT" envDefault:"50051"`
	LLMProvider  string `env:"LLM_PROVIDER" envDefault:"anthropic"`
	AnthropicKey string `env:"ANTHROPIC_API_KEY"`
	DefaultModel string `env:"DEFAULT_MODEL" envDefault:"claude-sonnet-4-20250514"`
}

func main() {
	logger, _ := zap.NewProduction()
	defer logger.Sync()

	var cfg Config
	if err := env.Parse(&cfg); err != nil {
		logger.Fatal("Failed to parse config", zap.Error(err))
	}

	// Build LLM adapter
	llmAdapter, err := adapter.NewAdapter(cfg.LLMProvider, adapter.AdapterConfig{
		APIKey:       cfg.AnthropicKey,
		DefaultModel: cfg.DefaultModel,
	})
	if err != nil {
		logger.Fatal("Failed to create LLM adapter", zap.Error(err))
	}

	// Start gRPC server
	lis, err := net.Listen("tcp", fmt.Sprintf(":%d", cfg.Port))
	if err != nil {
		logger.Fatal("Failed to listen", zap.Int("port", cfg.Port), zap.Error(err))
	}

	grpcServer := grpc.NewServer()
	coreServer := server.NewLLMCoreServer(llmAdapter, logger)
	server.RegisterLLMCoreServer(grpcServer, coreServer)
	reflection.Register(grpcServer)

	logger.Info("Core gRPC server starting", zap.Int("port", cfg.Port), zap.String("provider", cfg.LLMProvider))

	// Graceful shutdown
	go func() {
		sigCh := make(chan os.Signal, 1)
		signal.Notify(sigCh, syscall.SIGINT, syscall.SIGTERM)
		<-sigCh
		logger.Info("Shutting down Core")
		grpcServer.GracefulStop()
	}()

	if err := grpcServer.Serve(lis); err != nil {
		logger.Fatal("gRPC server failed", zap.Error(err))
	}
}
