# Circus Usage Guide

This guide covers day-to-day use of a running Circus instance. It is **not** an
installation, setup, deployment or cf configuration reference (those have been
placed in [INSTALL.md](./INSTALL.md) but instead a document to detail everyday
usage of Circus. API endpoint details live in [API.md](./API.md), and
distributed builder protocol details live in [DISTRIBUTED.md](./DISTRIBUTED.md).

Circus has three main user interfaces:

- The web dashboard at `/` for browsing projects, evaluations, builds, logs,
  queues, channels, news, and administration pages.
- The REST API under `/api/v1` for automation.
- `circus-admin` for common administrative API calls from a terminal.

## End Users

End users are people who inspect CI status, read logs, download products, follow
channels, or trigger authorized actions. Operators can make many dashboard pages
public, login-only, or admin-only, so the exact access level depends on the
server's `server.page_access` settings.

### Logging In

Open `/login` on the Circus server.

The login page supports:

- API key login: paste a valid API key to receive a dashboard session cookie.
- Username/password: use a local Circus user account.

Additional configured identity-provider flows are available through API routes:

- GitHub OAuth: start at `GET /api/v1/auth/github`.
- LDAP bind login: `POST /auth/ldap` with `username` and `password` JSON. This
  returns a dashboard session cookie on success.

Use `/logout` to clear the dashboard session.

### Dashboard Pages

Common dashboard pages:

<!--markdownlint-disable MD013-->

| Page                          | Purpose                                                                    |
| ----------------------------- | -------------------------------------------------------------------------- |
| `/`                           | Overview of recent builds, recent evaluations, project summaries, and news |
| `/projects`                   | Project list                                                               |
| `/projects/new`               | Admin project setup wizard with repository probing                         |
| `/project/{id}`               | Project details and jobsets                                                |
| `/project/{id}/notifications` | Admin notification settings for one project                                |
| `/jobset/{id}`                | Jobset evaluation history                                                  |
| `/jobset/{id}/jobs`           | Job history matrix for a jobset                                            |
| `/evaluations`                | Evaluation list                                                            |
| `/evaluation/{id}`            | Builds produced by one evaluation                                          |
| `/builds`                     | Build list with status, system, and job filters                            |
| `/build/{id}`                 | Build detail, steps, products, dependencies, and logs                      |
| `/queue`                      | Pending and running builds                                                 |
| `/channels`                   | Published channels                                                         |
| `/channel/{id}`               | Builds promoted to a channel                                               |
| `/news`                       | Announcements                                                              |
| `/starred`                    | Jobs starred by the current user                                           |
| `/metrics`                    | Human-readable metrics page                                                |
| `/users`                      | Admin user-management page                                                 |
| `/admin`                      | Admin overview, API keys, config, pinned outputs, builders, notifications  |

<!--markdownlint-enable MD013-->

Admin-only dashboard pages are covered in the admin section.

### Reading Build Status

Builds move through statuses such as `pending`, `running`, `succeeded`,
`failed`, `cancelled`, `aborted`, `unsupported_system`, and `cached_failure`.

The build detail page shows:

- The derivation path and target system.
- Build steps and command metadata.
- Live or completed logs.
- Build products and downloadable artifacts.
- Dependency and dependent builds.
- Error messages and extracted log error lines when available.

The build list supports filtering by status, system, and job name. The queue
page shows pending/running builds and elapsed runtime for active builds.

### Logs and Live Output

Build logs are available from the build detail page and API:

- `GET /api/v1/builds/{id}/log` returns the current or final log as text.
- `GET /api/v1/builds/{id}/log/stream` streams live log output with server-sent
  events while a build is running.

The dashboard links to the appropriate log view from each build page.

### Build Products

If a build records products, the build page lists them. Binary products can be
downloaded through:

```text
GET /api/v1/builds/{build_id}/products/{product_id}/download
```

Operators may also expose build outputs through the Nix binary cache and channel
manifests described below.

### Badges and Latest Outputs

Projects can publish Hydra-compatible job badges and latest-output redirects:

```text
GET /job/{project}/{jobset}/{job}/badge
GET /job/{project}/{jobset}/{job}/shield
GET /job/{project}/{jobset}/{job}/latest
```

Use badges in README files or status dashboards. Use `latest` when you need a
stable URL to the latest successful output for a job.

### Channels

Channels are named release streams promoted from evaluations. A channel can be
used by downstream Nix consumers to discover a selected revision and its store
paths.

User-facing channel endpoints include:

```text
GET /channel/{name}/git-revision
GET /channel/{name}/binary-cache-url
GET /channel/{name}/store-paths.xz
GET /channel/{name}/nixexprs.tar.xz
```

The dashboard shows channels under `/channels` and `/channel/{id}`.

### Binary Cache

When enabled by the operator, Circus serves a Nix binary cache under
`/nix-cache/`:

```text
GET /nix-cache/nix-cache-info
GET /nix-cache/{hash}
GET /nix-cache/nar/{hash}
```

> [!NOTE]
> Users normally consume this through Nix substituter configuration, not
> manually. Ask your administrator for the public cache URL and trusted public
> key if the cache is signed. Circus may serve NARs directly from the server's
> local Nix store or redirect downloads to a short-lived signed S3 URL for
> outputs uploaded by distributed agents. Nix follows these redirects
> automatically.

### Starred Jobs

Authenticated users can star jobs and view them on `/starred`. This is useful
for tracking high-value or frequently checked jobs across projects.

The API endpoints are:

```text
GET /api/v1/me/starred-jobs
POST /api/v1/me/starred-jobs
DELETE /api/v1/me/starred-jobs/{id}
```

### Profile and Password

Authenticated users can inspect and update their own profile through
`GET /api/v1/me` and `PUT /api/v1/me`. Local password users can change their
password with `POST /api/v1/me/password`. GitHub OAuth and LDAP users are
managed by their identity provider; their Circus role and enabled/disabled state
are still controlled by admins.

### Searching

Use the dashboard search controls where available, or the API:

```text
GET /api/v1/search
GET /api/v1/search/quick
```

`quick` is intended for autocomplete-style lookups. `search` supports richer
queries and filters.

### Read-Only API Use

Most read endpoints are safe to call from scripts, but default installations
still require authentication for `/api/v1` reads. Operators can opt out with
`server.require_api_key_for_reads = false`; otherwise send an API key as a
bearer token:

```bash
# Search the builds endpoint
$ curl -H "Authorization: Bearer $CIRCUS_API_KEY" \
  https://ci.example.org/api/v1/builds?status=failed
```

For the complete API list, see [API.md](./API.md).

Machine-readable OpenAPI is available from the running server at:

```text
GET /api/v1/openapi.json
```

## Administrators

While Circus provides a large API surface for scripted operations,
administrators that juggle between projects, jobsets, users, API keys, builders,
notifications, channels, evaluations, queued builds, and operational health
(potentially much more in the future) may prefer `circus-admin` for routine
tasks because it constructs API requests safely, and pretty-prints (with cool
tables) by default.

Set these environment variables once per shell:

```bash
# Set variables required by the circus-admin tool
$ export CIRCUS_URL=https://ci.example.org
$ export CIRCUS_API_KEY=<admin-api-key>
```

> [!TIP]
> Add `--json` to most `circus-admin` commands when machine-readable output is
> needed.

### Health and System Status

The admin CLI can be used to query various status fields:

```bash
# Public health check
$ circus-admin health

# Admin system status
$ circus-admin status
```

Operational HTTP endpoints:

```text
GET /health
GET /prometheus
GET /api/v1/admin/system
GET /api/v1/admin/audit-log
```

The dashboard admin page at `/admin` summarizes system status, pinned build
outputs, remote builders, API keys, and notification tasks.

### API Keys

API keys authenticate API and `circus-admin` requests. Keys are stored hashed;
the cleartext key is only shown when created.

```bash
# List app api-keys via administration CLI
$ circus-admin api-keys list

# Create a new key with a specific role.
$ circus-admin api-keys create --name deploy-bot --role admin

# Or a read-only key
$ circus-admin api-keys create --name readonly-dashboard --role read-only

# Revoke an API key by-id
$ circus-admin api-keys revoke <api-key-id>
```

There are a few roles that you may choose to assign based on what privileges you
wish to grant to a particular user.

| Role              | Intended use                       |
| ----------------- | ---------------------------------- |
| `admin`           | Full access                        |
| `read-only`       | Read-only API and dashboard access |
| `create-projects` | Project/jobset creation workflows  |
| `eval-jobset`     | Trigger evaluations                |
| `cancel-build`    | Cancel builds                      |
| `restart-jobs`    | Restart completed or failed builds |
| `bump-to-front`   | Bump pending build priority        |

Role behavior is still evolving; keep privileged keys narrow and rotate them if
they are exposed. It's a good idea to keep user access limited until role
behaviour is fully polished.

### Users

Local users can log in to the dashboard with username/password. GitHub OAuth and
LDAP users can also receive sessions when those integrations are configured.

```bash
# List all users
$ circus-admin users list

# Create a new user
$ circus-admin users create \
  --username alice \
  --email alice@example.org \
  --password 'change-me' \
  --role read-only

# Assign a role to a user
$ circus-admin users set-role <user-id> --role admin

# Disable a user's login, preventing them from logging in
$ circus-admin users disable <user-id>

# Re-enable a user's login
$ circus-admin users enable <user-id>

# Fully delete a user from the database
$ circus-admin users delete <user-id>
```

Users can update their own profile through `/api/v1/me` and change their
password with `/api/v1/me/password`.

### Projects and Jobsets

A project maps to a source repository. A jobset defines what to evaluate inside
that repository and how evaluations are triggered.

Basic project commands:

```bash
# List all projects
$ circus-admin projects list

# Create a new project from the CLI
$ circus-admin projects create \
  --name nixpkgs \
  --repository-url https://github.com/NixOS/nixpkgs \
  --description "Nixpkgs CI"

# Delete a project by ID
$ circus-admin projects delete <project-id>
```

Dashboard workflows:

- `/projects/new` provides a project setup wizard.
- `/project/{id}` shows jobsets and admin forms.
- `/project/{id}/notifications` manages project notification settings.

The setup wizard uses `POST /api/v1/projects/probe` to inspect a repository and
then `POST /api/v1/projects/setup` to create the project plus initial flake-mode
jobsets. Use those endpoints when automating the same flow; use the lower-level
project and jobset endpoints when you need explicit control over every field.

Useful project/jobset API endpoints:

```text
POST /api/v1/projects/probe
POST /api/v1/projects/setup
GET /api/v1/projects/{id}/jobsets
POST /api/v1/projects/{id}/jobsets
PUT /api/v1/projects/{project_id}/jobsets/{id}
DELETE /api/v1/projects/{project_id}/jobsets/{id}
GET /api/v1/projects/{project_id}/jobsets/{jobset_id}/inputs
POST /api/v1/projects/{project_id}/jobsets/{jobset_id}/inputs
DELETE /api/v1/projects/{project_id}/jobsets/{jobset_id}/inputs/{input_id}
```

Jobset trigger modes:

- Source-change jobsets evaluate when repository inputs change or a webhook
  creates an evaluation.
- Interval jobsets intentionally create a fresh run every due tick, even for the
  same commit/input hash.

### Evaluations

An evaluation runs Nix evaluation for a jobset and creates builds. Users browse
evaluations in `/evaluations`, `/evaluation/{id}`, `/jobset/{id}`, and
`/jobset/{id}/jobs`.

Administrative commands:

```bash
# List all evaluations
$ circus-admin evaluations list

# List all evaluations for a specific jobset, with a specific status
$ circus-admin evaluations list --jobset-id <jobset-id> --status completed

# Trigger an evaluation for a jobset with a commit hash
$ circus-admin evaluations trigger \
  --jobset-id <jobset-id> \
  --commit-hash <git-commit>
```

Pull-request metadata can be included when triggering manually:

```bash
# Alternatively, track a pull request
$ circus-admin evaluations trigger \
  --jobset-id <jobset-id> \
  --commit-hash <git-commit> \
  --pr-number 42 \
  --pr-head-branch feature \
  --pr-base-branch main \
  --pr-action synchronize
```

Admins can hide noisy or obsolete evaluations from non-admin dashboard/API views
without deleting their builds. The dashboard exposes this as **Hide** /
**Unhide** buttons on evaluation lists and evaluation detail pages. Hidden
evaluations are still visible to admins.

To compare two evaluations by job set membership and derivation path, call:

```text
GET /api/v1/evaluations/{from_eval_id}/compare?to={to_eval_id}
```

The comparison response separates new jobs, removed jobs, changed derivations,
and unchanged count. This is useful for release review and for checking what a
source update actually changed before promotion.

### Builds and Queue Control

Builds are created by evaluations and executed by the queue-runner. Admins and
authorized users can inspect, cancel, restart, reprioritize, and pin builds.

```bash
# List all builds
$ circus-admin builds list

# List all failed builds for a specific system
$ circus-admin builds list --status failed --system x86_64-linux

# Cancel a build by ID
$ circus-admin builds cancel <build-id>

# Restart a build by ID
$ circus-admin builds restart <build-id>

# Bump a build's priority, also by ID
$ circus-admin builds bump <build-id>

# Whether to pin a build to survive future garbage collections.
$ circus-admin builds keep <build-id> --value true
$ circus-admin builds keep <build-id> --value false
```

`bump` only applies to builds still in `pending` state. Each call adds `10` to
the build's numeric `priority`. The queue-runner selects pending work ordered by
highest priority first, then by jobset scheduling-share fairness, then oldest
creation time. This is a preference, not a preemption mechanism: it does not
stop or reorder builds that are already running.

Use `keep=true` for build outputs that must survive garbage collection. A kept
build pins every recorded product for that build: GC cleanup preserves the
primary build root, any recorded product GC root path, and any root symlink
whose target is a kept product path.

Admins can inspect retained outputs from the dashboard's **Administration** page
under **Pinned Build Outputs**. The table shows the build, output name, store
path, and recorded GC root. Use **Unpin Build** there, or run
`circus-admin
builds keep <build-id> --value false`, to make all outputs for
that build eligible for normal GC cleanup again after their roots age out.

The same data is available through the admin API:

```bash
# List retained products and their GC roots
$ curl -H "Authorization: Bearer $CIRCUS_API_KEY" \
    "$CIRCUS_URL/api/v1/admin/pinned-build-products"

# Unpin a build and all of its retained products
$ curl -X POST -H "Authorization: Bearer $CIRCUS_API_KEY" \
    "$CIRCUS_URL/api/v1/admin/pinned-builds/<build-id>/unpin"
```

### Remote Builders and Agents

Circus supports two distributed build paths:

- SSH remote builders, configured through the admin API or `circus-admin`.
- Persistent `circus-agent` processes that connect to the queue-runner over
  Cap'n Proto RPC.

SSH remote builder commands:

```bash
# List all builders
$ circus-admin builders list

# Add a new builder
$ circus-admin builders add \
  --name builder-1 \
  --ssh-uri builder-1.example.org \
  --systems x86_64-linux,aarch64-linux \
  --max-jobs 4 \
  --speed-factor 1

# Temporarily disable a builder
$ circus-admin builders disable <builder-id>

# Re-enable a builder
$ circus-admin builders enable <builder-id>

# Remove a builder
$ circus-admin builders remove <builder-id>
```

Optional builder metadata:

- `--supported-features kvm,nixos-test` advertises build capabilities.
- `--mandatory-features big-parallel` limits the builder to builds requiring
  those features.
- `--public-host-key` pins SSH host identity.
- `--ssh-key-file` selects a non-default SSH key.

Persistent agent sessions are visible through the admin API:

```text
GET /api/v1/admin/builders/sessions
GET /api/v1/admin/builders/sessions/connected
GET /api/v1/admin/builders/sessions/{machine_id}
```

The same data is available from `circus-admin`:

```bash
# List all recorded persistent and ephemeral agent sessions
$ circus-admin builders sessions

# Only list currently connected sessions
$ circus-admin builders sessions --connected

# Show one session by stable machine ID
$ circus-admin builders session <machine-id>
```

The queue-runner prefers connected agents over SSH builders when both match a
build. See [DISTRIBUTED.md](./DISTRIBUTED.md) for protocol and security details.

### Notifications

Circus can send build and evaluation notifications through configured providers
and keeps a retry queue for failed deliveries.

Notification task commands:

```bash
# List all build notifications
$ circus-admin notifications list

# Retry a notification by task ID
$ circus-admin notifications retry <task-id>
```

Project notification configuration is available from:

```text
/project/{id}/notifications
```

Relevant API endpoints include:

```text
GET /api/v1/admin/notification-tasks
POST /api/v1/admin/notification-tasks/{id}/retry
```

### Webhooks

Webhooks let forges trigger evaluations without waiting for polling. Supported
receivers are GitHub, GitLab, Gitea, and Forgejo:

```text
POST /api/v1/webhooks/{project_id}/github
POST /api/v1/webhooks/{project_id}/gitlab
POST /api/v1/webhooks/{project_id}/gitea
POST /api/v1/webhooks/{project_id}/forgejo
```

Webhook configs are managed per project:

```text
GET /api/v1/projects/{id}/webhooks
POST /api/v1/projects/{id}/webhooks
DELETE /api/v1/projects/{id}/webhooks/{webhook_id}
```

Create a webhook config with a `forge_type` of `github`, `gitlab`, `gitea`, or
`forgejo`, plus an optional `secret`. GitHub, Gitea, and Forgejo verify HMAC
signatures from that secret; GitLab compares the configured secret against its
token header. Webhooks trigger only source-change jobsets whose branch matches
the event branch or merge-request target.

### Channels and Releases

Channels publish selected evaluations as named streams. Admins can create,
delete, and promote evaluations to channels through the API or dashboard.

Useful endpoints:

```text
GET /api/v1/channels
POST /api/v1/channels
GET /api/v1/channels/{id}
DELETE /api/v1/channels/{id}
GET /api/v1/channels/{id}/nixexprs.tar.xz
POST /api/v1/channels/{channel_id}/promote/{eval_id}
GET /api/v1/projects/{project_id}/channels
```

Users consume channel manifests from `/channel/{name}/...` endpoints described
in the end-user section.

### News and Announcements

Admins can publish short news entries shown on the dashboard home page and news
page.

```text
GET /api/v1/news
POST /api/v1/news
DELETE /api/v1/news/{id}
```

The dashboard page is `/news`.

### Config Management

`circus-admin` can read or replace the server's declarative config body through
the admin API:

```bash
# View the *active* configuration
$ circus-admin config get

# Replace the server's configured TOML file body
$ circus-admin config apply ./circus.toml
```

This updates the config file body exposed by the server. This feature is
experimental, and the runtime behavior still depends on which settings the
daemons reload dynamically and which require a service restart. For the full
configuration schema, see [INSTALL.md](./INSTALL.md) and the repository's
`circus.toml`. Until further notice, **use this with caution**.

### Audit Log

Use the audit log to inspect recent administrative actions:

```bash
# Check audit log with an entry limit
$ circus-admin audit --limit 100
```

API endpoint:

```text
GET /api/v1/admin/audit-log
```

### Monitoring

For machine monitoring, scrape Prometheus metrics:

```text
GET /prometheus
```

The dashboard's `/metrics` page uses JSON time-series endpoints that can also be
called directly for custom dashboards:

```text
GET /api/v1/metrics/timeseries/builds
GET /api/v1/metrics/timeseries/duration
GET /api/v1/metrics/systems
```

These endpoints accept optional query parameters such as `project_id`,
`jobset_id`, `hours`, and `bucket`, depending on the metric.

For quick human checks, use:

```text
GET /health
GET /api/v1/admin/system
/metrics
```

Recommended alerts:

- `/health` is not healthy.
- Queue depth grows for longer than expected.
- Builds fail at an unusual rate.
- Remote builders or agents disappear.
- Notification retry tasks accumulate.
- Disk space for logs, GC roots, or builder work directories is low.

### Backup and Recovery

Circus state is primarily PostgreSQL plus build logs on disk. Back up:

- The PostgreSQL database.
- The configured `logs.log_dir`.
- Any declarative config file managed outside the database.
- Binary cache signing keys and other secrets.

Build outputs live in the Nix store and are retained through GC roots when
configured. They do not need a separate database backup, but losing all
builders' stores may require rebuilding outputs.

### Operational Safety Notes

- Treat API keys, webhook secrets, OAuth secrets, LDAP bind credentials, S3
  credentials, and signing keys as secrets. Do not share them, ensure they do
  not leak.
- Prefer HTTPS in front of the dashboard and set `server.force_secure_cookies`
  when Circus is behind an HTTPS reverse proxy.
- Keep page access private where project metadata or logs are sensitive.
- Use narrow roles for automation keys.
- Keep remote builder SSH host keys pinned when using SSH builders.

If using distributed agents:

- Use TLS/mTLS and rotate RPC bearer tokens as part of normal secret rotation.
