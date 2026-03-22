import type { ZodType, ZodTypeDef } from "zod";
import {
  type EdgeToolConstraints,
  type EdgeToolDefinition,
  type ToolHandler,
  ToolZone,
} from "./types.js";

// ---------------------------------------------------------------------------
// Defaults
// ---------------------------------------------------------------------------

const DEFAULT_MAX_RESULT_BYTES = 1_048_576; // 1 MiB
const DEFAULT_TIMEOUT_MS = 30_000; // 30 s
const DEFAULT_ALLOW_NETWORK = false;

// ---------------------------------------------------------------------------
// Options accepted by defineEdgeTool()
// ---------------------------------------------------------------------------

export interface DefineEdgeToolOptions<TInput, TOutput> {
  /** Globally unique tool name. */
  name: string;
  /** Short description surfaced to the LLM. */
  description: string;
  /** Zod schema for input validation. */
  inputSchema: ZodType<TInput, ZodTypeDef, unknown>;
  /** Default trust cost per invocation. Defaults to 1. */
  defaultTrustCost?: number;
  /** The async handler that runs the tool on the edge. */
  handler: ToolHandler<TInput, TOutput>;

  // -- Edge constraints (all optional with sensible defaults) ----------------

  /** Max serialised result size in bytes. Default: 1 MiB. */
  maxResultBytes?: number;
  /** Hard timeout in milliseconds. Default: 30 000 ms. */
  timeoutMs?: number;
  /** Whether the tool may make outbound network calls. Default: false. */
  allowNetwork?: boolean;
}

// ---------------------------------------------------------------------------
// defineEdgeTool()
// ---------------------------------------------------------------------------

/**
 * Create an {@link EdgeToolDefinition} — a tool definition with additional
 * constraints enforced on Satellite / edge nodes.
 *
 * Edge tools always have their zone set to {@link ToolZone.Satellite}.
 *
 * @example
 * ```ts
 * import { z } from "zod";
 * import { defineEdgeTool } from "@intellisphere/sdk";
 *
 * const localLookup = defineEdgeTool({
 *   name: "local.lookup",
 *   description: "Look up a value in the local edge cache.",
 *   inputSchema: z.object({ key: z.string() }),
 *   maxResultBytes: 4096,
 *   timeoutMs: 5_000,
 *   handler: async (input, _ctx) => ({
 *     success: true,
 *     data: { value: "cached" },
 *     trustCost: 0,
 *     durationMs: 1,
 *   }),
 * });
 * ```
 */
export function defineEdgeTool<TInput, TOutput>(
  opts: DefineEdgeToolOptions<TInput, TOutput>,
): EdgeToolDefinition<TInput, TOutput> {
  if (!opts.name || opts.name.trim().length === 0) {
    throw new Error("Edge tool name must be a non-empty string.");
  }

  if (!opts.description || opts.description.trim().length === 0) {
    throw new Error("Edge tool description must be a non-empty string.");
  }

  const defaultTrustCost = opts.defaultTrustCost ?? 1;
  if (defaultTrustCost < 0) {
    throw new Error("defaultTrustCost must be >= 0.");
  }

  const maxResultBytes = opts.maxResultBytes ?? DEFAULT_MAX_RESULT_BYTES;
  if (maxResultBytes <= 0) {
    throw new Error("maxResultBytes must be > 0.");
  }

  const timeoutMs = opts.timeoutMs ?? DEFAULT_TIMEOUT_MS;
  if (timeoutMs <= 0) {
    throw new Error("timeoutMs must be > 0.");
  }

  const edge: EdgeToolConstraints = Object.freeze({
    maxResultBytes,
    timeoutMs,
    allowNetwork: opts.allowNetwork ?? DEFAULT_ALLOW_NETWORK,
  });

  return Object.freeze<EdgeToolDefinition<TInput, TOutput>>({
    name: opts.name,
    description: opts.description,
    inputSchema: opts.inputSchema,
    zones: [ToolZone.Satellite] as const,
    defaultTrustCost,
    handler: opts.handler,
    edge,
  });
}
