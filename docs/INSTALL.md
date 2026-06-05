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

2. Run migrations using the migration CLI:

   ```bash
   # Run migrations
   $ circus-migrate -- up postgresql://circus@localhost/circus
   ```

3. Start the server:

   ```bash
   # Assuming PostgreSQL is running on localhost. If it's running elsewhere, update
   # the database URL accordingly.
   $ CIRCUS_DATABASE__URL=postgresql://circus@localhost/circus circus-server
   ```

4. Open `http://localhost:3000` in your browser.

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

### Example Admin CLI Calls

Circus is designed as a server, and the dashboard is a convenient wrapper around
the API. For routine administration, prefer `circus-admin`; it provides neat
little tables, clear errors, and safer request construction than ad-hoc shell
snippets.

```bash
# Health check
$ circus-admin --url http://localhost:3000 health

# Use an admin key for privileged commands
$ export CIRCUS_URL=http://localhost:3000
$ export CIRCUS_API_KEY=circus_demo_admin_key

# System status
$ circus-admin status

# Create a project
$ circus-admin projects create \
  --name my-project \
  --repository-url https://github.com/NixOS/nixpkgs

# List projects
$ circus-admin projects list

# Create an additional read-only API key
$ circus-admin api-keys create --name readonly-demo --role read-only
```

### Inside the VM

The serial console auto-logs in as root. While in the VM, you may use the TTY
access to investigate server logs or make API calls.

```bash
# Check server logs
$ systemctl status circus-server
$ journalctl -u circus-server -f

# Check instance health
$ circus-admin health

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

Circus reads configuration from a TOML file with environment variable overrides.
The override hierarchy is as follows:

1. Compiled defaults
2. `circus.toml` in the working directory
3. File at `CIRCUS_CONFIG_FILE` env var
4. `CIRCUS_*` env vars (`__` as nested separator, e.g. `CIRCUS_DATABASE__URL`)

See `circus.toml` in the repository root for the full schema with comments.

### Configuration Reference

A somewhat maintained list of configuration options. It may be outdated during
development; `circus.toml` remains the practical reference.

<!-- markdownlint-disable MD013 -->

| Section              | Key                          | Default                                             | Description                                      |
| -------------------- | ---------------------------- | --------------------------------------------------- | ------------------------------------------------ |
| `database`           | `url`                        | `postgresql://circus:password@localhost/circus`     | PostgreSQL connection URL                        |
| `database`           | `max_connections`            | `20`                                                | Maximum connection pool size                     |
| `database`           | `min_connections`            | `5`                                                 | Minimum idle connections                         |
| `database`           | `connect_timeout`            | `30`                                                | Connection timeout (seconds)                     |
| `database`           | `idle_timeout`               | `600`                                               | Idle connection timeout (seconds)                |
| `database`           | `max_lifetime`               | `1800`                                              | Maximum connection lifetime (seconds)            |
| `server`             | `host`                       | `127.0.0.1`                                         | HTTP listen address                              |
| `server`             | `port`                       | `3000`                                              | HTTP listen port                                 |
| `server`             | `request_timeout`            | `30`                                                | Per-request timeout (seconds)                    |
| `server`             | `max_body_size`              | `10485760`                                          | Maximum request body size (10 MB)                |
| `server`             | `api_key`                    | none                                                | Optional legacy API key (prefer DB keys)         |
| `server`             | `cors_permissive`            | `false`                                             | Allow all CORS origins                           |
| `server`             | `allowed_origins`            | `[]`                                                | Allowed CORS origins list                        |
| `server`             | `force_secure_cookies`       | `false`                                             | Force Secure flag on cookies (HTTPS proxy)       |
| `server`             | `rate_limit_rps`             | none                                                | Requests per second limit per IP                 |
| `server`             | `rate_limit_burst`           | none                                                | Burst size for rate limiting                     |
| `server`             | `allowed_url_schemes`        | `[]`                                                | Allowed URL schemes for repository URLs          |
| `server`             | `config_editor_enabled`      | `true`                                              | Allow admin config editing through API/dashboard |
| `server`             | `ldap.enabled`               | `true`                                              | Enable configured LDAP login                     |
| `server`             | `ldap.url`                   | none                                                | LDAP server URL                                  |
| `server`             | `ldap.bind_dn_template`      | none                                                | LDAP bind DN template (`{username}` placeholder) |
| `server`             | `ldap.base_dn`               | none                                                | LDAP base DN for user searches                   |
| `server`             | `ldap.tls_ca_cert`           | none                                                | Custom CA cert for LDAP TLS                      |
| `server`             | `email_validation_regex`     | none                                                | Custom regex for email validation                |
| `server.page_access` | per page                     | public                                              | Dashboard page visibility policy                 |
| `evaluator`          | `poll_interval`              | `60`                                                | Seconds between git poll cycles                  |
| `evaluator`          | `git_timeout`                | `600`                                               | Git operation timeout (seconds)                  |
| `evaluator`          | `nix_timeout`                | `1800`                                              | Nix evaluation timeout (seconds)                 |
| `evaluator`          | `max_concurrent_evals`       | `4`                                                 | Maximum concurrent evaluations                   |
| `evaluator`          | `work_dir`                   | `/tmp/circus-evaluator`                             | Working directory for clones                     |
| `evaluator`          | `restrict_eval`              | `true`                                              | Pass `--option restrict-eval true` to Nix        |
| `evaluator`          | `allow_ifd`                  | `false`                                             | Allow import-from-derivation                     |
| `evaluator`          | `strict_errors`              | `false`                                             | Abort on first evaluation cycle error            |
| `queue_runner`       | `workers`                    | `4`                                                 | Concurrent build slots                           |
| `queue_runner`       | `poll_interval`              | `5`                                                 | Seconds between build queue polls                |
| `queue_runner`       | `build_timeout`              | `3600`                                              | Per-build timeout (seconds)                      |
| `queue_runner`       | `max_silent_time`            | `0`                                                 | Fail agent builds silent for `N` seconds         |
| `queue_runner`       | `work_dir`                   | `/tmp/circus-queue-runner`                          | Working directory for builds                     |
| `queue_runner`       | `strict_errors`              | `false`                                             | Abort on first runner loop error                 |
| `queue_runner`       | `failed_paths_cache`         | `true`                                              | Cache failed derivation paths                    |
| `queue_runner`       | `failed_paths_ttl`           | `86400`                                             | TTL for failed paths cache (seconds)             |
| `queue_runner`       | `unsupported_timeout`        | none                                                | Timeout for unsupported system builds            |
| `queue_runner`       | `scheduling_strategy`        | `speed_factor_only`                                 | Builder selection strategy                       |
| `queue_runner`       | `psi_threshold`              | none                                                | PSI pressure threshold (skip builders)           |
| `queue_runner`       | `psi_check_timeout`          | `5`                                                 | SSH PSI check timeout (seconds)                  |
| `queue_runner`       | `extra_nix_build_args`       | `[]`                                                | Extra arguments passed to `nix build`            |
| `queue_runner`       | `rpc.bind`                   | none                                                | Cap'n Proto RPC listen address                   |
| `queue_runner`       | `rpc.auth_tokens`            | `[]`                                                | Valid authentication tokens for agents           |
| `queue_runner`       | `rpc.max_connections`        | `256`                                               | Maximum concurrent agent connections             |
| `queue_runner`       | `rpc.presign_expiry_secs`    | `3600`                                              | Presigned URL expiry (seconds)                   |
| `queue_runner`       | `rpc.tls`                    | none                                                | TLS configuration for RPC endpoint               |
| `queue_runner`       | `rpc.heartbeat_ttl_secs`     | `60`                                                | Agent heartbeat TTL before marking unavailable   |
| `gc`                 | `enabled`                    | `true`                                              | Manage GC roots for build outputs                |
| `gc`                 | `gc_roots_dir`               | `/nix/var/nix/gcroots/per-user/circus/circus-roots` | GC roots directory                               |
| `gc`                 | `max_age_days`               | `30`                                                | Remove GC roots older than N days                |
| `gc`                 | `cleanup_interval`           | `3600`                                              | GC cleanup interval (seconds)                    |
| `logs`               | `log_dir`                    | `/var/lib/circus/logs`                              | Build log storage directory                      |
| `logs`               | `compress`                   | `false`                                             | Compress stored logs                             |
| `cache`              | `enabled`                    | `true`                                              | Serve a Nix binary cache at `/nix-cache/`        |
| `cache`              | `secret_key_file`            | none                                                | Deprecated; outputs are signed via `[signing]`   |
| `cache`              | `compression`                | `zstd`                                              | NAR compression algorithm                        |
| `cache`              | `cache_url`                  | none                                                | Public cache URL for channel manifests           |
| `signing`            | `enabled`                    | `false`                                             | Sign build outputs                               |
| `signing`            | `key_file`                   | none                                                | Signing key file path                            |
| `cache_upload`       | `enabled`                    | `false`                                             | Upload builds to external cache store            |
| `cache_upload`       | `store_uri`                  | none                                                | Cache store URI (`s3://bucket/path`)             |
| `cache_upload`       | `s3.region`                  | none                                                | AWS region                                       |
| `cache_upload`       | `s3.prefix`                  | none                                                | Extra path prefix within bucket                  |
| `cache_upload`       | `s3.access_key_id`           | none                                                | Access key for presigned S3 uploads/redirects    |
| `cache_upload`       | `s3.secret_access_key`       | none                                                | Secret key for presigned S3 uploads/redirects    |
| `cache_upload`       | `s3.session_token`           | none                                                | Session token for temporary credentials          |
| `cache_upload`       | `s3.endpoint_url`            | none                                                | S3-compatible endpoint URL                       |
| `cache_upload`       | `s3.use_path_style`          | `false`                                             | Use path-style addressing                        |
| `cache_upload`       | `upload_concurrency`         | `4`                                                 | Concurrent uploads per build                     |
| `cache_upload`       | `upload_max_retries`         | `3`                                                 | Max retry attempts per path                      |
| `cache_upload`       | `fail_build_on_upload_error` | `false`                                             | Mark build failed on upload error                |
| `cache_upload`       | `compression`                | `zstd`                                              | Agent presigned-upload NAR compression           |
| `notifications`      | `webhook_url`                | none                                                | HTTP endpoint for build status JSON              |
| `notifications`      | `github_token`               | none                                                | GitHub token for commit status updates           |
| `notifications`      | `gitea_url`                  | none                                                | Gitea/Forgejo instance URL                       |
| `notifications`      | `gitea_token`                | none                                                | Gitea/Forgejo API token                          |
| `notifications`      | `gitlab_url`                 | none                                                | GitLab instance URL                              |
| `notifications`      | `gitlab_token`               | none                                                | GitLab API token                                 |
| `notifications`      | `enable_retry_queue`         | `true`                                              | Persistent retry queue with backoff              |
| `notifications`      | `max_retry_attempts`         | `5`                                                 | Max notification retry attempts                  |
| `notifications`      | `retention_days`             | `7`                                                 | Retention for completed notification tasks       |
| `notifications`      | `retry_poll_interval`        | `5`                                                 | Retry poll interval (seconds)                    |
| `notifications`      | `email.smtp_host`            | none                                                | SMTP host for email notifications                |
| `notifications`      | `email.smtp_port`            | none                                                | SMTP port                                        |
| `notifications`      | `email.smtp_user`            | none                                                | SMTP username (optional)                         |
| `notifications`      | `email.smtp_password`        | none                                                | SMTP password (optional)                         |
| `notifications`      | `email.tls`                  | `false`                                             | Enable TLS for SMTP connection                   |
| `notifications`      | `email.from_address`         | none                                                | From address for notification emails             |
| `notifications`      | `email.to_addresses`         | `[]`                                                | Recipient addresses                              |
| `notifications`      | `slack.webhook_url`          | none                                                | Slack incoming webhook URL                       |
| `notifications`      | `slack.on_failure_only`      | `false`                                             | Only send Slack alerts on failure                |
| `notifications`      | `alerts.enabled`             | `false`                                             | Enable error-rate threshold alerts               |
| `notifications`      | `alerts.error_threshold`     | `0.5`                                               | Error rate threshold to trigger alert            |
| `notifications`      | `alerts.time_window_minutes` | `60`                                                | Time window for error rate calculation           |
| `tracing`            | `level`                      | `info`                                              | Log level (trace/debug/info/warn/error)          |
| `tracing`            | `format`                     | `compact`                                           | Log output format                                |
| `tracing`            | `show_targets`               | `true`                                              | Show module path in log messages                 |
| `tracing`            | `show_timestamps`            | `true`                                              | Show timestamps in log messages                  |
| `oauth`              | `github.client_id`           | none                                                | GitHub OAuth App client ID                       |
| `oauth`              | `github.client_secret`       | none                                                | GitHub OAuth App client secret                   |
| `oauth`              | `github.redirect_uri`        | none                                                | OAuth redirect URI                               |
| `declarative`        | `projects`                   | `[]`                                                | Declarative project definitions                  |
| `declarative`        | `api_keys`                   | `[]`                                                | Declarative API key definitions                  |
| `declarative`        | `users`                      | `[]`                                                | Declarative user definitions                     |
| `declarative`        | `remote_builders`            | `[]`                                                | Declarative remote builder definitions           |
| `nix`                | `store_dir`                  | `/nix/store`                                        | Nix store directory                              |

<!-- markdownlint-enable MD013 -->

## Binary Cache Storage

Set `[cache].enabled = true` on the server to expose `/nix-cache/`. For outputs
present in the server's Nix store, Circus generates narinfo from `nix path-info`
and streams NARs with the configured `[cache].compression`.

Only two kinds of local store paths are served: build outputs the queue-runner
signed at build time (`[signing]` with a `key_file` — unsigned outputs are never
exposed), and content-addressed paths such as drvs and sources, which Nix
verifies against their `CA:` field and which agents substitute when starting
dispatched builds. Without a signing key the cache serves nothing beyond drv
closures.

Set `[cache_upload].enabled = true` and `store_uri = "s3://bucket[/prefix]"` to
push completed outputs to S3. SSH/local runner builds use `nix copy --to`; agent
builds use presigned PUT URLs and persist narinfo rows in the database. When a
client later requests those NARs through `/nix-cache/nar/...`, the server signs
a short-lived S3 GET URL and redirects the client.

If both `store_uri` and `s3.prefix` contain paths, Circus combines them. For
example, `store_uri = "s3://bucket/root"` plus `s3.prefix = "nix-cache"` writes
objects below `root/nix-cache/`.

## Dashboard Page Access

Dashboard pages are public by default, but operators can require login or admin
access per page through `[server.page_access]`. Valid values are `public`,
`authenticated`, and `admin`.

```toml
[server.page_access]
home        = "public"
projects    = "public"
project     = "public"
jobset      = "public"
jobset_jobs = "public"
evaluations = "authenticated"
evaluation  = "authenticated"
builds      = "authenticated"
build       = "authenticated"
queue       = "admin"
channels    = "public"
channel     = "public"
news        = "public"
starred     = "authenticated"
metrics     = "admin"
```

Admin-only pages such as `/admin`, `/users`, and project notification settings
remain admin-only regardless of this policy. API endpoint authorization is
separate from dashboard page visibility.

> [!NOTE]
> Set `server.config_editor_enabled = false` to make the admin config editor
> read-only and reject `PUT /api/v1/admin/config`. The `GET` endpoint still
> returns the default-backed effective config for inspection.

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
$ circus-migrate -- up <database_url>

# Validate schema
$ circus-migrate -- validate <database_url>

# Create a new migration file
$ circus-migrate -- create <name>
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
            migratePackage = circus.packages.x86_64-linux.circus-migrate-cli;

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
{ inputs, config, pkgs,  ... }: let
  circusPkgs = circus.packages.${pkgs.stdenv.hostPlatform.system}.packages
in {
  networking.firewall.allowedTCPPorts = [ 80 443 ];
  services.circus = {
    enable = true;
    package = circusPkgs.circus-server;
    migratePackage = circusPkgs.circus-migrate-cli;

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

## Distributed Builders

Circus supports SSH remote builders and persistent `circus-agent` builders.
Detailed usage and operations are covered in [USAGE.md](./USAGE.md); protocol
details are covered in [DISTRIBUTED.md](./DISTRIBUTED.md).

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
> Subsequent keys can be created with `circus-admin api-keys create` or the
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
status is available through `circus-admin status` or `/api/v1/admin/system`.

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
