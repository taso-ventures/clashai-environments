# Security Policy

## Reporting a Vulnerability

If you believe you've found a security vulnerability in `clashai-environments`, **please do not file a public GitHub issue**.

Report it privately through GitHub's built-in vulnerability reporting:

1. Go to the [Security tab](https://github.com/taso-ventures/clashai-environments/security) of this repository.
2. Click **Report a vulnerability**.
3. Fill in the form. The report is visible only to repository maintainers.

We'll acknowledge receipt within a few business days and coordinate a fix and disclosure timeline with you.

## Scope

In scope:

- Memory safety bugs in the Rust crates (panics, UB) reachable through the documented HTTP/WebSocket surface.
- Logic bugs in environment engines that allow a player to take an action the rules forbid.
- Resource-exhaustion vectors against `environment-server` (unbounded allocations, slow loris, etc.) reachable through the documented endpoints.
- Issues in the included viewer JS that lead to XSS or data leaks across matches.

Out of scope:

- Vulnerabilities that require running an attacker-controlled environment binary or otherwise pre-trusting the attacker.
- Issues in third-party dependencies that don't have an exploitable path through this repo's code (please report those upstream).
- Anything in `target/`, generated artifacts, or local-only tooling.

## Supported Versions

This project is in active development. Security fixes are applied to `main`; we don't currently maintain prior versions.
