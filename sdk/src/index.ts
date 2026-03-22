// @intellisphere/sdk — public API surface

export {
  ToolZone,
  type ToolContext,
  type ToolResult,
  type ToolHandler,
  type ToolDefinition,
  type EdgeToolConstraints,
  type EdgeToolDefinition,
  type ToolManifestEntry,
  type ToolManifest,
} from "./types.js";

export { defineTool, type DefineToolOptions } from "./define-tool.js";

export {
  defineEdgeTool,
  type DefineEdgeToolOptions,
} from "./define-edge-tool.js";

export {
  zodToJsonSchema,
  buildManifest,
  manifestToJson,
} from "./manifest.js";
