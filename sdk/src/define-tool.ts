import type { ZodType, ZodTypeDef } from "zod";
import { type ToolDefinition, type ToolHandler, ToolZone } from "./types.js";

// ---------------------------------------------------------------------------
// Options accepted by defineTool()
// ---------------------------------------------------------------------------

export interface DefineToolOptions<TInput, TOutput> {
  /** Globally unique tool name (e.g. "web.search"). */
  name: string;
  /** Short human-readable description surfaced to the LLM. */
  description: string;
  /** Zod schema that validates & parses the input payload. */
  inputSchema: ZodType<TInput, ZodTypeDef, unknown>;
  /** Execution zones this tool supports. Defaults to [ToolZone.Sphere]. */
  zones?: readonly ToolZone[];
  /** Default trust cost per invocation. Defaults to 1. */
  defaultTrustCost?: number;
  /** The async function that implements the tool. */
  handler: ToolHandler<TInput, TOutput>;
}

// ---------------------------------------------------------------------------
// defineTool()
// ---------------------------------------------------------------------------

/**
 * Create a validated {@link ToolDefinition}.
 *
 * The returned definition carries the Zod schema so the framework can
 * validate inputs at runtime *before* calling the handler.
 *
 * @example
 * ```ts
 * import { z } from "zod";
 * import { defineTool, ToolZone } from "@intellisphere/sdk";
 *
 * const searchTool = defineTool({
 *   name: "web.search",
 *   description: "Search the web for a query string.",
 *   inputSchema: z.object({ query: z.string().min(1) }),
 *   zones: [ToolZone.Sphere, ToolZone.Satellite],
 *   handler: async (input, ctx) => ({
 *     success: true,
 *     data: { results: [] },
 *     trustCost: 1,
 *     durationMs: 42,
 *   }),
 * });
 * ```
 */
export function defineTool<TInput, TOutput>(
  opts: DefineToolOptions<TInput, TOutput>,
): ToolDefinition<TInput, TOutput> {
  if (!opts.name || opts.name.trim().length === 0) {
    throw new Error("Tool name must be a non-empty string.");
  }

  if (!opts.description || opts.description.trim().length === 0) {
    throw new Error("Tool description must be a non-empty string.");
  }

  const zones: readonly ToolZone[] = opts.zones ?? [ToolZone.Sphere];

  if (zones.length === 0) {
    throw new Error("At least one execution zone must be specified.");
  }

  const defaultTrustCost = opts.defaultTrustCost ?? 1;

  if (defaultTrustCost < 0) {
    throw new Error("defaultTrustCost must be >= 0.");
  }

  return Object.freeze<ToolDefinition<TInput, TOutput>>({
    name: opts.name,
    description: opts.description,
    inputSchema: opts.inputSchema,
    zones,
    defaultTrustCost,
    handler: opts.handler,
  });
}
