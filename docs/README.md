# Circus

[design document]: ./DESIGN.md
[Hydra]: https://github.com/nixos/hydra
[discussions tab]: https://github.com/manic-systems/circus/discussions/landing

Circus is a fast, modular continuous integration system built from the ground up
in Rust for Nix-based projects for quick, easy deployments for mortals and
long-term reliability in mind with a special emphasis on declarative
configuration, and robust distributed builds without any fuss. It used to follow
[Hydra]'s three-daemon architecture, but the design has diverged since. Still,
Circus addresses the original pain points with Hydra in the areas of
performance, maintainability, and easy, declarative setups.

Our goal with Circus is to have an "all in one" Nix CI option that can be
maintained long-term with ease, and without compromising from performance while
allowing deployed anywhere, anytime _without friction_ for any user or team of
any size. Feature requests, feedback and constructive criticism are welcome in
preparing Circus for long-term adoption. If you feel overwhelmed by the
documentation at any time, please head to the [discussions tab] to ask your
questions directly.

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

## Architecture

Circus, after everything, Hydra's three-daemon model (with the addition of
optional distributed agents) with a shared PostgreSQL database. The server
handles the API and dashboard, the evaluator polls repositories and runs Nix
evaluations, and the queue-runner dispatches builds to workers. See the
[design document] for architectural details, data flow, and how the pieces fit
together.

- **server** (`circus-server`): REST API, dashboard, binary cache, metrics,
  webhooks
- **evaluator** (`circus-evaluator`): Git polling and Nix evaluation via
  `nix-eval-jobs`
- **queue-runner** (`circus-queue-runner`): Build dispatch with semaphore-based
  worker pool
- **agent** (`circus-agent`): Persistent build host that receives work over RPC
- **admin CLI** (`circus-admin`): API-backed command line interface for common
  administration tasks
- **common** (`circus-common`): Shared types, database layer, configuration,
  validation
- **proto** (`circus-proto`): Cap'n Proto schema and generated RPC bindings
- **migrate-cli** (`circus-migrate`): Database migration CLI
- **migrations** (`circus-migrations`): SQL migration files and runtime
- **xtask** (`xtask`): Developer tooling (API docs generation and route drift
  checks)

## Documentation

[INSTALL.md]: ./INSTALL.md
[USAGE.md]: ./USAGE.md
[DISTRIBUTED.md]: ./DISTRIBUTED.md

[COMPARISON.md]

Circus documentation is split into different documents, detailing different
aspects of its usage. Quickstart options, demo VM usage, configuration, database
setup, NixOS module usage, authentication bootstrapping, monitoring, backup and
source installation instructions for local testing live in the [INSTALL.md]
document. You may also find general, day-to-day usage for end users and
administrators in [USAGE.md]. Additionally, protocol overview and usage
instructions for _distributed_ build setups as an **experimental** feature of
Circus can be found in [DISTRIBUTED.md].

For developers (and more technically inclined users) a design overview is
maintained at [DESIGN.md], which contains architechture, data flow, rationale
and more. If you're interested in a comparison, [COMPARISON.md] holds a somewhat
up-to-date document detailing the weaknesses and strengths of Circus compared to
Hydra. Worth noting that Hydra has picked up steam again, and evolved since the
time of writing for this document.

- [API.md](./API.md): generated REST API endpoint reference. Running servers
  also expose the same machine-readable OpenAPI document at
  `/api/v1/openapi.json`.

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
cargo nextest run

# Type-check only
cargo check
```

Alternatively build and test a specific crate:

Build a specific crate:

```bash
# Build, e.g., circus-server:
$ cargo build -p circus-server
$ cargo nextest run -p circus-server
```

### API Reference Updates

The endpoint reference is generated into [docs/API.md](./API.md). Update it
after route changes with:

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
