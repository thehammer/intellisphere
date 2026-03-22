import type { EdgeToolDefinition, ToolResult } from "@intellisphere/sdk";

// ---------------------------------------------------------------------------
// Wire protocol types (JSON messages over WebSocket)
// ---------------------------------------------------------------------------

/** Session metadata returned by the Sphere on initial HTTP handshake. */
export interface SatelliteSession {
  readonly sessionId: string;
  readonly satelliteId: string;
  readonly wsEndpoint: string;
  readonly trustBudget: number;
  readonly heartbeatIntervalMs: number;
}

/** Incoming request from Sphere asking the Satellite to execute a tool. */
export interface ToolProposalRequest {
  readonly type: "tool_proposal_request";
  readonly requestId: string;
  readonly toolName: string;
  readonly input: unknown;
  readonly invocationId: string;
  readonly callerId: string;
  readonly trustBudget: number;
  readonly metadata: Record<string, unknown>;
}

/** Response sent back from the Satellite after executing a tool. */
export interface ToolProposalResponse {
  readonly type: "tool_proposal_response";
  readonly requestId: string;
  readonly result: ToolResult;
}

/** Trust budget update pushed by the Sphere. */
export interface TrustBudgetUpdate {
  readonly type: "trust_budget_update";
  readonly trustBudget: number;
}

/** Heartbeat ping from Sphere. */
export interface HeartbeatPing {
  readonly type: "heartbeat_ping";
}

/** Heartbeat pong sent by Satellite. */
export interface HeartbeatPong {
  readonly type: "heartbeat_pong";
}

/** Error notification from Sphere. */
export interface SphereError {
  readonly type: "error";
  readonly code: string;
  readonly message: string;
}

/** Union of all messages the Satellite can receive from the Sphere. */
export type InboundMessage =
  | ToolProposalRequest
  | TrustBudgetUpdate
  | HeartbeatPing
  | SphereError;

/** Union of all messages the Satellite can send to the Sphere. */
export type OutboundMessage = ToolProposalResponse | HeartbeatPong;

// ---------------------------------------------------------------------------
// Satellite events
// ---------------------------------------------------------------------------

export interface SatelliteEvents {
  onConnect?: (session: SatelliteSession) => void;
  onDisconnect?: (reason: string) => void;
  onTrustBudgetUpdate?: (budget: number) => void;
  onError?: (error: Error) => void;
}

// ---------------------------------------------------------------------------
// Satellite options
// ---------------------------------------------------------------------------

export interface SatelliteOptions {
  /** Registered edge tool definitions this satellite serves. */
  tools: readonly EdgeToolDefinition[];
  /** Event handlers. */
  events?: SatelliteEvents;
  /** Max reconnection attempts. Default: 5. */
  maxReconnectAttempts?: number;
  /** Base delay (ms) for exponential back-off. Default: 1000. */
  reconnectBaseDelayMs?: number;
}
