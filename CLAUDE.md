# Sakuin CLI

Rust CLI client for the [Sakuin](https://sakuin.org) manga database API.

## Tech Stack

- **tonic** - gRPC client
- **prost** - Protobuf codegen
- **clap** - CLI argument parsing
- **tokio** - Async runtime

## Proto Schema

Proto files are exported from the Buf Schema Registry:

```bash
buf export buf.build/sakuin/api -o proto
```

The build.rs compiles these into Rust types via tonic-build.

To update protos after schema changes:
```bash
buf export buf.build/sakuin/api -o proto
cargo build
```

## Building

```bash
cargo build --release
```

## Configuration

Config stored in `~/.config/sakuin-cli/config.json`:

```json
{
  "server": "https://sakuin.org",
  "token": "sk_your_api_key"
}
```

Get an API key from https://sakuin.org/profile after signing in.

## Commands

```bash
# Public (no auth required)
sakuin stats                      # Database statistics
sakuin search "one piece"         # Full-text search
sakuin get 6647                   # Get manga by ID
sakuin list                       # List manga

# Authenticated (requires API key)
sakuin list-mine                  # List your tracked manga
sakuin track 6647 reading         # Set reading status
sakuin progress 6647 "Ch. 45"     # Set progress
sakuin rate 6647 8                # Rate manga (1-10)
```
