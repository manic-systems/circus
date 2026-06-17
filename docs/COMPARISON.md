<!--markdownlint-disable MD013-->

# Circus vs Hydra

Circus is not exactly a competitor, but a direct replacement to Hydra with a
limited set of features that are prioritized. While Circus hopes to be a
complete replacement, not everything can be considered in-scope for a team of
FOSS developers without funding. _Should_ you create any issues, your feature
may or may not be implemented depending on our own needs. Please keep that in
mind.

Below document contains a "feature matrix" of concepts and features that we've
decided to think about. Not all of them will be fully implemented (i.e., we are
a-okay with being less powerful in some regards) but Circus _does_ aim to be
better than Hydra in the long term, through different means; reliability, UX and
performance.

## Executive Summary

Circus currently implements _more or less_ 50% of Hydra's core features, and has
various improvements over Hydra's architecture. As of writing, some gaps (such
as the plugin architecture, VCS diversity, and notification integrations)
remain. The persistent builder protocol is no longer one of them: Circus ships a
Cap'n Proto + capnp-rpc agent path (`crates/proto`, `crates/agent`, and
`crates/queue-runner/src/rpc/`). The transport choice intentionally diverges
from Hydra's gRPC + protobuf; see `docs/DISTRIBUTED.md` for the rationale and
the protocol overview.

As Circus is currently in _heavy_ development, those gaps will remain for the
foreseeable future, however, most _critical_ functionality has already been
implemented. In any case, I believe Circus has made good progress on the path of
being a "better Hydra". The following "strengths" and "weaknesses" are not to be
taken at face value, as they are _generally_ intentional divergences. Some can
be considered weaknesses depending on your needs and goals, but at the same time
we're open to new features and additions based on demand.

### Strengths

- Modern Rust codebase with better error handling
- Simpler, more maintainable architecture (small focused Rust crates, fewer
  daemons)
- Better API-first design with proper REST structure
- User management with argon2 password hashing and granular RBAC
- Cleaner database schema (16 tables vs ~25 core tables in Hydra)
- Better Nix Flake support from day one
- Improved & tested declarative jobsets

### Weaknesses

- Limited VCS support (Git only in Circus vs 6 types in Hydra)
- No plugin architecture for extensibility
- Missing several notification integrations (BitBucket, InfluxDB, Coverity,
  SoTest, etc.)
- No OpenTelemetry tracing

## Feature-by-Feature

### Circus Server

`circus-server` crate is the REST API server that powers Circus. In comparison
to support for full CRUD operations (on par with Hydra), Circus exceeds Hydra in
several areas, such as log streaming, evaluation comparison, build actions or
metrics visualization from the API. Below is a comparison table for the sake of
historical documentation and progress tracking:

| Feature                  | Hydra                      | Circus                  | Status   | Notes                                                  |
| ------------------------ | -------------------------- | ----------------------- | -------- | ------------------------------------------------------ |
| **REST API Structure**   | OpenAPI 3.0 spec           | REST                    | Complete | Circus has cleaner `/api/v1` structure                 |
| **Project Endpoints**    | Full CRUD                  | Full CRUD               | Complete |                                                        |
| **Jobset Endpoints**     | Full CRUD                  | Full CRUD               | Complete | Circus has jobset inputs                               |
| **Build Endpoints**      | Full                       | Full + actions          | Complete | Circus has cancel/restart/bump                         |
| **Build Constituents**   | `/build/{id}/constituents` | Implemented             | Complete | Circus has constituents endpoint                       |
| **Evaluation Endpoints** | Basic                      | Full + trigger          | Complete | Circus has trigger + compare                           |
| **Search API**           | Full search                | Advanced search         | Complete | Multi-entity, filters, sorting                         |
| **Channel API**          | Management                 | Full CRUD               | Complete |                                                        |
| **User API**             | User management            | Full CRUD + auth        | Complete |                                                        |
| **Binary Cache API**     | NAR/manifest               | Full cache protocol     | Complete |                                                        |
| **Webhook API**          | Push trigger               | GitHub/Gitea/GitLab     | Complete | Circus has HMAC verification                           |
| **Badge API**            | Status badges              | Implemented             | Complete | Both have shields.io badges                            |
| **Metrics API**          | Prometheus                 | Prometheus              | Complete | Both expose metrics                                    |
| **Log Streaming**        | Polling only               | SSE streaming           | Complete | Circus has Server-Sent Events                          |
| **Queue Runner API**     | Internal REST on :8080     | Agent-session admin API | Partial  | Circus exposes builder sessions through the server API |
