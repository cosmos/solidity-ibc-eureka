# PDA Generator

Generates PDA helper functions from Anchor IDL files.

## Usage

```bash
# From project root (recommended)
just generate-pda

# Or manually with explicit paths
go run e2e/interchaintestv8/tools/generate-pdas/main.go \
  --idl-dir programs/solana/target/idl \
  --output e2e/interchaintestv8/solana/pda.go
```

Both `--idl-dir` and `--output` flags are required.

## When to Regenerate

- After modifying Anchor programs
- After `just build-solana`
- When adding new programs
- When IDL structure changes

The tool scans all `.json` files in the IDL directory and generates one helper function per unique PDA pattern.
