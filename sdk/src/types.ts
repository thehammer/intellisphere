import type { ZodType, ZodTypeDef } from "zod";

// ---------------------------------------------------------------------------
// Enums
// ---------------------------------------------------------------------------

/** Where a tool is allowed to execute. */
export enum ToolZone {
  /** Runs inside the Sphere core (server-side). */
  Sphere = "sphere",
  /** Runs on a Satellite / edge node. */
  Satellite = "satellite",
  /** Runs inside a WASM sandbox. */
  Wasm = "wasm",
}

// ---------------------------------------------------------------------------
// Core types
// ---------------------------------------------------------------------------

/** Metadata passed to every tool handler at invocation time. */
export interface ToolContext {
  /** Unique identifier for the current invocation. */
  readonly invocationId: string;
  /** Identity of the caller (user or agent). */
  readonly callerId: string;
  /** Current trust-budget remaining for this session. */
  readonly trustBudget: number;
  /** Zone the tool is executing in. */
  readonly zone: ToolZone;
  /** Arbitrary key-value metadata forwarded from the orchestrator. */
  readonly metadata: Readonly<Record<string, unknown>>;
}

/** Outcome of a single tool execution. */
export interface ToolResult<TOutput = unknown> {
  /** Whether the tool completed successfully. */
  readonly success: boolean;
  /** The output payload (present when success === true). */
  readonly data?: TOutput;
  /** Human-readable error message (present when success === false). */
  readonly error?: string;
  /** Trust cost consumed by this invocation. */
  readonly trustCost: number;
  /** Wall-clock duration in milliseconds. */
  readonly durationMs: number;
}

/**
 * A function that implements the behaviour of a tool.
 *
 * @typeParam TInput  - Validated input type (inferred from the Zod schema).
 * @typeParam TOutput - Shape of the result data on success.
 */
export type ToolHandler<TInput = unknown, TOutput = unknown> = (
  input: TInput,
  ctx: ToolContext,
) => Promise<ToolResult<TOutput>>;

/** Full definition of a tool, combining metadata + runtime handler. */
export interface ToolDefinition<TInput = unknown, TOutput = unknown> {
  /** Globally unique tool name (e.g. "web.search"). */
  readonly name: string;
  /** Short human-readable description for LLM tool-use prompts. */
  readonly description: string;
  /** Zod schema that validates the input payload. */
  readonly inputSchema: ZodType<TInput, ZodTypeDef, unknown>;
  /** Execution zone(s) this tool supports. */
  readonly zones: readonly ToolZone[];
  /** Default trust cost charged per invocation. */
  readonly defaultTrustCost: number;
  /** The handler that runs the tool. */
  readonly handler: ToolHandler<TInput, TOutput>;
}

// ---------------------------------------------------------------------------
// Edge tool constraints
// ---------------------------------------------------------------------------

/** Additional constraints for tools running on Satellite / edge nodes. */
export interface EdgeToolConstraints {
  /** Maximum size (in bytes) of the serialised result payload. */
  readonly maxResultBytes: number;
  /** Hard timeout in milliseconds for the handler. */
  readonly timeoutMs: number;
  /** Whether the tool is allowed to make outbound network requests. */
  readonly allowNetwork: boolean;
}

/** A ToolDefinition augmented with edge-specific constraints. */
export interface EdgeToolDefinition<TInput = unknown, TOutput = unknown>
  extends ToolDefinition<TInput, TOutput> {
  readonly edge: EdgeToolConstraints;
}

// ---------------------------------------------------------------------------
// Manifest (serialisable, no functions)
// ---------------------------------------------------------------------------

/** JSON-serialisable description of a single tool for the Sphere registry. */
export interface ToolManifestEntry {
  readonly name: string;
  readonly description: string;
  /** JSON Schema (draft-07 compatible) derived from the Zod input schema. */
  readonly inputJsonSchema: Record<string, unknown>;
  readonly zones: readonly ToolZone[];
  readonly defaultTrustCost: number;
  /** Present only for edge tools. */
  readonly edge?: EdgeToolConstraints;
}

/** A manifest file listing all tools provided by a package. */
export interface ToolManifest {
  /** Semantic version of the manifest format. */
  readonly manifestVersion: "0.1.0";
  /** Package / plugin name that owns these tools. */
  readonly packageName: string;
  /** Individual tool entries. */
  readonly tools: readonly ToolManifestEntry[];
}
