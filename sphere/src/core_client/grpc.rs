use tonic::transport::Channel;

use crate::proto::intellisphere::v1::{
    llm_core_client::LlmCoreClient, CompletionChunk, CompletionRequest, CompletionResponse,
    HealthRequest, HealthResponse,
};

/// Client for communicating with the Core service over gRPC.
#[derive(Clone)]
pub struct CoreClient {
    inner: LlmCoreClient<Channel>,
}

impl CoreClient {
    /// Connect to the Core gRPC service.
    pub async fn connect(url: &str) -> Result<Self, tonic::transport::Error> {
        let inner = LlmCoreClient::connect(url.to_string()).await?;
        Ok(Self { inner })
    }

    /// Send a completion request and receive a single response.
    pub async fn complete(
        &self,
        request: CompletionRequest,
    ) -> Result<CompletionResponse, tonic::Status> {
        let mut client = self.inner.clone();
        let response = client.complete(request).await?;
        Ok(response.into_inner())
    }

    /// Send a completion request and receive a stream of chunks.
    pub async fn complete_stream(
        &self,
        request: CompletionRequest,
    ) -> Result<tonic::Streaming<CompletionChunk>, tonic::Status> {
        let mut client = self.inner.clone();
        let response = client.complete_stream(request).await?;
        Ok(response.into_inner())
    }

    /// Check the health of the Core service.
    pub async fn health(&self) -> Result<HealthResponse, tonic::Status> {
        let mut client = self.inner.clone();
        let response = client.health(HealthRequest {}).await?;
        Ok(response.into_inner())
    }
}
