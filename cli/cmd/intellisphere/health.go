package main

import (
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"os"
	"time"
)

func runHealth(args []string) {
	target := "http://localhost:8080"
	for i, arg := range args {
		if (arg == "--target" || arg == "-t") && i+1 < len(args) {
			target = args[i+1]
		}
	}

	url := target + "/health"
	client := &http.Client{Timeout: 5 * time.Second}

	resp, err := client.Get(url)
	if err != nil {
		fmt.Fprintf(os.Stderr, "Error connecting to %s: %v\n", url, err)
		os.Exit(1)
	}
	defer resp.Body.Close()

	body, err := io.ReadAll(resp.Body)
	if err != nil {
		fmt.Fprintf(os.Stderr, "Error reading response: %v\n", err)
		os.Exit(1)
	}

	var health map[string]any
	if err := json.Unmarshal(body, &health); err != nil {
		fmt.Fprintf(os.Stderr, "Invalid health response: %s\n", body)
		os.Exit(1)
	}

	formatted, _ := json.MarshalIndent(health, "", "  ")
	fmt.Printf("IntelliSphere Health (%s):\n%s\n", target, formatted)

	if status, ok := health["status"].(string); ok && status != "healthy" {
		os.Exit(1)
	}
}
