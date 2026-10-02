<!-- markdownlint-disable no-duplicate-heading -->

# Circus changelog

<!--
This changelog describes user-facing changes in Circus. Add new changes under
"Unreleased" until the next version is released. Use "Fixed" for changes that
correct existing behaviour, "Changed" for updates to existing behaviour, and
"Removed" for removed features or interfaces.
-->

## Unreleased

### Added

- Dashboard OIDC login with named providers, group access rules, and global role
  mappings.

## 0.14.0

### Added

- Jobsets can set `only_build_latest` to cancel older automated evaluations and
  builds from the same branch, change-request, or tag stream. Manual evaluations
  and restarts are preserved.
- Source-change jobsets can set `path_filters` with Git pathspecs. Circus now
  evaluates a source update only when a matching path changed.
- Declarative projects can opt into runtime changes through
  `declarative.allow_runtime_mutation`, with a per-project override. The default
  remains locked.
- Binary cache uploads can be cleaned up automatically with size, age, and
  cleanup-interval limits under `[cache.gc]`.
- Circus exports Prometheus metrics and includes a Grafana dashboard for build,
  evaluation, cache, system, and queue activity.

### Changed

- Evaluations now retain the newest source work when several updates arrive in
  quick succession. Stale webhook revisions are rejected, and older automated
  work is superseded according to the jobset policy.
- Nix evaluations preserve the configured timeout for zero-duration values and
  apply the timeout to Evix workers and auxiliary Nix processes.
- Empty Nix evaluations now report diagnostics instead of failing without an
  explanation.

### Fixed

- Source updates excluded by path filters no longer create empty evaluations.
- Evaluations now filter unsupported systems before creating build records,
  avoiding queued builds that no connected builder can run.
- Declarative project reconciliation applies the configured mutation policy at
  runtime instead of allowing changes to drift from the declaration.

### Removed

- Remote SSH builder configuration, dispatch, probing, REST endpoints, and
  dashboard management have been removed. Use Circus agents and queue-runner
  pools for distributed builds.
- Build-step recording, the build-steps database table, the
  `GET /builds/{id}/steps` endpoint, and the dashboard build-steps view have
  been removed.
