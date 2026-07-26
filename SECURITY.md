# Security Policy

## Supported versions

Security fixes are accepted against the latest published `0.1.x` release line of the `uf-valence*` crates.

## Reporting a vulnerability

Please **do not** open a public GitHub issue for security-sensitive reports.

Prefer one of the following:

1. **GitHub Security Advisories** — use [Report a vulnerability](https://github.com/unified-field-dev/valence/security/advisories/new) on this repository when available.
2. Contact the maintainers privately via the repository owner listed at https://github.com/unified-field-dev/valence.

Include:

- a description of the issue and its impact
- steps to reproduce or a proof of concept when possible
- affected crate names and versions (`uf-valence`, backends, etc.)

We will acknowledge receipt as soon as practical and coordinate a fix and disclosure timeline with you.

## Scope

In scope: vulnerabilities in published `uf-valence*` crates, documentation that could cause unsafe production defaults, and CI/supply-chain issues in this repository.

Out of scope: vulnerabilities solely in third-party databases or clients (Postgres, SurrealDB, Redis, etc.) unless Valence mishandles them in a security-relevant way; demo credentials in local `infra/` compose files.

## Operator hardening (L0)

Valence is an **in-process ORM**. Hosts own session auth, HTTP exposure, and whether clients can reach raw backends or elevated actors.

| Area | Guidance |
|------|----------|
| Privacy bypass | Dual-key only: `VALENCE_PRIVACY_BYPASS=1` **and** `VALENCE_PRIVACY_BYPASS_FORCE_ON=1`. Never set either in production. Bench/testkit set both when measuring bypass. |
| Default-deny policies | Schemas without entity `policies:` deny non-System actors. Declare explicit allow rules for readable tables. |
| Actor / System | Do not expose raw `Valence::with_actor(Actor::System)` or untrusted `actor_json` from clients. Install [`RejectExternalSystemActor`](https://docs.rs/uf-valence-core) on external [`RouterValenceFactory`](https://docs.rs/uf-valence-core) paths. |
| Backends | Do not expose raw `DatabaseBackend` / router keys to clients; wire through typed `Model` / host-authorized APIs. |
| Query paging | `QueryCore` clamps `limit`/`offset` to `MAX_QUERY_LIMIT` (1000) / `MAX_QUERY_OFFSET`. |
| Errors | `Error::database` redacts URL userinfo in database error strings. |

See also `uf-valence` crate docs (§ Features).
