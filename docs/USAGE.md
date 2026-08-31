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
- `circusctl` for API calls from a terminal.

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
| `/build/{id}`                 | Build detail, products, dependencies, and logs                             |
| `/queue`                      | Pending and running builds                                                 |
| `/channels`                   | Published channels                                                         |
| `/channel/{id}`               | Builds promoted to a channel                                               |
| `/news`                       | Announcements                                                              |
| `/starred`                    | Jobs starred by the current user                                           |
| `/metrics`                    | Human-readable metrics page                                                |
| `/caches`                     | Admin binary cache list, per-cache storage/traffic, and NAR search         |
| `/users`                      | Admin user-management page                                                 |
| `/admin`                      | Admin overview, API keys, config, pinned outputs, builders, notifications  |

<!--markdownlint-enable MD013-->

Admin-only dashboard pages are covered in the admin section.

### Customizing the Dashboard

Operators can customize the bundled dashboard without replacing the frontend.
The server loads bundled CSS first, generated CSS variables second, and optional
custom CSS last.

```toml
[ui]
brand_name = "Acme CI"
brand_subtitle = "Nix build farm"
logo_url = "/static/custom/logo.svg"
favicon_url = "/static/custom/favicon.svg"
custom_css = "/etc/circus/dashboard.css"
static_dir = "/etc/circus/static"

[ui.css_variables]
accent = "#2563eb"
bg = "#0f172a"
text = "#f8fafc"
```

With the NixOS module, set the same TOML-shaped keys under
`services.circus.settings`:

```nix
{
  services.circus.settings.ui = {
    brand_name = "Acme CI";
    brand_subtitle = "Nix build farm";
    logo_url = "/static/custom/logo.svg";
    favicon_url = "/static/custom/favicon.svg";
    custom_css = ./dashboard.css;
    static_dir = ./static;
    css_variables = {
      accent = "#2563eb";
      bg = "#0f172a";
      text = "#f8fafc";
    };
  };
}
```

Files in `static_dir` are served below `/static/custom/`. For a completely
custom frontend, disable the bundled dashboard with `ui.dashboard = false` and
build against the `/api/v1` endpoints directly.

### Reading Build Status

Builds move through statuses such as `pending`, `running`, `succeeded`,
`failed`, `cancelled`, `aborted`, `unsupported_system`, and `cached_failure`.

The build detail page shows:

- The derivation path and target system.
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
`/nix-cache/` and per-project caches under
`/projects/<project-name>/nix-cache/`:

```text
GET /nix-cache/nix-cache-info
GET /nix-cache/{hash}
GET /nix-cache/nar/{hash}

GET /projects/<project-name>/nix-cache/nix-cache-info
GET /projects/<project-name>/nix-cache/{hash}
GET /projects/<project-name>/nix-cache/nar/{hash}
```

> [!NOTE]
> Users normally consume this through Nix substituter configuration, not
> manually. Ask your administrator for the public cache URL and trusted public
> key if the cache is signed. Project caches only expose outputs owned by that
> project. Configured upstreams are used by Circus builds to avoid rebuilding
> dependencies already available from another substituter. Circus may serve NARs
> directly from the server's local Nix store or redirect downloads to a
> short-lived signed S3 URL for outputs uploaded by distributed agents. Nix
> follows these redirects automatically.

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

Set `server.openapi_enabled = false` to stop serving that endpoint.

## Administrators

While Circus provides a large API surface for scripted operations,
administrators that juggle between projects, jobsets, users, API keys, builders,
notifications, channels, evaluations, queued builds, and operational health
(potentially much more in the future) may prefer `circusctl` for routine tasks
because it constructs API requests safely, and pretty-prints (with cool tables)
by default.

Set these environment variables once per shell:

```bash
# Set variables required by the circusctl tool
$ export CIRCUS_URL=https://ci.example.org
$ export CIRCUS_API_KEY=<admin-api-key>
```

> [!TIP]
> Add `--json` to most `circusctl` commands when machine-readable output is
> needed.

### Health and System Status

The admin CLI can be used to query various status fields:

```bash
# Public health check
$ circusctl health

# Admin system status
$ circusctl admin status
```

Operational HTTP endpoints:

```text
GET /health
GET /prometheus
GET /api/v1/admin/system
GET /api/v1/admin/audit-log
```

The dashboard admin page at `/admin` summarizes system status, pinned build
outputs, API keys, and notification tasks.

### Binary Cache Observability

The admin Caches page at `/caches` lists the global cache plus every
cache-enabled project, each with NAR count, compressed size, and trailing-hour
request volume. A top stat strip totals NARs and storage across all caches.

Selecting a cache opens its detail page (`/caches/{name}`), which shows:

- **Storage**: packages stored, uncompressed and compressed totals, plus a chart
  of bytes and packages added over time, with a Minutes/Hours/Days/Weeks toggle.
- **Traffic**: requests and bytes served in the last hour, plus a
  serving-traffic chart with the same granularity toggle.
- **How to use this cache**: the substituter URL, the trusted public key derived
  from the configured signing secret, and a ready-to-paste `nix.conf` snippet,
  each with a copy button. These are exactly the values described in
  [INSTALL.md](./INSTALL.md) under "Binary Cache Storage".

The NARs page (`/caches/{name}/nars`) provides a searchable, paginated
inventory: filter by store-path hash prefix or package name, and inspect each
NAR's sizes, upload time, and last-fetched time. The matching JSON lives under
`GET /api/v1/admin/caches/{name}/nars`.

Operators can configure automatic age and size-pressure cleanup under
`[cache.gc]`. Cleanup applies globally to verified S3 objects tracked from agent
uploads, removes least-recently used objects first, and retries objects whose
deletion fails. See "Automatic Cache Cleanup" in [INSTALL.md](./INSTALL.md) for
the policy and its storage boundaries.

## First-Time Bootstrapping

On a fresh installation there are no users or API keys yet. You need an admin
account and an admin API key before you can use `circusctl admin` commands or
the admin dashboard.

### With the NixOS module

Set `declarative.users` and `declarative.api_keys` in the NixOS config. The
server reads these on startup and reconciles them with the database, creating or
updating records as needed. Prefer the `_file` variants to avoid storing secrets
in the world-readable Nix store:

```nix
{
  services.circus.settings.declarative.users = [
    {
      username = "admin";
      email = "admin@example.org";
      password_file = "/run/secrets/circus-admin-password"; # or agenix
      role = "admin";
    }
  ];

  services.circus.settings.declarative.api_keys = [
    {
      name = "admin-key";
      key_file = "/run/secrets/circus-admin-key"; # or agenix
      role = "admin";
    }
  ];
}
```

The `password` and `key` fields are also available for inline use during
testing, but the NixOS module emits a warning when they are used. Secrets placed
inline end up in the Nix store. Avoid this in production.

> [!TIP]
> API keys use the `circus_` prefix. Generate one with
> `echo "circus_$(openssl rand -hex 32)" > /path/to/key-file`.

### Without the NixOS module

Declarative config works the same way via `circus.toml`:

```toml
[declarative]

[[declarative.users]]
username = "admin"
email = "admin@example.org"
password_file = "/run/secrets/circus-admin-password"
role = "admin"

[[declarative.api_keys]]
name = "admin-key"
key_file = "/run/secrets/circus-admin-key"
role = "admin"
```

Inline `password` and `key` fields exist for testing, but avoid them in
production. Secrets in config files are not encrypted at rest. The server
reconciles declarative config with the database on every startup. Declarative
projects are read-only by default, and removed declarations remove their managed
database records. Set `declarative.allow_runtime_mutation = true` to allow
dashboard and API changes and preserve omitted projects and jobsets. Set
`allow_runtime_mutation` on an individual `[[declarative.projects]]` entry to
override the global policy for that project.

### Manual database insertion (any setup)

If you cannot restart the server or prefer direct SQL, insert records into
PostgreSQL:

```bash
# Create the first admin API key
export CIRCUS_KEY="circus_$(openssl rand -hex 32)"
export CIRCUS_HASH=$(echo -n "$CIRCUS_KEY" | sha256sum | cut -d' ' -f1)
sudo -u circus psql -U circus -d circus -c \
  "INSERT INTO api_keys (name, key_hash, role) VALUES ('admin', '$CIRCUS_HASH', 'admin')"

# Create the first admin user
# The password is hashed with argon2id, generating that hash from the shell
# is impractical. If you must insert users directly, use a known argon2 hash
# or prefer the declarative config approach.
```

> [!WARNING]
> The password must be an argon2id hash. Generating one from the shell is
> impractical; prefer the declarative config approach for user creation. Once
> you have an admin API key, use `circusctl admin users create` for additional
> users.

Once you have an admin API key, subsequent users and keys are managed with
`circusctl admin` (see below).

### API Keys

API keys authenticate API and `circusctl` requests. Keys are stored hashed; the
cleartext key is only shown when created.

```bash
# List app api-keys via administration CLI
$ circusctl admin api-keys list

# Create a new key with a specific role.
$ circusctl admin api-keys create --name deploy-bot --role admin

# Or a read-only key
$ circusctl admin api-keys create --name readonly-dashboard --role read-only

# Revoke an API key by-id
$ circusctl admin api-keys revoke <api-key-id>
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
$ circusctl admin users list

# Create a new user
$ circusctl admin users create \
  --username alice \
  --email alice@example.org \
  --password 'change-me' \
  --role read-only

# Assign a role to a user
$ circusctl admin users set-role <user-id> --role admin

# Disable a user's login, preventing them from logging in
$ circusctl admin users disable <user-id>

# Re-enable a user's login
$ circusctl admin users enable <user-id>

# Fully delete a user from the database
$ circusctl admin users delete <user-id>
```

Users can update their own profile through `/api/v1/me` and change their
password with `/api/v1/me/password`.

### Projects and Jobsets

A project maps to a source repository. A jobset defines what to evaluate inside
that repository and how evaluations are triggered.

Basic project commands:

```bash
# List all projects
$ circusctl projects list

# Create a new project from the CLI
$ circusctl projects create \
  --name nixpkgs \
  --repository-url https://github.com/NixOS/nixpkgs \
  --description "Nixpkgs CI"

# Delete a project by ID
$ circusctl projects delete <project-id>
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

> [!TIP]
> Source-change jobsets keep every triggered revision by default. Set
> `only_build_latest = true` on a declarative jobset (or
> `only_build_latest: true` through the API) to cancel older automated
> evaluations and builds in the same branch, change-request, or tag stream.
> Manual triggers and restarts are never superseded. During pattern polling,
> annotated tags are ordered by tagger time; lightweight tags fall back to the
> tagged commit time because Git does not record their creation time.

Source-change jobsets can set `path_filters` to a list of Git pathspecs. Circus
runs Nix evaluation when at least one changed path matches. A directory such as
`packages/hardened-kernel`, an exact file such as
`packages/hardened-kernel/default.nix`, and globs such as `**/*.nix` are
accepted. An evaluation with no known base revision always runs. If a recorded
base revision is unavailable after a force-push, Circus evaluates conservatively
instead of silently dropping work.

### Evaluations

An evaluation runs Nix evaluation for a jobset and creates builds. Users browse
evaluations in `/evaluations`, `/evaluation/{id}`, `/jobset/{id}`, and
`/jobset/{id}/jobs`.

Administrative commands:

```bash
# List all evaluations
$ circusctl evaluations list

# List all evaluations for a specific jobset, with a specific status
$ circusctl evaluations list --jobset-id <jobset-id> --status completed

# Trigger an evaluation for a jobset with a commit hash
$ circusctl evaluations trigger \
  --jobset-id <jobset-id> \
  --commit-hash <git-commit>
```

Pull-request metadata can be included when triggering manually:

```bash
# Alternatively, track a pull request
$ circusctl evaluations trigger \
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
$ circusctl builds list

# List all failed builds for a specific system
$ circusctl builds list --status failed --system x86_64-linux

# Cancel a build by ID
$ circusctl builds cancel <build-id>

# Restart a build by ID
$ circusctl builds restart <build-id>

# Bump a build's priority, also by ID
$ circusctl builds bump <build-id>

# Whether to pin a build to survive future garbage collections.
$ circusctl builds keep <build-id> --value true
$ circusctl builds keep <build-id> --value false
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
`circusctl
builds keep <build-id> --value false`, to make all outputs for that
build eligible for normal GC cleanup again after their roots age out.

The same data is available through the admin API:

```bash
# List retained products and their GC roots
$ curl -H "Authorization: Bearer $CIRCUS_API_KEY" \
    "$CIRCUS_URL/api/v1/admin/pinned-build-products"

# Unpin a build and all of its retained products
$ curl -X POST -H "Authorization: Bearer $CIRCUS_API_KEY" \
    "$CIRCUS_URL/api/v1/admin/pinned-builds/<build-id>/unpin"
```

### Build Agents

Circus dispatches distributed builds to persistent `circus-agent` processes that
connect to the queue-runner over Cap'n Proto RPC.

Persistent agent sessions are visible through the admin API:

```text
GET /api/v1/admin/builders/sessions
GET /api/v1/admin/builders/sessions/connected
GET /api/v1/admin/builders/sessions/{machine_id}
```

The same data is available from `circusctl`:

```bash
# List all recorded persistent and ephemeral agent sessions
$ circusctl admin builders sessions

# Only list currently connected sessions
$ circusctl admin builders sessions --connected

# Show one session by stable machine ID
$ circusctl admin builders session <machine-id>
```

The queue-runner prefers connected agents over SSH builders when both match a
build. See [DISTRIBUTED.md](./DISTRIBUTED.md) for protocol and security details.

### Notifications

Circus can send build and evaluation notifications through configured providers
and keeps a retry queue for failed deliveries.

Notification task commands:

```bash
# List all build notifications
$ circusctl admin notifications list

# Retry a notification by task ID
$ circusctl admin notifications retry <task-id>
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

`circusctl` can read or replace the server's declarative config body through the
admin API:

```bash
# View the *active* configuration
$ circusctl admin config get

# Replace the server's configured TOML file body
$ circusctl admin config apply /etc/circus/circus.toml
```

This updates the config file body exposed by the server. This feature is
experimental, and the runtime behavior still depends on which settings the
daemons reload dynamically and which require a service restart. For the full
configuration schema, see [INSTALL.md](./INSTALL.md) and the repository's
`circus.example.toml`. Until further notice, **use this with caution**.

### Audit Log

Use the audit log to inspect recent administrative actions:

```bash
# Check audit log with an entry limit
$ circusctl admin audit --limit 100
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
- Agents disappear.
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

If using distributed agents:

- Use TLS/mTLS and rotate RPC bearer tokens as part of normal secret rotation.
