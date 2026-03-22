import type { ZodType, ZodTypeDef } from "zod";
import type {
  EdgeToolDefinition,
  ToolDefinition,
  ToolManifest,
  ToolManifestEntry,
} from "./types.js";

// ---------------------------------------------------------------------------
// Zod → JSON Schema conversion (lightweight, no external deps)
// ---------------------------------------------------------------------------

/**
 * Convert a Zod schema to a JSON Schema (draft-07 compatible) object.
 *
 * This is intentionally minimal — it handles the most common Zod types used
 * in tool input schemas.  For full fidelity you can swap in `zod-to-json-schema`
 * at a later stage.
 */
export function zodToJsonSchema(
  schema: ZodType<unknown, ZodTypeDef, unknown>,
): Record<string, unknown> {
  // Zod stores its internal definition on `_def`.
  const def = (schema as unknown as { _def: Record<string, unknown> })._def;
  const typeName = def["typeName"] as string | undefined;

  switch (typeName) {
    case "ZodString":
      return { type: "string" };

    case "ZodNumber":
      return { type: "number" };

    case "ZodBoolean":
      return { type: "boolean" };

    case "ZodLiteral":
      return { type: typeof def["value"], const: def["value"] };

    case "ZodEnum": {
      const values = (def["values"] as readonly string[]).slice();
      return { type: "string", enum: values };
    }

    case "ZodArray": {
      const innerType = def["type"] as ZodType<unknown, ZodTypeDef, unknown>;
      return { type: "array", items: zodToJsonSchema(innerType) };
    }

    case "ZodOptional": {
      const innerType = def["innerType"] as ZodType<unknown, ZodTypeDef, unknown>;
      return zodToJsonSchema(innerType);
    }

    case "ZodNullable": {
      const innerType = def["innerType"] as ZodType<unknown, ZodTypeDef, unknown>;
      const inner = zodToJsonSchema(innerType);
      return { oneOf: [inner, { type: "null" }] };
    }

    case "ZodDefault": {
      const innerType = def["innerType"] as ZodType<unknown, ZodTypeDef, unknown>;
      const inner = zodToJsonSchema(innerType);
      return { ...inner, default: def["defaultValue"] };
    }

    case "ZodObject": {
      const shape = def["shape"] as
        | (() => Record<string, ZodType<unknown, ZodTypeDef, unknown>>)
        | Record<string, ZodType<unknown, ZodTypeDef, unknown>>;

      const resolvedShape: Record<string, ZodType<unknown, ZodTypeDef, unknown>> =
        typeof shape === "function" ? shape() : shape;

      const properties: Record<string, Record<string, unknown>> = {};
      const required: string[] = [];

      for (const [key, value] of Object.entries(resolvedShape)) {
        properties[key] = zodToJsonSchema(value);

        const innerDef = (value as unknown as { _def: Record<string, unknown> })._def;
        const innerTypeName = innerDef["typeName"] as string | undefined;
        if (innerTypeName !== "ZodOptional" && innerTypeName !== "ZodDefault") {
          required.push(key);
        }
      }

      const result: Record<string, unknown> = {
        type: "object",
        properties,
        additionalProperties: false,
      };
      if (required.length > 0) {
        result["required"] = required;
      }
      return result;
    }

    case "ZodRecord": {
      const valueType = def["valueType"] as ZodType<unknown, ZodTypeDef, unknown>;
      return {
        type: "object",
        additionalProperties: zodToJsonSchema(valueType),
      };
    }

    case "ZodUnion": {
      const options = def["options"] as readonly ZodType<unknown, ZodTypeDef, unknown>[];
      return { oneOf: options.map(zodToJsonSchema) };
    }

    default:
      // Fallback: opaque object so the manifest is still valid JSON Schema.
      return {};
  }
}

// ---------------------------------------------------------------------------
// Manifest entry helpers
// ---------------------------------------------------------------------------

function isEdgeTool(
  tool: ToolDefinition | EdgeToolDefinition,
): tool is EdgeToolDefinition {
  return "edge" in tool;
}

function toManifestEntry(tool: ToolDefinition | EdgeToolDefinition): ToolManifestEntry {
  const base: ToolManifestEntry = {
    name: tool.name,
    description: tool.description,
    inputJsonSchema: zodToJsonSchema(tool.inputSchema),
    zones: tool.zones,
    defaultTrustCost: tool.defaultTrustCost,
  };

  if (isEdgeTool(tool)) {
    return { ...base, edge: tool.edge };
  }
  return base;
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/**
 * Build a {@link ToolManifest} from an array of tool definitions.
 *
 * @param packageName - Name of the package / plugin that owns the tools.
 * @param tools       - Array of tool definitions (may mix core and edge).
 */
export function buildManifest(
  packageName: string,
  tools: ReadonlyArray<ToolDefinition | EdgeToolDefinition>,
): ToolManifest {
  if (!packageName || packageName.trim().length === 0) {
    throw new Error("packageName must be a non-empty string.");
  }

  return Object.freeze<ToolManifest>({
    manifestVersion: "0.1.0",
    packageName,
    tools: tools.map(toManifestEntry),
  });
}

/**
 * Serialise a manifest to a JSON string (pretty-printed).
 */
export function manifestToJson(manifest: ToolManifest): string {
  return JSON.stringify(manifest, null, 2);
}
