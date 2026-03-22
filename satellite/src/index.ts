// @intellisphere/satellite — public API surface

export { DysonSatellite } from "./satellite.js";

export { SatelliteGuardrails, type GuardrailsConfig } from "./guardrails.js";

export {
  type Transport,
  type TransportFactory,
  WebSocketTransport,
  defaultTransportFactory,
} from "./transport.js";

export type {
  SatelliteSession,
  ToolProposalRequest,
  ToolProposalResponse,
  TrustBudgetUpdate,
  HeartbeatPing,
  HeartbeatPong,
  SphereError,
  InboundMessage,
  OutboundMessage,
  SatelliteEvents,
  SatelliteOptions,
} from "./types.js";
