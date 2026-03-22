package main

import (
	"encoding/json"
	"fmt"
	"os"
)

// toolManifest mirrors the ToolManifest TypeScript type for validation.
type toolManifest struct {
	ManifestVersion string              `json:"manifestVersion"`
	PackageName     string              `json:"packageName"`
	Tools           []toolManifestEntry `json:"tools"`
}

type toolManifestEntry struct {
	Name             string                 `json:"name"`
	Description      string                 `json:"description"`
	InputJSONSchema  map[string]interface{} `json:"inputJsonSchema"`
	Zones            []string               `json:"zones"`
	DefaultTrustCost float64                `json:"defaultTrustCost"`
	Edge             *edgeConstraints       `json:"edge,omitempty"`
}

type edgeConstraints struct {
	MaxResultBytes int  `json:"maxResultBytes"`
	TimeoutMs      int  `json:"timeoutMs"`
	AllowNetwork   bool `json:"allowNetwork"`
}

var validZones = map[string]bool{
	"sphere":    true,
	"satellite": true,
	"wasm":      true,
}

func runToolValidate(args []string) {
	if len(args) < 1 {
		fmt.Fprintln(os.Stderr, "Usage: intellisphere tool validate <path>")
		fmt.Fprintln(os.Stderr, "  <path>  Path to a JSON manifest file")
		os.Exit(1)
	}

	manifestPath := args[0]

	data, err := os.ReadFile(manifestPath)
	if err != nil {
		fmt.Fprintf(os.Stderr, "Error reading %s: %v\n", manifestPath, err)
		os.Exit(1)
	}

	var manifest toolManifest
	if err := json.Unmarshal(data, &manifest); err != nil {
		fmt.Fprintf(os.Stderr, "Invalid JSON in %s: %v\n", manifestPath, err)
		os.Exit(1)
	}

	errors := validateManifest(&manifest)

	if len(errors) > 0 {
		fmt.Fprintf(os.Stderr, "Validation failed for %s:\n", manifestPath)
		for _, e := range errors {
			fmt.Fprintf(os.Stderr, "  - %s\n", e)
		}
		os.Exit(1)
	}

	fmt.Printf("Manifest %s is valid (%d tool(s)).\n", manifestPath, len(manifest.Tools))
}

func validateManifest(m *toolManifest) []string {
	var errs []string

	if m.ManifestVersion == "" {
		errs = append(errs, "missing manifestVersion")
	} else if m.ManifestVersion != "0.1.0" {
		errs = append(errs, fmt.Sprintf("unsupported manifestVersion: %q (expected \"0.1.0\")", m.ManifestVersion))
	}

	if m.PackageName == "" {
		errs = append(errs, "missing packageName")
	}

	if len(m.Tools) == 0 {
		errs = append(errs, "manifest must contain at least one tool")
	}

	seen := map[string]bool{}
	for i, t := range m.Tools {
		prefix := fmt.Sprintf("tools[%d]", i)

		if t.Name == "" {
			errs = append(errs, fmt.Sprintf("%s: missing name", prefix))
		} else if seen[t.Name] {
			errs = append(errs, fmt.Sprintf("%s: duplicate tool name %q", prefix, t.Name))
		}
		seen[t.Name] = true

		if t.Description == "" {
			errs = append(errs, fmt.Sprintf("%s (%s): missing description", prefix, t.Name))
		}

		if len(t.InputJSONSchema) == 0 {
			errs = append(errs, fmt.Sprintf("%s (%s): missing or empty inputJsonSchema", prefix, t.Name))
		}

		if len(t.Zones) == 0 {
			errs = append(errs, fmt.Sprintf("%s (%s): must have at least one zone", prefix, t.Name))
		}
		for _, z := range t.Zones {
			if !validZones[z] {
				errs = append(errs, fmt.Sprintf("%s (%s): invalid zone %q", prefix, t.Name, z))
			}
		}

		if t.DefaultTrustCost < 0 {
			errs = append(errs, fmt.Sprintf("%s (%s): defaultTrustCost must be >= 0", prefix, t.Name))
		}

		if t.Edge != nil {
			if t.Edge.MaxResultBytes <= 0 {
				errs = append(errs, fmt.Sprintf("%s (%s): edge.maxResultBytes must be > 0", prefix, t.Name))
			}
			if t.Edge.TimeoutMs <= 0 {
				errs = append(errs, fmt.Sprintf("%s (%s): edge.timeoutMs must be > 0", prefix, t.Name))
			}
		}
	}

	return errs
}
