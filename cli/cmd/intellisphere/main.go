package main

import (
	"fmt"
	"os"
)

const version = "0.1.0"

func main() {
	if len(os.Args) < 2 {
		printUsage()
		os.Exit(1)
	}

	command := os.Args[1]

	switch command {
	case "tool":
		if len(os.Args) < 3 {
			fmt.Fprintln(os.Stderr, "Usage: intellisphere tool <create|validate> [args...]")
			os.Exit(1)
		}
		subcommand := os.Args[2]
		switch subcommand {
		case "create":
			runToolCreate(os.Args[3:])
		case "validate":
			runToolValidate(os.Args[3:])
		default:
			fmt.Fprintf(os.Stderr, "Unknown tool subcommand: %s\n", subcommand)
			os.Exit(1)
		}

	case "health":
		runHealth(os.Args[2:])

	case "version":
		fmt.Printf("intellisphere %s\n", version)

	case "help", "--help", "-h":
		printUsage()

	default:
		fmt.Fprintf(os.Stderr, "Unknown command: %s\n", command)
		printUsage()
		os.Exit(1)
	}
}

func printUsage() {
	fmt.Println(`IntelliSphere CLI

Usage:
  intellisphere <command> [arguments]

Commands:
  tool create <name>       Scaffold a new tool definition
  tool validate <path>     Validate a tool manifest file
  health --target <url>    Check Sphere health
  version                  Print CLI version
  help                     Show this help`)
}
