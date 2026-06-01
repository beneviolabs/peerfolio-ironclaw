# Peerfolio IronClaw / OpenClaw Contact Discovery

This mono-repo contains the hackathon MVP for [Peerfolio](https://getpeerfolio.app/)'s privacy-preserving contact discovery flow for agent-native applications.

The project explores a pattern that should replace the old "upload your address book" growth loop. Instead of sending raw contacts to an app, a user can let an IronClaw-secured agent tokenize their contacts locally and use those private tokens to make Peerfolio strategies discoverable to people who already know them.

The goal is to turn a user's existing social graph into a privacy-preserving distribution channel for apps, strategies, tools, and agent skills.

## Hackathon Thesis

As consumer agents become mainstream, users will install tools, skills, and app extensions that may interact with sensitive personal data. Discovery and trust become harder: how does a user know which tools are worth trying, and how can apps grow without asking for raw address books?

Peerfolio Contact Discovery demonstrates one answer:

> Let people who already know you discover your Peerfolio strategies privately via [IronClaw](https://www.ironclaw.com/).

A Peerfolio user opts in, tokenizes phone contacts through an IronClaw-secured boundary, and uploads only derived contact tokens to Peerfolio. A prospective user can then ask their agent to discover strategies shared by people who have them in contacts, without Peerfolio receiving raw phone numbers or full contact lists.

## What This Repo Contains

This mono-repo currently contains two IronClaw WASM tools plus product and technical planning docs.

- `contact-tokenizer/`
  - Flow 1 tool.
  - Accepts pasted VCF text from an iOS contacts export.
  - Extracts phone numbers only.
  - Normalizes phone numbers to E.164.
  - Derives HMAC-SHA256 contact tokens.
  - Deduplicates tokens.
  - Does not call Peerfolio APIs.
  - Does not request HTTP, file, or workspace capabilities.
- `contact-discovery/`
  - Flow 3 tool.
  - Accepts one already-tokenized contact identifier.
  - Calls Peerfolio's public `POST /contact-discovery/discover` endpoint.
  - Returns safe strategy previews only.
  - Never accepts raw phone numbers, VCF text, contact names, emails, or contact lists.
- `peerfolio-openclaw-contact-discovery.md`
  - Product framing, MVP flows, acceptance criteria, out-of-scope items, and success metrics.
- `peerfolio-openclaw.md`
  - Technical RFC covering endpoint shapes, backend data model, rate limits, tokenization, revocation, and V2 hardening recommendations.

## Product Flow

### 1. Tokenize Contacts

A Peerfolio user exports contacts from iOS as a `.vcf`, gives the VCF text to their agent, and runs `peerfolio-contact-tokenizer`.

The tokenizer extracts only phone numbers, normalizes them, and returns contact tokens. Raw phone numbers are not uploaded to Peerfolio.

### 2. Opt In and Upload Tokens

The Peerfolio user explicitly enables:

> Allow contacts to discover my Peerfolio strategies

The user then uploads only tokenized contact identifiers to Peerfolio. Discovery is off by default.

### 3. Discover Shared Strategies

A prospective user asks their agent to find Peerfolio strategies shared by people who have their phone number in contacts.

The discovery tool sends a tokenized phone identifier to Peerfolio and receives safe strategy previews, including:

- invite link
- strategy owner display name
- strategy title
- short strategy summary

The response does not include raw phone numbers, contact lists, balances, holdings, or hidden relationship metadata.

### 4. Revoke Contact Data

A Peerfolio user can revoke contact data. Revocation soft deletes uploaded contact tokens and disables future discovery from that data.

## Privacy Model

Peerfolio should never receive or store raw contact phone numbers.

The MVP uses:

```text
contact_token = HMAC-SHA256(peerfolio_contact_graph_secret, normalized_phone_number)
```

Key boundaries:

- Raw `.vcf` files are never sent to Peerfolio.
- Contact names, emails, notes, addresses, and photos are ignored for V1.
- The tokenizer does not call Peerfolio APIs.
- The discovery tool does not accept raw contact data.
- Discovery returns the same empty result shape for no match, disabled discovery, and no shareable strategies.

## MVP Limitations

This is a hackathon MVP, not a fully hardened production privacy system.

Known limitations:

- No phone-number possession proof before lookup.
- No mutual-contact requirement.
- Public discovery endpoint is acceptable only with strict rate limits and shaped responses.
- Secret rotation is deferred.
- Email, Telegram, and other identifier types are out of scope.
- Copying, customizing, funding, or trading strategies directly from the agent is out of scope.

The first production hardening step should be phone-number possession proof before discovery lookup.

## Why It Matters

Today's common growth pattern asks users to upload entire address books to applications. In an agent-native world, that pattern should become obsolete.

This project shows how apps can plug into a user's social network as a distribution channel while preserving user privacy:

- Users keep raw contacts out of application backends.
- Apps gain agent-native discovery and acquisition.
- Prospective users discover tools and strategies through people they already know.
- Agents can mediate trust without exposing the user's full social graph.

## Build

See each tool README for build and install instructions:

- `contact-tokenizer/README.md`
- `contact-discovery/README.md`
