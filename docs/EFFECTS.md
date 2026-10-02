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

## State Files

`getStateFile` and `putStateFile` from hercules-ci-effects work as on Hercules.
Each attempt gets a token under the `hercules-ci` secrets key for
`$HERCULES_CI_API_BASE_URL/api/v1/current-task/state/<name>/data`, valid only
while it runs. Files are stored per project, last write wins, up to
`server.max_body_size`.

## Status

Effects appear in an Effects panel on the evaluation page, on their own detail
page with live logs, through the `kind` filter of the builds API, and in the
usual notifications.
