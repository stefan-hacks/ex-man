# ex-man

An intelligent manpage example generator. Smarter than tldr/tealdeer - parses local manpages, extracts flags/options, and generates contextual usage examples with AI-like reasoning.

[![Rust](https://img.shields.io/badge/Rust-1.70%2B-orange)](https://www.rust-lang.org)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](https://opensource.org/licenses/MIT)

## Features

- **🧠 Intelligent Parsing**: Reads manpages directly from your system (handles .gz, .bz2, .xz compression)
- **⚡ Lightning Fast**: Local SQLite cache - subsequent lookups are instant
- **🎯 Contextual Examples**: Not just static examples - generates relevant examples based on each flag's purpose
- **🔄 Combination Examples**: Automatically creates multi-flag examples for common workflows
- **📊 Rich Display**: Beautiful terminal output with colored syntax highlighting
- **🐧 Cross-Platform**: Works on all Linux distributions and NixOS environments
- **📦 Zero Config**: Works out of the box with your existing manpages

## Installation

### From source (requires Rust 1.70+)

```bash
git clone https://github.com/stefan-hacks/ex-man.git
cd ex-man
cargo build --release
sudo cp target/release/ex-man /usr/local/bin/
```

### NixOS (flake)

```nix
# In your flake.nix inputs
ex-man = {
  url = "github:stefan-hacks/ex-man";
  inputs.nixpkgs.follows = "nixpkgs";
};

# In your packages
inputs.ex-man.packages.${system}.default
```

## Usage

```bash
# Show examples for a command (first run parses manpage, subsequent runs use cache)
ex-man ls

# Force re-parse (ignore cache)
ex-man ls --refresh

# Show raw extracted options without examples
ex-man ls --raw

# Output as JSON
ex-man ls --json

# Search for tools in database
ex-man -s grep

# List all known tools
ex-man -l

# Show examples with specific flags only
ex-man curl --flags verbose,output
```

## Example Output

```
═══════════════════════════════════════════════
  ex-man → ls
  📊 60 options parsed, 63 examples generated
═══════════════════════════════════════════════

OPTIONS
  FLAG            DESCRIPTION                    ARG
  ──────────────────────────────────────────────────────────────
  -a, --all       Show all files including...  —
  -l              Use long listing format       —
  -h, --human-readable Human readable sizes      —
  ...

EXAMPLES

  1 -a, --all: Show all files including hidden
     $ ls --all

  2 -l: Use long listing format
     $ ls -l

  3 Common usage: combines the most frequently used flags
     $ ls --all -l --human-readable

  4 Show all items with verbose output (--verbose + --all)
     $ ls --verbose --all

═══════════════════════════════════════════════
```

## How It Works

1. **Manpage Discovery**: Uses `man` command and MANPATH to find manpages (handles compressed formats)
2. **Smart Parsing**: Renders manpages to plain text and extracts options using heuristics that understand common manpage formatting
3. **Semantic Analysis**: Categorizes options (output, io, behavior, formatting, etc.) to generate meaningful examples
4. **Intelligent Examples**: Creates realistic arguments based on option descriptions (e.g., `--output` gets `output.txt`, `--host` gets `example.com`)
5. **Local Cache**: Stores everything in a SQLite database at `~/.local/share/ex-man/ex-man.db`

## Architecture

```
ex-man/
├── src/
│   ├── main.rs          # CLI entry point with clap
│   ├── manpage.rs       # Manpage parsing (rendered text + groff source)
│   ├── example.rs       # Example generation engine with semantic analysis
│   ├── db.rs            # SQLite cache layer
│   └── display.rs       # Terminal output formatting
├── tests/               # Integration tests
└── Cargo.toml
```

## Comparison

| Tool | Intelligence | Local | Offline | Examples | Speed |
|------|-------------|-------|---------|----------|-------|
| tldr | Static | No | No | Yes | Fast |
| tealdeer | Static | Yes | Yes | Yes | Fast |
| **ex-man** | **Dynamic** | **Yes** | **Yes** | **Generated** | **Fast** |

## License

MIT
