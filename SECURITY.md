# Security Policy

## Supported Versions

Until Nexus reaches `1.0.0`, only the latest minor release on `main` receives security fixes.

| Version | Supported          |
| ------- | ------------------ |
| `0.1.x` | :white_check_mark: |
| `< 0.1` | :x:                |

## Reporting a Vulnerability

**Please do not open public GitHub issues for security vulnerabilities.**

Report privately via one of:

1. **GitHub Security Advisories** (preferred): https://github.com/praesidia-ai/nexus/security/advisories/new
2. **Email**: `security@praesidia.ai` (PGP key on request)

Include:

- A description of the issue and its impact
- Steps to reproduce (PoC if possible)
- Affected versions / commits
- Any suggested mitigation

## Disclosure Process

| Step                          | Target    |
| ----------------------------- | --------- |
| Acknowledgement of report     | 72 hours  |
| Initial assessment + severity | 7 days    |
| Fix or mitigation available   | 30 days   |
| Public disclosure / advisory  | Coordinated with reporter |

We follow **coordinated disclosure**: we will not publish details until a fix is available, and we will credit reporters in the advisory unless they prefer to remain anonymous.

## Scope

In scope:

- The Rust workspace under `crates/`
- The Next.js frontend under `web/`
- Official Docker images and install scripts

Out of scope:

- Vulnerabilities in third-party dependencies (please report upstream; we will track via `cargo audit` / `npm audit`)
- Issues that require physical access to the user's machine
- Self-XSS or social-engineering scenarios
- DoS via resource exhaustion against a self-hosted local instance

## Hardening Recommendations for Operators

- Set `NEXUS_DATA_DIR` outside any web-served directory.
- Provide `OPENAI_API_KEY` / `ANTHROPIC_API_KEY` via environment variables, never in source control.
- Run behind a reverse proxy that terminates TLS if exposed beyond `localhost`.
- Keep `cargo` and `npm` dependencies up to date — CI runs `cargo audit` on every PR.
