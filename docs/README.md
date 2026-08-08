# Circus

[design document]: ./DESIGN.md
[Hydra]: https://github.com/nixos/hydra
[evix]: https://github.com/manic-systems/evix
[discussions tab]: https://github.com/manic-systems/circus/discussions

Circus is fast, modular and declarative continuous integration (CI) system built
from the ground up in Rust for Nix-based projects. It is designed for quick,
painless deployments (for mortals & clowns) and robust remote/distributed builds
and evaluations [^eval] without any fuss. While Circus initially followed
[Hydra]'s three-daemon architecture at first and attempted to remain close in
featureset, the design has since diverged. As our needs evolved, so did Circus
into an "all in one" Nix CI option that can be maintained long-term with ease,
deployed anywhere without friction, for any user or any team of any size. Thus,
Circus is the last Nix CI system that addresses the questions of performance,
maintainability and easy declarative setups with seamless scaling.

Feature requests, feedback and constructive criticism are welcome in preparing
Circus for long-term adoption. If you feel overwhelmed by the documentation at
any time, please head to the [discussions tab] to ask your questions directly.

[^eval]: While remote and distributed builds are nothing new in Nix, remote
    evaluation is an ongoing side-branch of development alongside Circus as
    [evix], our This aims to let you break free from the traditional model of
    "eval locally, build remotely", bounded by Nix's clunky evaluator by
    embedding it to Circus agents with resource controls, cache pre-warming and
    more.

> [!NOTE]
> Until 1.0.0 is tagged and released (or, alternatively, this note is removed),
> Circus should be considered **heavily work in progress**. As you'll appreciate
> it is _not very easy_ to build a futureproof CI system, and documentation is
> still lacking in some areas until we have the time to sit down and apply the
> polish it deserves. So to the answer to your burning question of _"should I
> deploy this in production"_ is a very big _maybe_. _Yes_, this is going to be
> good. _No_, it's not quite there yet. We would, however, be delighted to
> receive your feedback and bug reports.
>
> Please create an issue if you notice an obvious inaccuracy or a critical error
> that breaks your setup. While we cannot guarantee a quick response, we would
> appreciate the heads-up. PRs are also very welcome for issues that you've
> noticed, and would like to fix.

The project is named "Circus" because it has "CI" in the name, and well, _CI is
for clowns_. Hope this answers your other burning question.

## Features

Circus is intended to be a full Nix CI system rather than a thin build script
runner or a structured Nix wrapper. Powered by various crates and component, the
main feature areas of Circus are:

### Nix-native evaluation and builds

- Flake-oriented project and jobset evaluation, with support for traditional Nix
  expressions where configured.
- Build creation from evaluated derivations, including aggregate builds and
  build dependencies.
- `requiredSystemFeatures` extraction and capability-aware scheduling, so builds
  that require features such as `kvm`, `nixos-test`, `benchmark`, or
  `big-parallel` are routed to suitable builders through Circus "agents".
- Failed-path caching to skip derivations already known to fail during the cache
  TTL.
- Fixed-output derivation detection and reuse of already-valid store paths.
- Interval, source-change, and manual evaluation triggers.

### Distributed builders

- Push-based `circus-agent` builders connected over Cap'n Proto RPC.
- Persistent agents for long-running builder machines.
- Ephemeral single-session agents for CI providers such as GitHub Actions.
- Agent heartbeats with load, memory, disk, and PSI pressure data.
- Scheduling by system, supported features, mandatory features, current load,
  speed factor, and optional PSI thresholds.
- Legacy SSH remote builders remain supported, with hardened SSH options and
  optional pinned host-key enforcement.
- Local queue-runner builds remain available when the runner host advertises the
  required system and features.

### GitHub Actions ephemeral autoscaling

- Queue-runner-driven `workflow_dispatch` launches for short-lived GitHub
  Actions builders.
- OIDC registration for ephemeral agents using GitHub ID tokens.
- Repository allowlists, audience checks, issuer/JWKS verification, and
  trusted-ref scheduling gates.
- Autoscaling based on pending trusted build demand, live ephemeral capacity,
  in-flight launch TTLs, and scale-up cooldowns.
- Unique ephemeral agent names derived from GitHub run metadata and a fresh
  machine ID to avoid concurrent-run collisions.

### Binary cache and artifact handling

- Built-in Nix binary cache endpoints under `/nix-cache/`.
- Output signing through configured Nix signing keys.
- S3 cache upload support for local, SSH, and agent builds.
- Presigned S3 upload flow for agents, including runner-side verification of the
  uploaded compressed file and uncompressed NAR before narinfo is signed and
  persisted.
- Short-lived signed S3 GET redirects for cache clients when artifacts live in
  object storage.
- GC root management for retained outputs.

### Web UI and API

- REST API for projects, jobsets, evaluations, builds, users, search, admin
  operations, configuration, and cache access.
- Dashboard for browsing projects, jobsets, evaluations, builds, logs, channels,
  machine health, and administration pages.
- OpenAPI document exposed by running instances and generated Markdown API
  reference in [API.md].
- Prometheus-compatible metrics endpoint.
- Page-access policy for public, authenticated, and admin-only dashboard views.

### Configuration and operations

- Declarative configuration for projects, jobsets, users, API keys, channels,
  project members, and remote builders.
- NixOS modules for Circus services and `circus-agent` deployments.
- PostgreSQL-backed state with SQL migrations and a dedicated migration CLI.
- Runtime configuration validation for database settings, URLs, auth tokens,
  webhook targets, SSH builder settings, OIDC settings, and GHA autoscaling
  requirements.
- Admin CLI for common API-backed administrative workflows.
- Structured tracing configuration for service logs.

### Authentication, authorization, and notifications

- API-key authentication and role-based access control.
- Local user/password authentication.
- Optional GitHub OAuth login.
- Optional LDAP bind login.
- Webhook ingestion for common forge events.
- Build status notifications through GitHub, Gitea/Forgejo, GitLab, generic
  webhooks, Slack, and email where configured.
- Persistent notification retry queue with backoff and retention.

## Architecture

Circus, after everything, follows Hydra's three-daemon model (with the addition
of optional distributed agents) with a shared PostgreSQL database. The server
handles the API and dashboard, the evaluator polls repositories and runs Nix
evaluations, and the queue-runner dispatches builds to workers. See the
[design document] for architectural details, data flow, and how the pieces fit
together.

## Documentation

[INSTALL.md]: ./INSTALL.md
[USAGE.md]: ./USAGE.md
[DISTRIBUTED.md]: ./DISTRIBUTED.md
[COMPARISON.md]: ./COMPARISON.md
[API.md]: ./API.md

Circus documentation is split into different documents, detailing different
aspects of its usage. Quickstart options, demo VM usage, configuration, database
setup, NixOS module usage, authentication bootstrapping, monitoring, backup and
source installation instructions for local testing live in the [INSTALL.md]
document. You may also find general, day-to-day usage for end users and
administrators in [USAGE.md]. Additionally, protocol overview and usage
instructions for _distributed_ build setups as an **experimental** feature of
Circus can be found in [DISTRIBUTED.md].

For developers (and more technically inclined users) a design overview is
maintained at [DESIGN.md], which contains architecture, data flow, rationale and
more. If you're interested in a comparison, [COMPARISON.md] holds a somewhat
up-to-date document detailing the weaknesses and strengths of Circus compared to
Hydra. Worth noting that Hydra has picked up steam again, and evolved since the
time of writing for this document.

On a running instance, you may find the machine-readable OpenAPI spec document
at `/api/v1/openapi.json`, unless `server.openapi_enabled = false` is
configured. The generated REST API endpoint reference is also rendered in
markdown at [API.md].

Please create an issue if you notice inaccuracies, or have some questions that
you think is worth answering in the documentation for future reference.

## Hacking

Circus is built with Rust, using 1.95.0 and 2024 edition features. For an
optimal development experience, using Nix is your best bet. The
[Nix flake](../flake.nix) contains a development shell providing the necessary
toolchain. For a consistent experience, Nix is encouraged. Otherwise ensure that
you have at least Rust 1.95.0.

### Building Circus

To build Circus, ensure you have the correct Rust version installed on your
system. On systems with Nix, run `nix develop` or use the provided
[Cade](https://github.com/manic-systems/cade) configuration:

```bash
# Permit Cade to load the flake
$ cade allow
```

Then proceed with the typical Rust workflow:

```bash
# Build all crates
cargo build --workspace

# Run all tests using cargo-nextest; it is provided by the dev shell
cargo nextest run --workspace

# Type-check only
cargo check --workspace
```

Alternatively build and test a specific crate:

Build a specific crate:

```bash
# Build, e.g., circus-server:
$ cargo build -p circus-server
$ cargo nextest run -p circus-server
```

### API Reference Updates

The endpoint reference is generated into [API.md]. Update it after route changes
with:

```bash
# Run api-docs task to generate 'docs/API.md'
$ cargo run -p circus-xtask -- api-docs
```

Or alternatively, check for drift without rewriting the file:

```bash
cargo run -p circus-xtask -- api-docs --check
```

## License

<!--markdownlint-disable MD059-->

This project is made available under Mozilla Public License (MPL) version 2.0.
See [LICENSE](../LICENSE) for more details on the exact conditions. An online
copy is provided [here](https://www.mozilla.org/en-US/MPL/2.0/).

<!--markdownlint-enable MD059-->
