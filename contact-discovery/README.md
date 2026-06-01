# Peerfolio Contact Discovery

IronClaw WASM tool for Flow 3 of Peerfolio contact discovery.

The tool accepts one already-tokenized contact identifier and calls Peerfolio's public `POST /contact-discovery/discover` endpoint. It never accepts raw phone numbers, VCF text, VCF files, names, emails, or contact-list data.

Use this after `peerfolio-contact-tokenizer` has produced contact tokens.

## Input

```json
{
  "viewer_contact_token": "hex-encoded-hmac-sha256",
  "token_version": 1,
  "identifier_type": "phone_number"
}
```

For staging or local development, callers may override the API base URL:

```json
{
  "viewer_contact_token": "hex-encoded-hmac-sha256",
  "peerfolio_api_base_url": "https://staging-api.peerfolio.app"
}
```

## Output

```json
{
  "strategies": [
    {
      "invite_link": "https://peerfolio.app/invite/...",
      "owner_display_name": "Ada",
      "strategy_title": "Long-term crypto mix",
      "strategy_summary": "A concise public summary"
    }
  ]
}
```

Peerfolio returns the same empty response shape for no match, disabled owners, and owners with no shareable strategies:

```json
{
  "strategies": []
}
```

## Build

```bash
cargo +1.86.0 build --target wasm32-wasip2 --release
```

Build with Rust 1.86 or newer. IronClaw 0.29.0's HTTP-capable WASM tools expect the newer WASI component ABI emitted by that toolchain line; older Rust 1.82 builds can install successfully but fail at runtime with a `near:agent/host@0.3.0` linker error.

This tool vendors IronClaw's `near:agent@0.3.0` WIT interface in `wit/tool.wit`. If IronClaw reports a function signature or interface mismatch, compare that file against the upstream IronClaw `wit/tool.wit` and rebuild.

## Install

```bash
ironclaw tool install ./target/wasm32-wasip2/release/peerfolio_contact_discovery.wasm \
  --capabilities ./contact-discovery.capabilities.json \
  --name peerfolio-contact-discovery
```

The callable tool name is the installed tool name, `peerfolio-contact-discovery`.
