import type { InboundMessage, OutboundMessage } from "./types.js";

// ---------------------------------------------------------------------------
// Transport abstraction
// ---------------------------------------------------------------------------

/** Minimal interface for a bidirectional message transport (WebSocket). */
export interface Transport {
  /** Send a JSON-serialisable message to the remote peer. */
  send(message: OutboundMessage): void;
  /** Register a handler for incoming messages. */
  onMessage(handler: (message: InboundMessage) => void): void;
  /** Register a handler for connection close. */
  onClose(handler: (code: number, reason: string) => void): void;
  /** Register a handler for transport-level errors. */
  onError(handler: (error: Error) => void): void;
  /** Gracefully close the transport. */
  close(code?: number, reason?: string): void;
  /** Whether the transport is currently open. */
  readonly isOpen: boolean;
}

// ---------------------------------------------------------------------------
// WebSocket transport (uses the standard WebSocket API available in Node 22+)
// ---------------------------------------------------------------------------

export class WebSocketTransport implements Transport {
  private ws: WebSocket;
  private messageHandler: ((msg: InboundMessage) => void) | null = null;
  private closeHandler: ((code: number, reason: string) => void) | null = null;
  private errorHandler: ((error: Error) => void) | null = null;

  constructor(url: string, protocols?: string | string[]) {
    this.ws = new WebSocket(url, protocols);

    this.ws.addEventListener("message", (event: MessageEvent) => {
      if (this.messageHandler) {
        try {
          const parsed = JSON.parse(String(event.data)) as InboundMessage;
          this.messageHandler(parsed);
        } catch (err) {
          this.errorHandler?.(
            err instanceof Error ? err : new Error(String(err)),
          );
        }
      }
    });

    this.ws.addEventListener("close", (event: CloseEvent) => {
      this.closeHandler?.(event.code, event.reason);
    });

    this.ws.addEventListener("error", () => {
      this.errorHandler?.(new Error("WebSocket error"));
    });
  }

  get isOpen(): boolean {
    return this.ws.readyState === WebSocket.OPEN;
  }

  send(message: OutboundMessage): void {
    if (!this.isOpen) {
      throw new Error("Cannot send: WebSocket is not open.");
    }
    this.ws.send(JSON.stringify(message));
  }

  onMessage(handler: (message: InboundMessage) => void): void {
    this.messageHandler = handler;
  }

  onClose(handler: (code: number, reason: string) => void): void {
    this.closeHandler = handler;
  }

  onError(handler: (error: Error) => void): void {
    this.errorHandler = handler;
  }

  close(code?: number, reason?: string): void {
    this.ws.close(code, reason);
  }
}

// ---------------------------------------------------------------------------
// Factory (allows test injection of mock transports)
// ---------------------------------------------------------------------------

export type TransportFactory = (url: string) => Transport;

/** Default factory: creates a real WebSocket transport. */
export const defaultTransportFactory: TransportFactory = (url) =>
  new WebSocketTransport(url);
