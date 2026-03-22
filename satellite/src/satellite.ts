import type {
  EdgeToolDefinition,
  ToolContext,
  ToolResult,
  ToolZone,
} from "@intellisphere/sdk";

import { SatelliteGuardrails } from "./guardrails.js";
import { defaultTransportFactory, type Transport, type TransportFactory } from "./transport.js";
import type {
  HeartbeatPong,
  SatelliteEvents,
  SatelliteOptions,
  SatelliteSession,
  ToolProposalRequest,
  ToolProposalResponse,
  InboundMessage,
} from "./types.js";

// ---------------------------------------------------------------------------
// DysonSatellite
// ---------------------------------------------------------------------------

/**
 * Client that connects to a Dyson Sphere instance and serves edge tools.
 *
 * Lifecycle:
 *   1. `connect(sphereUrl, authToken)` — HTTP handshake then WebSocket.
 *   2. Receives `tool_proposal_request` messages, runs the matching handler.
 *   3. Sends back `tool_proposal_response` messages.
 *   4. Handles heartbeats, trust-budget updates, and errors.
 *   5. Reconnects automatically on unexpected disconnects.
 */
export class DysonSatellite {
  // Configuration
  private readonly tools: ReadonlyMap<string, EdgeToolDefinition>;
  private readonly events: SatelliteEvents;
  private readonly maxReconnectAttempts: number;
  private readonly reconnectBaseDelayMs: number;

  // Runtime state
  private session: SatelliteSession | null = null;
  private transport: Transport | null = null;
  private guardrails: SatelliteGuardrails | null = null;
  private reconnectAttempt = 0;
  private closed = false;
  private currentTrustBudget = 0;

  /** Overridable for testing. */
  private transportFactory: TransportFactory;

  constructor(opts: SatelliteOptions, transportFactory?: TransportFactory) {
    const toolMap = new Map<string, EdgeToolDefinition>();
    for (const t of opts.tools) {
      if (toolMap.has(t.name)) {
        throw new Error(`Duplicate edge tool name: "${t.name}".`);
      }
      toolMap.set(t.name, t);
    }
    this.tools = toolMap;
    this.events = opts.events ?? {};
    this.maxReconnectAttempts = opts.maxReconnectAttempts ?? 5;
    this.reconnectBaseDelayMs = opts.reconnectBaseDelayMs ?? 1_000;
    this.transportFactory = transportFactory ?? defaultTransportFactory;
  }

  // -----------------------------------------------------------------------
  // Public API
  // -----------------------------------------------------------------------

  /**
   * Establish a session with the Sphere and open the WebSocket channel.
   */
  async connect(sphereUrl: string, authToken: string): Promise<void> {
    this.closed = false;
    this.reconnectAttempt = 0;

    // 1. HTTP handshake — fetch session info
    this.session = await this.fetchSession(sphereUrl, authToken);
    this.currentTrustBudget = this.session.trustBudget;

    // 2. Build guardrails from registered tools
    this.guardrails = new SatelliteGuardrails({
      allowedTools: new Set(this.tools.keys()),
    });

    // 3. Open WebSocket
    this.openTransport(this.session.wsEndpoint);
  }

  /**
   * Gracefully disconnect from the Sphere.
   */
  disconnect(): void {
    this.closed = true;
    this.transport?.close(1000, "client disconnect");
    this.transport = null;
  }

  /** Current trust budget as last reported by the Sphere. */
  get trustBudget(): number {
    return this.currentTrustBudget;
  }

  // -----------------------------------------------------------------------
  // Session handshake
  // -----------------------------------------------------------------------

  private async fetchSession(
    sphereUrl: string,
    authToken: string,
  ): Promise<SatelliteSession> {
    const toolNames = Array.from(this.tools.keys());

    const res = await fetch(new URL("/api/v1/satellite/session", sphereUrl), {
      method: "POST",
      headers: {
        "Content-Type": "application/json",
        Authorization: `Bearer ${authToken}`,
      },
      body: JSON.stringify({ tools: toolNames }),
    });

    if (!res.ok) {
      const body = await res.text().catch(() => "");
      throw new Error(
        `Sphere session handshake failed: ${res.status} ${res.statusText} — ${body}`,
      );
    }

    return (await res.json()) as SatelliteSession;
  }

  // -----------------------------------------------------------------------
  // Transport management
  // -----------------------------------------------------------------------

  private openTransport(wsEndpoint: string): void {
    this.transport = this.transportFactory(wsEndpoint);

    this.transport.onMessage((msg) => {
      void this.handleMessage(msg);
    });

    this.transport.onClose((_code, reason) => {
      this.events.onDisconnect?.(reason);
      if (!this.closed) {
        void this.attemptReconnect();
      }
    });

    this.transport.onError((err) => {
      this.events.onError?.(err);
    });

    // Notify once transport is open (best-effort: event fires on next tick)
    if (this.session) {
      this.events.onConnect?.(this.session);
    }
  }

  // -----------------------------------------------------------------------
  // Reconnection with exponential back-off
  // -----------------------------------------------------------------------

  private async attemptReconnect(): Promise<void> {
    while (
      !this.closed &&
      this.reconnectAttempt < this.maxReconnectAttempts
    ) {
      this.reconnectAttempt++;
      const delayMs =
        this.reconnectBaseDelayMs * Math.pow(2, this.reconnectAttempt - 1);

      await this.sleep(delayMs);

      if (this.closed) return;

      try {
        if (this.session) {
          this.openTransport(this.session.wsEndpoint);
          this.reconnectAttempt = 0;
          return;
        }
      } catch {
        // Will retry on next iteration.
      }
    }

    if (!this.closed) {
      this.events.onError?.(
        new Error(
          `Failed to reconnect after ${this.maxReconnectAttempts} attempts.`,
        ),
      );
    }
  }

  // -----------------------------------------------------------------------
  // Message handling
  // -----------------------------------------------------------------------

  private async handleMessage(msg: InboundMessage): Promise<void> {
    switch (msg.type) {
      case "tool_proposal_request":
        await this.handleToolProposal(msg);
        break;

      case "trust_budget_update":
        this.currentTrustBudget = msg.trustBudget;
        this.events.onTrustBudgetUpdate?.(msg.trustBudget);
        break;

      case "heartbeat_ping":
        this.sendPong();
        break;

      case "error":
        this.events.onError?.(new Error(`Sphere error [${msg.code}]: ${msg.message}`));
        break;
    }
  }

  // -----------------------------------------------------------------------
  // Tool execution
  // -----------------------------------------------------------------------

  private async handleToolProposal(req: ToolProposalRequest): Promise<void> {
    const toolDef = this.tools.get(req.toolName);

    // Unknown tool
    if (!toolDef) {
      this.sendResponse(req.requestId, {
        success: false,
        error: `Unknown tool: "${req.toolName}"`,
        trustCost: 0,
        durationMs: 0,
      });
      return;
    }

    // Pre-validation (allowlist + rate limit)
    const rejection = this.guardrails?.preValidate(req.toolName) ?? null;
    if (rejection) {
      this.sendResponse(req.requestId, {
        success: false,
        error: rejection,
        trustCost: 0,
        durationMs: 0,
      });
      return;
    }

    // Validate input against Zod schema
    const parseResult = toolDef.inputSchema.safeParse(req.input);
    if (!parseResult.success) {
      this.sendResponse(req.requestId, {
        success: false,
        error: `Input validation failed: ${parseResult.error.message}`,
        trustCost: 0,
        durationMs: 0,
      });
      return;
    }

    // Build context
    const ctx: ToolContext = {
      invocationId: req.invocationId,
      callerId: req.callerId,
      trustBudget: req.trustBudget,
      zone: "satellite" as ToolZone,
      metadata: req.metadata,
    };

    // Execute with timeout
    const startMs = Date.now();
    let result: ToolResult;

    try {
      result = await this.executeWithTimeout(
        toolDef.handler(parseResult.data, ctx),
        toolDef.edge.timeoutMs,
      );
    } catch (err) {
      const elapsed = Date.now() - startMs;
      result = {
        success: false,
        error: err instanceof Error ? err.message : String(err),
        trustCost: 0,
        durationMs: elapsed,
      };
    }

    // Sanitise result
    if (this.guardrails) {
      result = this.guardrails.sanitiseResult(result, toolDef);
    }

    this.sendResponse(req.requestId, result);
  }

  private async executeWithTimeout<T>(
    promise: Promise<T>,
    timeoutMs: number,
  ): Promise<T> {
    return new Promise<T>((resolve, reject) => {
      const timer = setTimeout(
        () => reject(new Error(`Tool execution timed out after ${timeoutMs}ms.`)),
        timeoutMs,
      );

      promise
        .then((val) => {
          clearTimeout(timer);
          resolve(val);
        })
        .catch((err) => {
          clearTimeout(timer);
          reject(err instanceof Error ? err : new Error(String(err)));
        });
    });
  }

  // -----------------------------------------------------------------------
  // Outbound helpers
  // -----------------------------------------------------------------------

  private sendResponse(requestId: string, result: ToolResult): void {
    const response: ToolProposalResponse = {
      type: "tool_proposal_response",
      requestId,
      result,
    };
    try {
      this.transport?.send(response);
    } catch (err) {
      this.events.onError?.(
        err instanceof Error ? err : new Error(String(err)),
      );
    }
  }

  private sendPong(): void {
    const pong: HeartbeatPong = { type: "heartbeat_pong" };
    try {
      this.transport?.send(pong);
    } catch {
      // Non-critical; will reconnect if transport is broken.
    }
  }

  // -----------------------------------------------------------------------
  // Utilities
  // -----------------------------------------------------------------------

  private sleep(ms: number): Promise<void> {
    return new Promise((resolve) => setTimeout(resolve, ms));
  }
}
