{
  pkgs,
  self,
}:
pkgs.testers.nixosTest {
  name = "circus-channel-tarball";

  nodes.machine = {
    imports = [
      self.nixosModules.circus
      ../vm-common.nix
    ];
    _module.args.self = self;
  };

  testScript = ''
    import hashlib
    import json

    machine.start()
    machine.wait_for_unit("postgresql.service")
    machine.wait_until_succeeds("sudo -u circus psql -U circus -d circus -c 'SELECT 1'", timeout=30)
    machine.wait_for_unit("circus-server.service")
    machine.wait_until_succeeds("curl -sf http://127.0.0.1:3000/health", timeout=30)

    api_token = "circus_testkey123"
    api_hash = hashlib.sha256(api_token.encode()).hexdigest()
    machine.succeed(
        f"sudo -u circus psql -U circus -d circus -c \"INSERT INTO api_keys (name, key_hash, role) VALUES ('test', '{api_hash}', 'admin')\""
    )
    auth_header = f"-H 'Authorization: Bearer {api_token}'"

    # Create project
    project_id = machine.succeed(
        "curl -sf -X POST http://127.0.0.1:3000/api/v1/projects "
        f"{auth_header} "
        "-H 'Content-Type: application/json' "
        "-d '{\"name\": \"tarball-test\", \"repository_url\": \"https://github.com/test/tarball\"}' "
        "| jq -r .id"
    ).strip()

    # Create jobset
    jobset_id = machine.succeed(
        f"curl -sf -X POST 'http://127.0.0.1:3000/api/v1/projects/{project_id}/jobsets' "
        f"{auth_header} "
        "-H 'Content-Type: application/json' "
        "-d '{\"name\": \"packages\", \"nix_expression\": \"packages\"}' "
        "| jq -r .id"
    ).strip()

    # Create evaluation via SQL
    eval_id = machine.succeed(
        "sudo -u circus psql -U circus -d circus -tA -c "
        "\"INSERT INTO evaluations (jobset_id, commit_hash, status) "
        f"VALUES ('{jobset_id}', 'abc123', 'completed') RETURNING id\" | head -1"
    ).strip()

    # Create succeeded builds with output paths
    machine.succeed(
        "sudo -u circus psql -U circus -d circus -c "
        "\"INSERT INTO builds (evaluation_id, job_name, drv_path, status, system, build_output_path) "
        f"VALUES ('{eval_id}', 'hello', '/nix/store/fake-hello.drv', 'succeeded', 'x86_64-linux', '/nix/store/aaaa-hello-1.0')\""
    )
    machine.succeed(
        "sudo -u circus psql -U circus -d circus -c "
        "\"INSERT INTO builds (evaluation_id, job_name, drv_path, status, system, build_output_path) "
        f"VALUES ('{eval_id}', 'world', '/nix/store/fake-world.drv', 'succeeded', 'x86_64-linux', '/nix/store/bbbb-world-2.0')\""
    )
    # A failed build should not appear in the tarball
    machine.succeed(
        "sudo -u circus psql -U circus -d circus -c "
        "\"INSERT INTO builds (evaluation_id, job_name, drv_path, status, system) "
        f"VALUES ('{eval_id}', 'broken', '/nix/store/fake-broken.drv', 'failed', 'x86_64-linux')\""
    )

    # Create channel
    channel_id = machine.succeed(
        "curl -sf -X POST http://127.0.0.1:3000/api/v1/channels "
        f"{auth_header} "
        "-H 'Content-Type: application/json' "
        f"-d '{{\"project_id\": \"{project_id}\", \"name\": \"nixos-unstable\", \"jobset_id\": \"{jobset_id}\"}}' "
        "| jq -r .id"
    ).strip()

    with subtest("Channel without evaluation returns 404 for tarball"):
        # The channel auto-promotes on create if eval exists, so check if it already has one
        ch = json.loads(machine.succeed(
            f"curl -sf http://127.0.0.1:3000/api/v1/channels/{channel_id}"
        ))
        if ch["current_evaluation_id"] is None:
            code = machine.succeed(
                f"curl -s -o /dev/null -w '%{{http_code}}' "
                f"http://127.0.0.1:3000/api/v1/channels/{channel_id}/nixexprs.tar.xz"
            )
            assert code.strip() == "404", f"Expected 404 for no-eval channel, got {code.strip()}"

    # Promote evaluation to channel
    machine.succeed(
        f"curl -sf -X POST http://127.0.0.1:3000/api/v1/channels/{channel_id}/promote/{eval_id} "
        f"{auth_header}"
    )

    with subtest("Channel has current_evaluation_id after promotion"):
        result = machine.succeed(
            f"curl -sf http://127.0.0.1:3000/api/v1/channels/{channel_id}"
        )
        ch = json.loads(result)
        assert ch["current_evaluation_id"] == eval_id, \
            f"Expected current_evaluation_id={eval_id}, got {ch['current_evaluation_id']}"

    with subtest("nixexprs.tar.xz returns 200 with correct content-type"):
        headers = machine.succeed(
            "curl -sf -D - -o /tmp/nixexprs.tar.xz "
            f"http://127.0.0.1:3000/api/v1/channels/{channel_id}/nixexprs.tar.xz"
        )
        assert "application/x-xz" in headers, \
            f"Expected application/x-xz content-type, got: {headers}"

    with subtest("Tarball is valid xz and contains default.nix"):
        listing = machine.succeed("xz -d < /tmp/nixexprs.tar.xz | tar tf -")
        assert "default.nix" in listing, \
            f"Expected default.nix in tarball, got: {listing}"

    with subtest("default.nix contains succeeded builds"):
        machine.succeed("xz -d < /tmp/nixexprs.tar.xz | tar xf - -C /tmp")
        content = machine.succeed("cat /tmp/default.nix")
        assert "hello" in content, "Expected 'hello' job in default.nix"
        assert "world" in content, "Expected 'world' job in default.nix"
        assert "/nix/store/aaaa-hello-1.0" in content, \
            "Expected hello output path in default.nix"
        assert "/nix/store/bbbb-world-2.0" in content, \
            "Expected world output path in default.nix"

    with subtest("default.nix excludes failed builds"):
        content = machine.succeed("cat /tmp/default.nix")
        assert "broken" not in content, \
            "Failed build 'broken' should not appear in default.nix"

    with subtest("default.nix has mkFakeDerivation structure"):
        content = machine.succeed("cat /tmp/default.nix")
        assert "mkFakeDerivation" in content, \
            "Expected mkFakeDerivation helper in default.nix"
        assert "builtin:fetchurl" in content, \
            "Expected builtin:fetchurl in mkFakeDerivation"

    with subtest("Nonexistent channel returns 404 for tarball"):
        code = machine.succeed(
            "curl -s -o /dev/null -w '%{http_code}' "
            "http://127.0.0.1:3000/api/v1/channels/00000000-0000-0000-0000-000000000000/nixexprs.tar.xz"
        )
        assert code.strip() == "404", f"Expected 404 for nonexistent channel, got {code.strip()}"

    # Cleanup
    machine.succeed(
        f"curl -sf -X DELETE http://127.0.0.1:3000/api/v1/projects/{project_id} {auth_header}"
    )
  '';
}
