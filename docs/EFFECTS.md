# Effects

An effect is an impure task that runs after every build of an evaluation
succeeds, such as a deploy, a release upload or a branch push. Circus follows
the Hercules CI effects contract, so effects from
[hercules-ci-effects](https://github.com/hercules-ci/hercules-ci-effects) work
without a wrapper.

## Declaring an Effect

Any derivation with `isEffect = true` becomes an effect instead of a build.
`mkEffect` and `modularEffect` from hercules-ci-effects set it for you.

```nix
{
  effects.deploy-staging = effects.runNixOS {
    configuration = self.nixosConfigurations.staging;
    secretsMap.ssh = "deploy-ssh";
  };
}
```

The effect itself is never built. Its closure is built and cached first like any
other job, then an agent runs the effect's builder.

## Lifecycle

- An effect waits for every regular build of its evaluation and becomes
  `dependency_failed` if any of them fails.
- Effects of one evaluation run concurrently. Effects of different evaluations
  of the same project run one at a time.
- Only agents with effects enabled run them, never SSH builders or the runner
  host.
- Only source-change and interval evaluations of the jobset's branches and tags
  run effects. Pull request and manual evaluations never do.
- Effects are never retried automatically.

Cancelling a running effect asks the agent to abort it. When the agent cannot
confirm the outcome, the effect stays `running` with an "outcome unknown" error
and blocks newer effects of the project. After checking the target system, an
admin releases it with `POST /api/v1/builds/{id}/force-release-effect` and
`{"acknowledge_outcome_unknown":true}`. Make effects idempotent where possible.

## Runtime

The effect runs with network access and these variables, plus `CIRCUS_*`
aliases.

- `IN_HERCULES_CI_EFFECT=true`
- `HERCULES_CI_SECRETS_JSON`, the resolved secrets file
- `HERCULES_CI_API_BASE_URL`, the Circus server
- `HERCULES_CI_PROJECT_ID` and `HERCULES_CI_PROJECT_PATH`

On Linux (5.8 or newer) the effect runs in user, mount and PID namespaces as
uid 0. It sees the Nix store read-only, DNS, CA certificates, a writable
`/build`, a private `$HOME` and its secrets, and nothing else of the host.
Background processes die with it. `mkEffect`'s `fsRoot` is copied into the root,
and `/bin/sh` defaults to the builder.

Effects need a non-root agent that is not rootless. Their duration is bounded by
the build timeout options.

On darwin the effect runs as a plain process under the agent user, in a process
group that is killed when it ends. `$HOME`, `$TMPDIR` and the secrets file are
real paths, and the effect can read anything the agent user can. `mkEffect` and
`modularEffect` assume the Linux layout, so darwin agents only run plain
derivations with `isEffect = true`.

## Configuration

On the agent, point `[agent.effects]` at a Hercules-format secrets file that
only the agent user can read. With the NixOS module, set
`services.circus-agent.effectsSecretsFile` instead.

```toml
[agent.effects]
secrets_file = "/var/lib/circus-agent/secrets.json"
```

```json
{
  "deploy-ssh": {
    "kind": "Secret",
    "data": { "privateKey": "..." },
    "condition": { "and": [{ "isRepo": "infra" }, "isDefaultBranch"] }
  }
}
```

An effect's `secretsMap` names the secrets it needs. Each condition
(`isDefaultBranch`, `isTag`, `isBranch`, `isRepo`, `isOwner`, `and`, `or`) is
checked against the job, and a missing or denied secret fails the effect before
it starts. Secrets never leave the agent.

On the runner, set `queue_runner.rpc.api_base_url` to the public Circus URL and
both `queue_runner.rpc.cache_substituter` and
`queue_runner.rpc.cache_public_key`. Agents need a TLS connection to the runner
(`circus+tls://` or `[agent.tls]`). Ephemeral agents never run effects.

## GitToken Secrets

A `secretsMap` entry of `{ type = "GitToken"; }` gets `{ "token": "..." }`, an
hour-long GitHub App token that can push to the effect's own repository only.

```toml
[queue_runner.github_app]
app_id = 123456
private_key_file = "/run/secrets/circus-github-app.pem"
```

Install the app with `contents` write access on each repository whose effects
push. `api_url` selects GitHub Enterprise. Without the app, or on other forges,
an effect that asks for a token fails before it starts.

## State Files

`getStateFile` and `putStateFile` from hercules-ci-effects work as on Hercules.
Each attempt gets a token under the `hercules-ci` secrets key for
`$HERCULES_CI_API_BASE_URL/api/v1/current-task/state/<name>/data`, valid only
while it runs. Files are stored per project, last write wins, up to
`server.max_body_size`.

## Running an Effect Locally

`circus-agent effect run` builds an effect's inputs and runs it in the agent's
sandbox, like `hci effect run`.

```sh
circus-agent effect run .#effects.deploy \
  --secrets-file ./secrets.json \
  --owner acme --repo infra \
  --pretend-branch main --default-branch
```

`--api-url` and `--token` give it a server for state files.

## herculesCI

A flake jobset with `nix_expression = "herculesCI"` evaluates the flake's
`herculesCI` attribute like Hercules. A function form gets `ref`, `branch`,
`tag`, `rev`, `shortRev` and `primaryRepo`. Each `onPush.<job>.outputs` becomes
jobs prefixed with `<job>.`, and without `onPush` the default job covers
`packages`, `checks`, `devShells` and `effects`, limited to `ciSystems`.

`onSchedule.<name>` jobs are read from the default branch and fire in UTC per
`when` (`minute`, `hour`, `dayOfWeek`, `dayOfMonth`). Unset fields get a fixed
per-schedule value, so `when = {}` runs daily. Each run evaluates
`onSchedule.<name>.outputs` at that default-branch commit.

## Status

Effects appear in an Effects panel on the evaluation page, on their own detail
page with live logs, through the `kind` filter of the builds API, and in the
usual notifications.
