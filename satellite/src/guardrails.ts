import type { EdgeToolDefinition, ToolResult } from "@intellisphere/sdk";

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

export interface GuardrailsConfig {
  /** Explicit set of tool names this satellite is allowed to execute. */
  allowedTools: ReadonlySet<string>;
  /** Maximum invocations per tool per sliding window. Default: 100. */
  rateLimitPerTool?: number;
  /** Sliding window duration in milliseconds. Default: 60 000 (1 min). */
  rateLimitWindowMs?: number;
  /** Global list of attribute keys to strip from results. */
  redactedAttributes?: readonly string[];
  /** Hard cap on serialised result bytes (overrides per-tool if smaller). */
  globalMaxResultBytes?: number;
}

// ---------------------------------------------------------------------------
// Rate-limit tracker
// ---------------------------------------------------------------------------

interface RateBucket {
  timestamps: number[];
}

// ---------------------------------------------------------------------------
// SatelliteGuardrails
// ---------------------------------------------------------------------------

/**
 * Pre-validation and result sanitisation layer that sits between the
 * transport and the tool handler.
 */
export class SatelliteGuardrails {
  private readonly allowedTools: ReadonlySet<string>;
  private readonly rateLimitPerTool: number;
  private readonly rateLimitWindowMs: number;
  private readonly redactedAttributes: ReadonlySet<string>;
  private readonly globalMaxResultBytes: number | null;

  private readonly buckets = new Map<string, RateBucket>();

  constructor(config: GuardrailsConfig) {
    this.allowedTools = config.allowedTools;
    this.rateLimitPerTool = config.rateLimitPerTool ?? 100;
    this.rateLimitWindowMs = config.rateLimitWindowMs ?? 60_000;
    this.redactedAttributes = new Set(config.redactedAttributes ?? []);
    this.globalMaxResultBytes = config.globalMaxResultBytes ?? null;
  }

  // -------------------------------------------------------------------------
  // Pre-validation
  // -------------------------------------------------------------------------

  /**
   * Validate that the tool is allowed and has not exceeded its rate limit.
   * Returns `null` if OK, or an error string if rejected.
   */
  preValidate(toolName: string): string | null {
    // Allowlist check
    if (!this.allowedTools.has(toolName)) {
      return `Tool "${toolName}" is not in the satellite allowlist.`;
    }

    // Rate-limit check
    const now = Date.now();
    let bucket = this.buckets.get(toolName);
    if (!bucket) {
      bucket = { timestamps: [] };
      this.buckets.set(toolName, bucket);
    }

    // Evict expired timestamps
    const cutoff = now - this.rateLimitWindowMs;
    bucket.timestamps = bucket.timestamps.filter((t) => t > cutoff);

    if (bucket.timestamps.length >= this.rateLimitPerTool) {
      return `Rate limit exceeded for tool "${toolName}": ${this.rateLimitPerTool} invocations per ${this.rateLimitWindowMs}ms window.`;
    }

    bucket.timestamps.push(now);
    return null;
  }

  // -------------------------------------------------------------------------
  // Result sanitisation
  // -------------------------------------------------------------------------

  /**
   * Sanitise a tool result before sending it back to the Sphere.
   *
   * - Strips redacted attribute keys from the data payload.
   * - Enforces the maximum result byte size (per-tool and global).
   *
   * Returns a new ToolResult (never mutates the original).
   */
  sanitiseResult(
    result: ToolResult,
    toolDef: EdgeToolDefinition,
  ): ToolResult {
    let data = result.data;

    // Strip redacted attributes (shallow, top-level keys only)
    if (data !== undefined && typeof data === "object" && data !== null) {
      data = this.stripRedacted(data as Record<string, unknown>);
    }

    // Enforce size limits
    const maxBytes = this.globalMaxResultBytes
      ? Math.min(toolDef.edge.maxResultBytes, this.globalMaxResultBytes)
      : toolDef.edge.maxResultBytes;

    const serialised = JSON.stringify(data);
    const byteLength = new TextEncoder().encode(serialised).byteLength;

    if (byteLength > maxBytes) {
      return {
        success: false,
        error: `Result exceeds maximum size: ${byteLength} bytes > ${maxBytes} byte limit.`,
        trustCost: result.trustCost,
        durationMs: result.durationMs,
      };
    }

    return {
      success: result.success,
      data,
      error: result.error,
      trustCost: result.trustCost,
      durationMs: result.durationMs,
    };
  }

  // -------------------------------------------------------------------------
  // Private helpers
  // -------------------------------------------------------------------------

  private stripRedacted(obj: Record<string, unknown>): Record<string, unknown> {
    if (this.redactedAttributes.size === 0) return obj;

    const cleaned: Record<string, unknown> = {};
    for (const [key, value] of Object.entries(obj)) {
      if (!this.redactedAttributes.has(key)) {
        cleaned[key] = value;
      }
    }
    return cleaned;
  }
}
