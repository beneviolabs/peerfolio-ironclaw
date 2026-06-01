# Peerfolio Contact Tokenizer

IronClaw WASM tool for Flow 1 of Peerfolio contact discovery.

The tool accepts pasted VCF text, including multi-contact iOS exports, extracts `TEL` fields, normalizes phone numbers to E.164, derives contact tokens with HMAC-SHA256, deduplicates the result, and returns only tokenized identifiers.

It does not call Peerfolio APIs, upload tokens, read files, or request HTTP capabilities.

Input VCF text is limited to 10MB.

## Input

```json
{
  "vcf_text": "BEGIN:VCARD\nVERSION:3.0\nTEL;TYPE=CELL:(415) 555-0100\nEND:VCARD",
  "token_version": 1,
  "default_country_code": "1"
}
```

## Output

```json
{
  "token_version": 1,
  "identifier_type": "phone_number",
  "contact_tokens": ["hex-encoded-hmac-sha256"],
  "extracted_phone_count": 1,
  "normalized_phone_count": 1,
  "deduplicated_count": 0,
  "invalid_phone_count": 0
}
```

The contact graph secret is intentionally not part of the tool schema. Current IronClaw WASM tools can receive credentials through outbound HTTP injection, but they cannot read stored secrets directly for local-only HMAC work. Until we choose a better non-HTTP secret primitive, this repo-local scaffold expects the secret at build time via `PEERFOLIO_CONTACT_GRAPH_SECRET`.

## Build

```bash
PEERFOLIO_CONTACT_GRAPH_SECRET=$PEERFOLIO_CONTACT_GRAPH_SECRET cargo build --target wasm32-wasip2 --release
```

This tool vendors IronClaw's `near:agent@0.3.0` WIT interface in `wit/tool.wit`. If IronClaw reports a function signature or interface mismatch, compare that file against the upstream IronClaw `wit/tool.wit` and rebuild.

## Install

```bash
ironclaw tool install ./target/wasm32-wasip2/release/peerfolio_contact_tokenizer.wasm \
  --capabilities ./contact-tokenizer.capabilities.json \
  --name peerfolio-contact-tokenizer
```

The callable tool name is the installed tool name, `peerfolio-contact-tokenizer`. `tokenize_vcf_contacts` is not a separate exposed function.
