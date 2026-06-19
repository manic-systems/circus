# Installing and Deploying Circus

This guide covers setup, deployment, and operational configuration for Circus.
This document covers only the installation and deployment steps. For day-to-day
usage after an instance is running, please take a look at the
[usage document](./USAGE.md).

## Quick Start

For brevity this quickstart guide assumes you're building Circus from source
while checked out to the repository. If you're using NixOS, follow the
[deploying on NixOS](#deploying-on-nixos) section to obtain the necessary
packages. You may also use `nix shell` to acquire the necessary components.

1. Enter the development shell and start PostgreSQL:

   ```bash
   nix develop
   initdb -D /tmp/circus-pg
   pg_ctl -D /tmp/circus-pg start
   createuser circus
   createdb -O circus circus
   ```

   If you have a running PostgreSQL instance, simply create the `circus` user
   and database. Give the `circus` user necessary privileges in the `circus`
   database.

2. Run migrations from the checkout:

   ```bash
   $ cargo run -p circus-cli -- migrate up postgresql://circus@localhost/circus
   ```

3. Start the server:

   ```bash
   # Assuming PostgreSQL is running on localhost. If it's running elsewhere, update
   # the database URL accordingly.
   $ CIRCUS_DATABASE__URL=postgresql://circus@localhost/circus \
       cargo run -p circus-server --
   ```

4. In separate shells, start the evaluator and queue runner when you want builds
   to be discovered and executed:

   ```bash
   $ CIRCUS_DATABASE__URL=postgresql://circus@localhost/circus \
       cargo run -p circus-evaluator --

   $ CIRCUS_DATABASE__URL=postgresql://circus@localhost/circus \
       cargo run -p circus-queue-runner --
   ```

5. Open `http://localhost:3000` in your browser.

The source quickstart starts with an empty database. Create the first admin API
key with the [authentication bootstrapping](#authentication-bootstrapping)
steps, or seed it declaratively before first server startup.

For installed binaries, the equivalent commands are `circusctl migrate`,
`circus-server`, `circus-evaluator`, and `circus-queue-runner`.

## Demo VM

A self-contained NixOS VM is available for trying Circus without any manual
setup. It runs `circus-server` with PostgreSQL, seeds demo API keys, and
forwards port 3000 to the host.

### Running

```bash
# Build the demo VM
$ nix build .#demo-vm

# Run the demo VM
$ ./result/bin/run-circus-demo-vm
```

The VM boots to a serial console (no graphical display). Once the boot
completes, the server is reachable from your host at `http://localhost:3000`.

### Pre-Seeded Credentials

To make testing easier, an admin key and a read-only API key are pre-seeded in
the demo VM.

| Key                        | Role        | Use for                      |
| -------------------------- | ----------- | ---------------------------- |
| `circus_demo_admin_key`    | `admin`     | Full access, dashboard login |
| `circus_demo_readonly_key` | `read-only` | Read-only API access         |

Log in to the dashboard at `http://localhost:3000/login` using the admin key.

### Example CLI Calls

Circus is designed as a server, and the dashboard is a convenient wrapper around
the API. For routine administration, prefer `circusctl`; it provides neat little
tables, clear errors, and safer request construction than ad-hoc shell snippets.

```bash
# Health check
$ circusctl --url http://localhost:3000 health

# Use an admin key for privileged commands
$ export CIRCUS_URL=http://localhost:3000
$ export CIRCUS_API_KEY=circus_demo_admin_key

# System status
$ circusctl admin status

# Create a project
$ circusctl projects create \
  --name my-project \
  --repository-url https://github.com/NixOS/nixpkgs

# List projects
$ circusctl projects list

# Create an additional read-only API key
$ circusctl admin api-keys create --name readonly-demo --role read-only
```

### Inside the VM

The serial console auto-logs in as root. While in the VM, you may use the TTY
access to investigate server logs or make API calls.

```bash
# Check server logs
$ systemctl status circus-server
$ journalctl -u circus-server -f

# Check instance health
$ circusctl health

# View metrics
$ curl -sf localhost:3000/prometheus
```

Press `Ctrl-a x` to shut down QEMU.

### VM Options

The VM uses QEMU user-mode networking. If port 3000 conflicts on your host, you
can override the QEMU options:

```bash
QEMU_NET_OPTS="hostfwd=tcp::8080-:3000" ./result/bin/run-circus-demo-vm
```

This makes the dashboard available at `http://localhost:8080` instead.

## Configuration

Circus service binaries require an explicit TOML configuration file. Pass it
with `--config` or set `CIRCUS_CONFIG_FILE`. Environment variables override the
file.

1. Compiled defaults
2. File from `--config` or `CIRCUS_CONFIG_FILE`
3. `CIRCUS_*` env vars (`__` as nested separator, e.g. `CIRCUS_DATABASE__URL`)

See `circus.example.toml` in the repository root for a compact example. The Rust
config types in `crates/config/src/structs.rs` remain the schema source of
truth.

### Configuration Reference

A maintained list of operator-facing configuration options. Secret-bearing
fields generally have a matching `*_file` form; inline values are useful for
development, while file-backed values avoid storing secrets in world-readable
configuration or the Nix store.

<!-- markdownlint-disable MD013 -->

| Section              | Key                                                    | Default                                             | Description                                      |
| -------------------- | ------------------------------------------------------ | --------------------------------------------------- | ------------------------------------------------ |
| `database`           | `url`                                                  | `postgresql://circus:password@localhost/circus`     | PostgreSQL connection URL                        |
| `database`           | `url_file`                                             | none                                                | File containing the PostgreSQL URL               |
| `database`           | `max_connections`                                      | `20`                                                | Maximum connection pool size                     |
| `database`           | `min_connections`                                      | `5`                                                 | Minimum idle connections                         |
| `database`           | `connect_timeout`                                      | `30`                                                | Connection timeout (seconds)                     |
| `database`           | `idle_timeout`                                         | `600`                                               | Idle connection timeout (seconds)                |
| `database`           | `max_lifetime`                                         | `1800`                                              | Maximum connection lifetime (seconds)            |
| `server`             | `host`                                                 | `127.0.0.1`                                         | HTTP listen address                              |
| `server`             | `port`                                                 | `3000`                                              | HTTP listen port                                 |
| `server`             | `request_timeout`                                      | `30`                                                | Per-request timeout (seconds)                    |
| `server`             | `max_body_size`                                        | `10485760`                                          | Maximum request body size (10 MB)                |
| `server`             | `api_key`                                              | none                                                | Optional legacy API key (prefer DB keys)         |
| `server`             | `api_key_file`                                         | none                                                | File containing the optional legacy API key      |
| `server`             | `cors_permissive`                                      | `false`                                             | Allow all CORS origins                           |
| `server`             | `allowed_origins`                                      | `[]`                                                | Allowed CORS origins list                        |
| `server`             | `force_secure_cookies`                                 | `false`                                             | Force Secure flag on cookies (HTTPS proxy)       |
| `server`             | `rate_limit_rps`                                       | none                                                | Requests per second limit per IP                 |
| `server`             | `rate_limit_burst`                                     | none                                                | Burst size for rate limiting                     |
| `server`             | `allowed_url_schemes`                                  | `[ "https", "git", "ssh" ]`                         | Allowed URL schemes for repository URLs          |
| `server`             | `config_editor_enabled`                                | `false`                                             | Allow admin config editing through API/dashboard |
| `server`             | `require_api_key_for_reads`                            | `true`                                              | Require auth for read-only `/api/v1` requests    |
| `server`             | `openapi_enabled`                                      | `true`                                              | Serve `/api/v1/openapi.json`                     |
| `server`             | `webhook_secret_encryption_key`                        | none                                                | Encrypt webhook and notification secrets         |
| `server`             | `webhook_secret_encryption_key_file`                   | none                                                | File containing the encryption key               |
| `server`             | `ldap.enabled`                                         | `true`                                              | Enable configured LDAP login                     |
| `server`             | `ldap.url`                                             | none                                                | LDAP server URL                                  |
| `server`             | `ldap.bind_dn_template`                                | none                                                | LDAP bind DN template (`{username}` placeholder) |
| `server`             | `ldap.base_dn`                                         | none                                                | LDAP base DN for user searches                   |
| `server`             | `ldap.tls_ca_cert`                                     | none                                                | Custom CA cert for LDAP TLS                      |
| `server`             | `email_validation_regex`                               | none                                                | Custom regex for email validation                |
| `server.page_access` | per page                                               | mixed                                               | Dashboard page visibility policy                 |
| `ui`                 | `enabled`                                              | `true`                                              | Mount bundled dashboard and static UI routes     |
| `ui`                 | `dashboard`                                            | `true`                                              | Mount server-rendered dashboard/login pages      |
| `ui`                 | `assets`                                               | `true`                                              | Serve bundled static UI assets                   |
| `ui`                 | `brand_name`                                           | `circus`                                            | Dashboard sidebar brand name                     |
| `ui`                 | `brand_subtitle`                                       | `Nix CI`                                            | Dashboard sidebar brand subtitle                 |
| `ui`                 | `logo_url`                                             | none                                                | Optional logo URL                                |
| `ui`                 | `favicon_url`                                          | none                                                | Optional favicon URL                             |
| `ui`                 | `custom_css`                                           | none                                                | CSS file served as `/static/custom.css`          |
| `ui`                 | `static_dir`                                           | none                                                | Directory served below `/static/custom/`         |
| `ui.css_variables`   | CSS variable names                                     | `{}`                                                | Variables emitted in `/static/theme.css`         |
| `evaluator`          | `poll_interval`                                        | `60`                                                | Seconds between git poll cycles                  |
| `evaluator`          | `git_timeout`                                          | `600`                                               | Git operation timeout (seconds)                  |
| `evaluator`          | `nix_timeout`                                          | `1800`                                              | Nix evaluation timeout (seconds)                 |
| `evaluator`          | `max_concurrent_evals`                                 | `4`                                                 | Maximum concurrent evaluations                   |
| `evaluator`          | `work_dir`                                             | `/tmp/circus-evaluator`                             | Working directory for clones                     |
| `evaluator`          | `restrict_eval`                                        | `true`                                              | Pass `--option restrict-eval true` to Nix        |
| `evaluator`          | `allow_ifd`                                            | `false`                                             | Allow import-from-derivation                     |
| `evaluator`          | `strict_errors`                                        | `false`                                             | Abort on first evaluation cycle error            |
| `queue_runner`       | `workers`                                              | `4`                                                 | Concurrent build slots                           |
| `queue_runner`       | `poll_interval`                                        | `5`                                                 | Seconds between build queue polls                |
| `queue_runner`       | `build_timeout`                                        | `3600`                                              | Per-build timeout (seconds)                      |
| `queue_runner`       | `max_silent_time`                                      | `0`                                                 | Fail agent builds silent for `N` seconds         |
| `queue_runner`       | `work_dir`                                             | `/tmp/circus-queue-runner`                          | Working directory for builds                     |
| `queue_runner`       | `strict_errors`                                        | `false`                                             | Abort on first runner loop error                 |
| `queue_runner`       | `failed_paths_cache`                                   | `true`                                              | Cache failed derivation paths                    |
| `queue_runner`       | `failed_paths_ttl`                                     | `86400`                                             | TTL for failed paths cache (seconds)             |
| `queue_runner`       | `unsupported_timeout`                                  | none                                                | Timeout for unsupported system builds            |
| `queue_runner`       | `scheduling_strategy`                                  | `speed_factor_only`                                 | Builder selection strategy                       |
| `queue_runner`       | `psi_threshold`                                        | none                                                | PSI pressure threshold (skip builders)           |
| `queue_runner`       | `psi_check_timeout`                                    | `5`                                                 | SSH PSI check timeout (seconds)                  |
| `queue_runner`       | `ssh_require_host_key`                                 | `false`                                             | Skip SSH builders without pinned host keys       |
| `queue_runner`       | `extra_nix_build_args`                                 | `[]`                                                | Extra arguments passed to `nix build`            |
| `queue_runner`       | `local_systems`                                        | none                                                | Systems the runner host may build locally        |
| `queue_runner`       | `local_features`                                       | none                                                | System features available on local builds        |
| `queue_runner`       | `rpc.bind`                                             | none                                                | Cap'n Proto RPC listen address                   |
| `queue_runner`       | `rpc.auth_tokens`                                      | `[]`                                                | Valid authentication tokens for agents           |
| `queue_runner`       | `rpc.max_connections`                                  | `256`                                               | Maximum concurrent agent connections             |
| `queue_runner`       | `rpc.presign_expiry_secs`                              | `3600`                                              | Presigned URL expiry (seconds)                   |
| `queue_runner`       | `rpc.tls`                                              | none                                                | TLS configuration for RPC endpoint               |
| `queue_runner`       | `rpc.heartbeat_ttl_secs`                               | `60`                                                | Agent heartbeat TTL before marking unavailable   |
| `queue_runner`       | `rpc.cache_substituter`                                | none                                                | Cache URL forwarded to agents for drv inputs     |
| `queue_runner`       | `rpc.cache_public_key`                                 | none                                                | Public key trusted for `rpc.cache_substituter`   |
| `queue_runner`       | `rpc.oidc.issuer`                                      | `https://token.actions.githubusercontent.com`       | OIDC issuer accepted for agent registration      |
| `queue_runner`       | `rpc.oidc.jwks_url`                                    | discovered                                          | JWKS endpoint override                           |
| `queue_runner`       | `rpc.oidc.audiences`                                   | `[]`                                                | Accepted OIDC audiences                          |
| `queue_runner`       | `rpc.oidc.allowed_repositories`                        | `[]`                                                | GitHub `owner/repo` slugs allowed to register    |
| `queue_runner`       | `rpc.oidc.allowed_subjects`                            | `[]`                                                | Exact OIDC `sub` claims allowed to register      |
| `queue_runner`       | `rpc.oidc.allowed_subject_prefixes`                    | `[]`                                                | OIDC `sub` prefixes allowed to register          |
| `queue_runner`       | `rpc.oidc.allowed_workflow_refs`                       | `[]`                                                | GitHub `workflow_ref` claims allowed             |
| `queue_runner`       | `rpc.oidc.allowed_refs`                                | `[]`                                                | GitHub `ref` claims allowed                      |
| `queue_runner`       | `ephemeral_pools`                                      | `[]`                                                | GitHub Actions ephemeral autoscaling pools       |
| `queue_runner`       | `ephemeral_pools[].name`                               | `gha-x86_64-linux`                                  | Pool name and base agent name                    |
| `queue_runner`       | `ephemeral_pools[].allowed_build_repositories`         | `[]`                                                | Source repositories this pool may build          |
| `queue_runner`       | `ephemeral_pools[].systems`                            | `[ "x86_64-linux" ]`                                | Systems advertised by pool agents                |
| `queue_runner`       | `ephemeral_pools[].supported_features`                 | `[]`                                                | Features advertised by pool agents               |
| `queue_runner`       | `ephemeral_pools[].mandatory_features`                 | `[]`                                                | Required build features for pool agents          |
| `queue_runner`       | `ephemeral_pools[].max_jobs`                           | `1`                                                 | Build slots per pool agent                       |
| `queue_runner`       | `ephemeral_pools[].cores`                              | `0`                                                 | Nix `cores` value passed to pool agents          |
| `queue_runner`       | `ephemeral_pools[].speed_factor`                       | `1.0`                                               | Scheduling weight for pool agents                |
| `queue_runner`       | `ephemeral_pools[].max_inflight`                       | `4`                                                 | Maximum workflow launches waiting to register    |
| `queue_runner`       | `ephemeral_pools[].inflight_ttl_secs`                  | `900`                                               | Time before an unregistered launch is forgotten  |
| `queue_runner`       | `ephemeral_pools[].scale_up_cooldown_secs`             | `30`                                                | Minimum delay between autoscaler launches        |
| `queue_runner`       | `ephemeral_pools[].poll_interval_secs`                 | `10`                                                | Autoscaler polling interval                      |
| `queue_runner`       | `ephemeral_pools[].github_actions.workflow_repository` | `""`                                                | GitHub repo containing the builder workflow      |
| `queue_runner`       | `ephemeral_pools[].github_actions.workflow`            | `circus-builder.yml`                                | Workflow file name or numeric workflow ID        |
| `queue_runner`       | `ephemeral_pools[].github_actions.ref_name`            | `main`                                              | Git ref used for workflow dispatch               |
| `queue_runner`       | `ephemeral_pools[].github_actions.token`               | none                                                | GitHub token with Actions write access           |
| `queue_runner`       | `ephemeral_pools[].github_actions.token_file`          | none                                                | File containing the GitHub token                 |
| `queue_runner`       | `ephemeral_pools[].github_actions.runner_url`          | `""`                                                | Runner URL passed to ephemeral agents            |
| `queue_runner`       | `ephemeral_pools[].github_actions.oidc_audience`       | `circus-agent`                                      | Audience requested by the builder workflow       |
| `queue_runner`       | `ephemeral_pools[].github_actions.agent_binary_url`    | `""`                                                | Pinned `circus-agent` binary URL                 |
| `gc`                 | `enabled`                                              | `true`                                              | Manage GC roots for build outputs                |
| `gc`                 | `gc_roots_dir`                                         | `/nix/var/nix/gcroots/per-user/circus/circus-roots` | GC roots directory                               |
| `gc`                 | `max_age_days`                                         | `30`                                                | Remove GC roots older than N days                |
| `gc`                 | `cleanup_interval`                                     | `3600`                                              | GC cleanup interval (seconds)                    |
| `logs`               | `log_dir`                                              | `/var/lib/circus/logs`                              | Build log storage directory                      |
| `logs`               | `compress`                                             | `false`                                             | Compress stored logs                             |
| `cache`              | `enabled`                                              | `true`                                              | Serve a Nix binary cache at `/nix-cache/`        |
| `cache`              | `secret_key_file`                                      | none                                                | Deprecated; outputs are signed via `[signing]`   |
| `cache`              | `cache_url`                                            | none                                                | Public cache URL for channel manifests           |
| `cache`              | `upstreams`                                            | `[]`                                                | Upstream binary caches used by global builds     |
| `signing`            | `enabled`                                              | `false`                                             | Sign build outputs                               |
| `signing`            | `key_file`                                             | none                                                | Signing key file path                            |
| `cache_upload`       | `enabled`                                              | `false`                                             | Upload builds to external cache store            |
| `cache_upload`       | `store_uri`                                            | none                                                | Cache store URI (`s3://bucket/path`)             |
| `cache_upload`       | `s3.region`                                            | none                                                | AWS region                                       |
| `cache_upload`       | `s3.prefix`                                            | none                                                | Extra path prefix within bucket                  |
| `cache_upload`       | `s3.access_key_id`                                     | none                                                | Access key for presigned S3 uploads/redirects    |
| `cache_upload`       | `s3.secret_access_key`                                 | none                                                | Secret key for presigned S3 uploads/redirects    |
| `cache_upload`       | `s3.secret_access_key_file`                            | none                                                | File containing S3 secret access key             |
| `cache_upload`       | `s3.session_token`                                     | none                                                | Session token for temporary credentials          |
| `cache_upload`       | `s3.session_token_file`                                | none                                                | File containing S3 session token                 |
| `cache_upload`       | `s3.endpoint_url`                                      | none                                                | S3-compatible endpoint URL                       |
| `cache_upload`       | `s3.use_path_style`                                    | `false`                                             | Use path-style addressing                        |
| `cache_upload`       | `upload_concurrency`                                   | `4`                                                 | Concurrent uploads per build                     |
| `cache_upload`       | `upload_max_retries`                                   | `3`                                                 | Max retry attempts per path                      |
| `cache_upload`       | `fail_build_on_upload_error`                           | `false`                                             | Mark build failed on upload error                |
| `cache_upload`       | `compression`                                          | `zstd`                                              | Agent presigned-upload NAR compression           |
| `notifications`      | `webhook_url`                                          | none                                                | HTTP endpoint for build status JSON              |
| `notifications`      | `webhook_url_file`                                     | none                                                | File containing generic webhook URL              |
| `notifications`      | `github_token`                                         | none                                                | GitHub token for commit status updates           |
| `notifications`      | `github_token_file`                                    | none                                                | File containing GitHub token                     |
| `notifications`      | `gitea_url`                                            | none                                                | Gitea/Forgejo instance URL                       |
| `notifications`      | `gitea_token`                                          | none                                                | Gitea/Forgejo API token                          |
| `notifications`      | `gitea_token_file`                                     | none                                                | File containing Gitea/Forgejo token              |
| `notifications`      | `gitlab_url`                                           | none                                                | GitLab instance URL                              |
| `notifications`      | `gitlab_token`                                         | none                                                | GitLab API token                                 |
| `notifications`      | `gitlab_token_file`                                    | none                                                | File containing GitLab token                     |
| `notifications`      | `enable_retry_queue`                                   | `true`                                              | Persistent retry queue with backoff              |
| `notifications`      | `max_retry_attempts`                                   | `5`                                                 | Max notification retry attempts                  |
| `notifications`      | `retention_days`                                       | `7`                                                 | Retention for completed notification tasks       |
| `notifications`      | `retry_poll_interval`                                  | `5`                                                 | Retry poll interval (seconds)                    |
| `notifications`      | `email.smtp_host`                                      | none                                                | SMTP host for email notifications                |
| `notifications`      | `email.smtp_port`                                      | none                                                | SMTP port                                        |
| `notifications`      | `email.smtp_user`                                      | none                                                | SMTP username (optional)                         |
| `notifications`      | `email.smtp_password`                                  | none                                                | SMTP password (optional)                         |
| `notifications`      | `email.smtp_password_file`                             | none                                                | File containing SMTP password                    |
| `notifications`      | `email.tls`                                            | `false`                                             | Enable TLS for SMTP connection                   |
| `notifications`      | `email.from_address`                                   | none                                                | From address for notification emails             |
| `notifications`      | `email.to_addresses`                                   | `[]`                                                | Recipient addresses                              |
| `notifications`      | `slack.webhook_url`                                    | none                                                | Slack incoming webhook URL                       |
| `notifications`      | `slack.webhook_url_file`                               | none                                                | File containing Slack webhook URL                |
| `notifications`      | `slack.on_failure_only`                                | `false`                                             | Only send Slack alerts on failure                |
| `notifications`      | `alerts.enabled`                                       | `false`                                             | Enable error-rate threshold alerts               |
| `notifications`      | `alerts.error_threshold`                               | `0.5`                                               | Error rate threshold to trigger alert            |
| `notifications`      | `alerts.time_window_minutes`                           | `60`                                                | Time window for error rate calculation           |
| `tracing`            | `level`                                                | `info`                                              | Log level (trace/debug/info/warn/error)          |
| `tracing`            | `format`                                               | `compact`                                           | Log output format                                |
| `tracing`            | `show_targets`                                         | `true`                                              | Show module path in log messages                 |
| `tracing`            | `show_timestamps`                                      | `true`                                              | Show timestamps in log messages                  |
| `oauth`              | `github.client_id`                                     | none                                                | GitHub OAuth App client ID                       |
| `oauth`              | `github.client_secret`                                 | none                                                | GitHub OAuth App client secret                   |
| `oauth`              | `github.client_secret_file`                            | none                                                | File containing GitHub OAuth secret              |
| `oauth`              | `github.redirect_uri`                                  | none                                                | OAuth redirect URI                               |
| `declarative`        | `projects`                                             | `[]`                                                | Declarative project definitions                  |
| `declarative`        | `api_keys`                                             | `[]`                                                | Declarative API key definitions                  |
| `declarative`        | `users`                                                | `[]`                                                | Declarative user definitions                     |
| `declarative`        | `remote_builders`                                      | `[]`                                                | Declarative remote builder definitions           |
| `nix`                | `store_dir`                                            | `/nix/store`                                        | Nix store directory                              |

<!-- markdownlint-enable MD013 -->

## Binary Cache Storage

Set `[cache].enabled = true` on the server to expose the global cache at
`/nix-cache/`. Each project also exposes its own cache at
`/projects/<project-name>/nix-cache/` when that project's `cache_enabled` field
is true. Global cache availability is controlled only by `[cache].enabled`;
project caches are controlled per project.

For outputs present in the server's Nix store, Circus generates narinfo from
`nix path-info` and serves the corresponding NAR from the store. Project caches
only serve outputs that belong to that project. The global cache preserves the
old behavior and can serve all eligible Circus outputs.

Only two kinds of local store paths are served: build outputs the queue-runner
signed at build time (`[signing]` with a `key_file`; unsigned outputs are never
exposed), and content-addressed paths such as drvs and sources, which Nix
verifies against their `CA:` field and which agents substitute when starting
dispatched builds. Without a signing key the cache serves nothing beyond drv
closures.

Set `[cache_upload].enabled = true` and `store_uri = "s3://bucket[/prefix]"` to
push completed outputs to S3. SSH/local runner builds use `nix copy --to`; agent
builds use presigned PUT URLs and persist narinfo rows in the database. When a
client later requests those NARs through `/nix-cache/nar/...`, the server signs
a short-lived S3 GET URL and redirects the client.

> [!NOTE] For presigned agent uploads, the queue-runner fetches the uploaded
> object back from S3 before signing narinfo. It verifies the compressed file
> hash/size, decompresses the file according to the advertised compression,
> recomputes the NAR hash/size, and rejects mismatches without persisting the
> upload.

If both `store_uri` and `s3.prefix` contain paths, Circus combines them. For
example, `store_uri = "s3://bucket/root"` plus `s3.prefix = "nix-cache"` writes
objects below `root/nix-cache/`.

### Public Binary Cache Use

Configure a public URL for the global cache with `[cache].cache_url`, for
example:

```toml
[cache]
enabled = true
cache_url = "https://ci.example.org/nix-cache/"

[[cache.upstreams]]
url = "https://cache.nixos.org/"
public_key = "cache.nixos.org-1:..."
```

Project cache URLs are stored on the project record. Declarative projects may
set them in `[[declarative.projects]]`:

```toml
[[declarative.projects]]
name = "my-project"
repository_url = "https://github.com/example/my-project"
cache_enabled = true
cache_url = "https://ci.example.org/projects/my-project/nix-cache/"

[[declarative.projects.cache_upstreams]]
url = "https://cache.nixos.org/"
public_key = "cache.nixos.org-1:..."
```

Users add the cache and its upstreams to `nix.conf` manually:

```text
extra-substituters = https://ci.example.org/projects/my-project/nix-cache/ https://cache.nixos.org/
extra-trusted-public-keys = cache.nixos.org-1:...
```

Circus also passes the selected cache and upstream substituters to builds it
controls. Project builds use the project's cache URL and upstreams; builds
without project context use the global cache URL and upstreams. Upstreams are
contacted directly by Nix clients/builders; Circus does not proxy upstream
narinfos or NARs.

## Dashboard Page Access

Dashboard pages have conservative defaults: the home page is public, most
browsing pages require an authenticated user, and queue/metrics views require an
admin. Operators can loosen or tighten individual pages through
`[server.page_access]`. Valid values are `public`, `authenticated`, and `admin`.

```toml
[server.page_access]
home        = "public"
projects    = "authenticated"
project     = "authenticated"
jobset      = "authenticated"
jobset_jobs = "authenticated"
evaluations = "authenticated"
evaluation  = "authenticated"
builds      = "authenticated"
build       = "authenticated"
queue       = "admin"
channels    = "authenticated"
channel     = "authenticated"
news        = "authenticated"
starred     = "authenticated"
metrics     = "admin"
```

Admin-only pages such as `/admin`, `/users`, and project notification settings
remain admin-only regardless of this policy. API endpoint authorization is
separate from dashboard page visibility.

> [!NOTE]
> The admin config editor is disabled by default. Set
> `server.config_editor_enabled = true` only when you explicitly want admins to
> replace the configured TOML file body through the dashboard or
> `PUT /api/v1/admin/config`. The `GET` endpoint still returns the
> default-backed effective config for inspection.

## Authentication Providers

The dashboard login page always supports local username/password users and API
key login. Additional identity providers are exposed as API-backed login flows
when configured.

GitHub OAuth requires an OAuth App with a callback pointing at the configured
redirect URI, usually `https://ci.example.org/api/v1/auth/github/callback`:

```toml
[oauth.github]
client_id = "..."
client_secret = "..."
redirect_uri = "https://ci.example.org/api/v1/auth/github/callback"
```

Users start that flow at `/api/v1/auth/github`. On first login, Circus creates
or updates a read-only user record for the GitHub identity.

LDAP bind login is enabled through `[server.ldap]` and exposed at `/auth/ldap`:

```toml
[server.ldap]
enabled = true
url = "ldaps://ldap.example.org"
bind_dn_template = "uid={username},ou=people,dc=example,dc=org"
base_dn = "ou=people,dc=example,dc=org"
# tls_ca_cert = "/etc/ssl/certs/example-ldap-ca.pem"
```

The LDAP endpoint accepts a JSON body with `username` and `password`, performs a
simple bind after escaping the username for DN substitution, and returns a
dashboard session cookie on success. LDAP and OAuth users still receive their
Circus role and enabled/disabled state from the local user record.

## Database

Circus uses PostgreSQL with [sqlx](https://crates.io/crates/sqlx). Migrations
live in `crates/migrations/migrations/` and are added when the database schema
changes.

```bash
# Run pending migrations
$ circusctl migrate up <database_url>

# Validate schema
$ circusctl migrate validate <database_url>

# Create a new migration file
$ circusctl migrate create <name>
```

## Deploying on NixOS

Circus, for the time being, only supports being deployed on NixOS systems. While
it is possible to run on _any_ system with a Nix installation, it might be
rather clunky. You're encouraged to provide documentation for alternative
methods if you successfully run them.

Circus ships a NixOS module at `nixosModules.default`. Minimal configuration:

```nix
{
  inputs.circus.url = "github:manic-systems/circus";

  outputs = { self, nixpkgs, circus, ... }: {
    nixosConfigurations.myhost = nixpkgs.lib.nixosSystem {
      modules = [
        circus.nixosModules.default
        {
          services.circus = {
            enable = true;
            package = circus.packages.x86_64-linux.circus-server;
            migratePackage = circus.packages.x86_64-linux.circus-cli;

            server.enable = true;
            # evaluator.enable = true;
            # queueRunner.enable = true;
          };
        }
      ];
    };
  };
}
```

### Full Deployment Example

A complete production configuration with all three daemons and NGINX reverse
proxy:

```nix
{ inputs, pkgs, ... }: let
  circusPkgs = inputs.circus.packages.${pkgs.stdenv.hostPlatform.system};
in {
  networking.firewall.allowedTCPPorts = [ 80 443 ];
  services.circus = {
    enable = true;
    package = circusPkgs.circus-server;
    evaluatorPackage = circusPkgs.circus-evaluator;
    queueRunnerPackage = circusPkgs.circus-queue-runner;
    migratePackage = circusPkgs.circus-cli;

    server.enable = true;
    evaluator.enable = true;
    queueRunner.enable = true;

    settings = {
      database.url = "postgresql:///circus?host=/run/postgresql";
      server.host = "127.0.0.1";
      server.port = 3000;

      server.force_secure_cookies = true;
      server.rate_limit_rps = 100;
      server.rate_limit_burst = 20;

      evaluator.poll_interval = 300;
      evaluator.restrict_eval = true;
      queue_runner.workers = 8;
      queue_runner.build_timeout = 7200;

      gc.enabled = true;
      gc.max_age_days = 90;
      cache.enabled = true;
      logs.log_dir = "/var/lib/circus/logs";
      logs.compress = true;
    };
  };

  services.nginx = {
    enable = true;
    virtualHosts."ci.example.org" = {
      forceSSL = true;
      enableACME = true;
      locations."/" = {
        proxyPass = "http://127.0.0.1:3000";
        proxyWebsockets = true;
        extraConfig = ''
          proxy_set_header X-Real-IP $remote_addr;
          proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
          proxy_set_header X-Forwarded-Proto $scheme;
          client_max_body_size 50M;
        '';
      };
    };
  };
}
```

### Multi-Machine Deployment

For larger or distributed setups, you may choose to run the daemons on different
machines sharing the same database. For example:

- **Head node**: runs `circus-server` and `circus-evaluator`, has the PostgreSQL
  database locally
- **Builder machines**: run `circus-queue-runner`, connect to the head node's
  database via `postgresql://circus@headnode/circus`

On builder machines, set `database.createLocally = false` and provide the remote
database URL:

```nix
{
  services.circus = {
    enable = true;
    database.createLocally = false;
    queueRunner.enable = true;

    settings.database.url = "postgresql://circus@headnode.internal/circus";
    settings.queue_runner.workers = 16;
  };
}
```

Ensure the PostgreSQL server on the head node allows connections from builder
machines via `pg_hba.conf` or equivalent NixOS PostgreSQL module settings.

## Building Installable Packages

The flake exposes one package per binary:

```bash
$ nix build .#circus-server
$ nix build .#circus-evaluator
$ nix build .#circus-queue-runner
$ nix build .#circus-cli
$ nix build .#circus-agent
```

For local source builds without Nix packaging, use Cargo package names:

```bash
$ cargo build -p circus-server
$ cargo build -p circus-evaluator
$ cargo build -p circus-queue-runner
$ cargo build -p circus-cli
$ cargo build -p circus-agent
```

Run all service binaries with the same `--config <path>` or
`CIRCUS_CONFIG_FILE`. The NixOS module runs migrations before starting
`circus-server`; manual or non-NixOS deployments should run
`circus-migrate up <database_url>` before starting upgraded services.

## Distributed Builders

Circus supports SSH remote builders and persistent `circus-agent` builders.
Detailed usage and operations are covered in [USAGE.md](./USAGE.md); protocol
details are covered in [DISTRIBUTED.md](./DISTRIBUTED.md).

Agent configuration is loaded from `/etc/circus-agent.toml`, the path passed to
`--config`, or `CIRCUS_AGENT_CONFIG`. Environment overrides use the
`CIRCUS_AGENT__` prefix and `__` as a path separator. These fields are the
agent-side settings most relevant to distributed and ephemeral builders:

<!-- markdownlint-disable MD013 -->

| Section | Field                         | Default                 | Description                                                       |
| ------- | ----------------------------- | ----------------------- | ----------------------------------------------------------------- |
| `agent` | `name`                        | required                | Operator-assigned agent name                                      |
| `agent` | `runner_url`                  | required                | `circus://` or `circus+tls://` runner endpoint                    |
| `agent` | `auth_token`                  | required                | Bearer token or OIDC JWT; may be supplied by `CIRCUS_AGENT_TOKEN` |
| `agent` | `systems`                     | required                | Nix systems advertised by the agent                               |
| `agent` | `supported_features`          | `[]`                    | Build features the agent can satisfy                              |
| `agent` | `mandatory_features`          | `[]`                    | Features every assigned build must require                        |
| `agent` | `max_jobs`                    | `4`                     | Maximum concurrent builds                                         |
| `agent` | `cores`                       | `0`                     | Nix `cores` value for each build; `0` keeps the host default      |
| `agent` | `speed_factor`                | `1.0`                   | Scheduling weight relative to other agents                        |
| `agent` | `reconnect_delay_secs`        | `5`                     | Delay before reconnecting after a dropped connection              |
| `agent` | `heartbeat_interval_secs`     | `10`                    | Interval between runner heartbeats                                |
| `agent` | `work_dir`                    | `/var/lib/circus-agent` | Transient agent state directory                                   |
| `agent` | `machine_id_file`             | `<work_dir>/machine_id` | Persistent machine ID path for non-ephemeral agents               |
| `agent` | `tls.ca_file`                 | none                    | CA file used to trust the runner certificate                      |
| `agent` | `tls.cert_file`               | none                    | Optional client certificate for mTLS                              |
| `agent` | `tls.key_file`                | none                    | Optional client key for mTLS                                      |
| `agent` | `rootless`                    | `false`                 | Run Nix inside the rootless namespace setup described below       |
| `agent` | `rootless_data_dir`           | XDG data dir            | Data directory used by rootless mode                              |
| `agent` | `ephemeral.max_builds`        | none                    | Exit after this many completed builds                             |
| `agent` | `ephemeral.max_lifetime_secs` | none                    | Exit after this many wall-clock seconds                           |
| `agent` | `ephemeral.max_idle_secs`     | `120`                   | Exit after this many idle seconds                                 |
| `agent` | `ephemeral.unique_name`       | `true`                  | Append a unique suffix to avoid CI name collisions                |

<!-- markdownlint-enable MD013 -->

### Ephemeral GitHub Actions Agents

Ephemeral agents are short-lived `circus-agent` processes intended for CI
runners. They mint a fresh machine ID, register once, drain any assigned work,
and exit instead of reconnecting. Enable the mode with
`circus-agent
--ephemeral` or by adding `[agent.ephemeral]` to the agent config.

Queue-runner-driven GitHub Actions scaling needs an RPC endpoint, a derivation
substituter for fresh CI machines, OIDC registration, and at least one ephemeral
pool:

```toml
[queue_runner.rpc]
bind = "0.0.0.0:8443"
cache_substituter = "https://ci.example.org/nix-cache/"
cache_public_key = "circus-cache:..."

[queue_runner.rpc.oidc]
issuer = "https://token.actions.githubusercontent.com"
audiences = [ "circus-agent" ]
allowed_repositories = [ "example/circus-builders" ]
allowed_subject_prefixes = [
  "repo:example/circus-builders:ref:refs/heads/main",
]
allowed_workflow_refs = [
  "example/circus-builders/.github/workflows/circus-builder.yml@refs/heads/main",
]

[[queue_runner.ephemeral_pools]]
name = "gha-x86_64-linux"
allowed_build_repositories = [ "example/project" ]
systems = [ "x86_64-linux" ]
supported_features = [ "kvm", "nixos-test" ]
max_inflight = 4

[queue_runner.ephemeral_pools.github_actions]
workflow_repository = "example/circus-builders"
workflow = "circus-builder.yml"
ref_name = "main"
token_file = "/run/credentials/circus-queue-runner/github-token"
runner_url = "circus+tls://ci.example.org:8443"
oidc_audience = "circus-agent"
agent_binary_url = "https://ci.example.org/artifacts/circus-agent-x86_64-linux"
```

Install `.github/workflows/circus-builder.yml` in the repository named by
`github_actions.workflow_repository`. The workflow must allow
`workflow_dispatch` and grant `id-token: write`. The queue-runner passes plain
string inputs, the workflow installs Nix, downloads the pinned
`agent_binary_url`, requests a GitHub OIDC token, and starts
`circus-agent --ephemeral` with direct CLI flags.

Important constraints:

- `queue_runner.rpc.cache_substituter` and `cache_public_key` are required so a
  clean GitHub runner can substitute the assigned derivation closure.
- `queue_runner.rpc.oidc.audiences` must include `github_actions.oidc_audience`.
- `queue_runner.rpc.oidc.allowed_repositories` must include
  `github_actions.workflow_repository`; an empty allowlist rejects all OIDC
  agents.
- Restrict OIDC `sub` prefixes or `workflow_ref` values to the intended builder
  workflow/ref.
- The GitHub token in `github_actions.token` or `github_actions.token_file`
  needs permission to create workflow dispatches for the configured workflow
  repository.
- The autoscaler only launches runners for trusted, non-PR builds whose project
  repository is listed in `allowed_build_repositories` and whose
  systems/features fit the pool configuration.
- `max_inflight`, `inflight_ttl_secs`, and `scale_up_cooldown_secs` control
  launch pressure; they do not bypass normal scheduler trust checks.

### Rootless Agents

`circus-agent` can run on machines where you have an unprivileged shell account
and no Nix installation (such as unprivileged ssh access). Set `rootless = true`
under `[agent]` and the agent executes every Nix invocation inside a
`user+mount` namespace, with `$XDG_DATA_HOME/circus-agent` mounted as `/nix`,
build scratch under its `tmp/`, and `proc`/`dev`/DNS/CA bind-mounts from the
host.

Requirements:

- Unprivileged user namespaces must be enabled on the host kernel. You can check
  if they are with `unshare --user --map-root-user true` succeeding.
- `CIRCUS_AGENT_NIX` must point to a `nix` (or `nix-store`) binary **at its
  `/nix/store` path as seen inside the sandbox**, that is, the binary and its
  closure must be seeded into `$XDG_DATA_HOME/circus-agent/store` first. This
  can be done by unpacking a Nix release tarball there. A host path like
  `~/.nix-profile/bin/nix` will not exist after the sandbox pivots its root.

The data directory can be moved with `rootless_data_dir` under `[agent]` (or the
`CIRCUS_AGENT_DATA_DIR` environment variable) for hosts without a usable home
directory.

The agent validates both requirements at startup and refuses to register when
they fail. Nix settings for builds come from `etc/nix/nix.conf` in the data dir.
By default Nix sandboxes each build in a nested user namespace inside the
agent's, however, if the host kernel does not support this, just set
`sandbox = false` there.

Rootless mode is an isolation mechanism, not a security boundary. The namespace
only exists to give Nix the filesystem layout it expects. Builds still run as
your real uid with unrestricted network access. Do not rely on it to contain
untrusted build code.

## Authentication Bootstrapping

Circus supports API keys, local users, GitHub OAuth, and LDAP. Day-to-day user
and admin workflows are covered in [USAGE.md](./USAGE.md). The most important
installation-time task is creating the first admin API key. SHA-256 hashed API
keys are stored in the `api_keys` table. To create the first admin key after
initial deployment:

<!--markdownlint-disable MD013-->

```bash
# Generate a key and its hash
$ export CIRCUS_KEY="circus_$(openssl rand -hex 16)"
$ export CIRCUS_HASH=$(echo -n "$CIRCUS_KEY" | sha256sum | cut -d' ' -f1)

# Insert into the database
$ sudo -u circus psql -U circus -d circus -c \
  "INSERT INTO api_keys (name, key_hash, role) VALUES ('admin', '$CIRCUS_HASH', 'admin')"

# XXX: Save the key. It cannot be recovered from the hash.
# echo "Admin API key: $CIRCUS_KEY"
```

<!--markdownlint-enable MD013-->

> [!TIP]
> Subsequent keys can be created with `circusctl admin api-keys create` or the
> admin dashboard using this initial admin key.

## Monitoring

Circus exposes a Prometheus-compatible metrics endpoint at `/prometheus`.

```yaml
scrape_configs:
  - job_name: "circus-ci"
    static_configs:
      - targets: ["ci.example.org:3000"]
    metrics_path: "/prometheus"
    scrape_interval: 30s
```

The `/health` endpoint reports database and service status. Administrative
status is available through `circusctl admin status` or `/api/v1/admin/system`.

## Backup and Restore

Until Circus reaches 1.0.0, take regular backups. Circus state is primarily in
PostgreSQL plus build logs on disk.

```bash
# Create a backup
$ pg_dump -U circus circus > circus-backup-$(date +%Y%m%d).sql

# Restore a backup
$ psql -U circus circus < circus-backup-20250101.sql
```

Build logs are stored in the filesystem at the configured `logs.log_dir`
(defaults to `/var/lib/circus/logs`). Include this directory in your backup
strategy to preserve logs across catastrophic failures. Build outputs live in
the Nix store and are protected by GC roots under `gc.gc_roots_dir`; they do not
need a separate database backup as long as derivation paths are retained in the
database.

Also back up:

- The active configuration file.
- API/webhook/OAuth/LDAP/S3 secrets in your secret manager.
- Binary cache signing keys.
- Agent RPC tokens and TLS material if using persistent agents.
