package main

import (
	"fmt"
	"os"
	"path/filepath"
	"strings"
)

func runToolCreate(args []string) {
	if len(args) < 1 {
		fmt.Fprintln(os.Stderr, "Usage: intellisphere tool create <name>")
		fmt.Fprintln(os.Stderr, "  <name>  Tool name in dot-notation (e.g. web.search)")
		os.Exit(1)
	}

	toolName := args[0]

	// Derive a filesystem-safe directory name from the tool name.
	dirName := strings.ReplaceAll(toolName, ".", "-")
	toolDir := filepath.Join(".", dirName)

	if err := os.MkdirAll(filepath.Join(toolDir, "src"), 0o755); err != nil {
		fmt.Fprintf(os.Stderr, "Error creating directory: %v\n", err)
		os.Exit(1)
	}

	// package.json
	writeFile(filepath.Join(toolDir, "package.json"), scaffoldPackageJSON(toolName, dirName))

	// tsconfig.json
	writeFile(filepath.Join(toolDir, "tsconfig.json"), scaffoldTSConfig())

	// src/index.ts
	writeFile(filepath.Join(toolDir, "src", "index.ts"), scaffoldIndexTS(toolName))

	fmt.Printf("Scaffolded tool %q in ./%s/\n", toolName, dirName)
	fmt.Println("Next steps:")
	fmt.Println("  1. cd", dirName)
	fmt.Println("  2. npm install")
	fmt.Println("  3. Edit src/index.ts to implement your tool handler")
	fmt.Println("  4. npm run build")
}

func writeFile(path, content string) {
	if err := os.WriteFile(path, []byte(content), 0o644); err != nil {
		fmt.Fprintf(os.Stderr, "Error writing %s: %v\n", path, err)
		os.Exit(1)
	}
}

func scaffoldPackageJSON(toolName, dirName string) string {
	return fmt.Sprintf(`{
  "name": "@intellisphere/tool-%s",
  "version": "0.1.0",
  "type": "module",
  "main": "dist/index.js",
  "types": "dist/index.d.ts",
  "scripts": {
    "build": "tsc",
    "test": "node --test dist/**/*.test.js"
  },
  "dependencies": {
    "@intellisphere/sdk": "workspace:*",
    "zod": "^3.23.0"
  },
  "devDependencies": {
    "typescript": "^5.4.0",
    "@types/node": "^22.0.0"
  }
}
`, dirName)
}

func scaffoldTSConfig() string {
	return `{
  "compilerOptions": {
    "target": "ES2022",
    "module": "NodeNext",
    "moduleResolution": "NodeNext",
    "lib": ["ES2022"],
    "outDir": "dist",
    "rootDir": "src",
    "declaration": true,
    "strict": true,
    "skipLibCheck": true,
    "esModuleInterop": true,
    "isolatedModules": true
  },
  "include": ["src/**/*.ts"],
  "exclude": ["node_modules", "dist"]
}
`
}

func scaffoldIndexTS(toolName string) string {
	return fmt.Sprintf(`import { z } from "zod";
import { defineTool, ToolZone } from "@intellisphere/sdk";

/**
 * %s — TODO: describe your tool.
 */
export const tool = defineTool({
  name: %q,
  description: "TODO: describe what this tool does.",
  inputSchema: z.object({
    // Define your input fields here.
    query: z.string().min(1).describe("The input query"),
  }),
  zones: [ToolZone.Sphere],
  defaultTrustCost: 1,
  handler: async (input, ctx) => {
    const start = Date.now();

    // TODO: implement your tool logic here.
    const result = { echo: input.query };

    return {
      success: true,
      data: result,
      trustCost: 1,
      durationMs: Date.now() - start,
    };
  },
});
`, toolName, toolName)
}
