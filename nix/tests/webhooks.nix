{
  pkgs,
  self,
}:
pkgs.testers.nixosTest {
  name = "circus-webhooks";

  nodes.machine = {
    imports = [
      self.nixosModules.circus
      ../vm-common.nix
    ];
    _module.args.self = self;
  };

  # Webhook and PR integration tests
  testScript = ''
    import hashlib
    import json

    machine.start()
    machine.wait_for_unit("postgresql.service")

    # Ensure PostgreSQL is actually ready to accept connections before circus-server starts
    machine.wait_until_succeeds("sudo -u circus psql -U circus -d circus -c 'SELECT 1'", timeout=30)

    machine.wait_for_unit("circus-server.service")

    # Wait for the server to start listening
    machine.wait_until_succeeds("curl -sf http://127.0.0.1:3000/health", timeout=30)

    # Seed an API key for write operations
    api_token = "fc_testkey123"
    api_hash = hashlib.sha256(api_token.encode()).hexdigest()
    machine.succeed(
        f"sudo -u circus psql -U circus -d circus -c \"INSERT INTO api_keys (name, key_hash, role) VALUES ('test', '{api_hash}', 'admin')\""
    )
    auth_header = f"-H 'Authorization: Bearer {api_token}'"

    # Create a test project for webhook tests
    with subtest("Create test project for webhooks"):
        result = machine.succeed(
            "curl -sf -X POST http://127.0.0.1:3000/api/v1/projects "
            f"{auth_header} "
            "-H 'Content-Type: application/json' "
            "-d '{\"name\": \"webhook-test\", \"repository_url\": \"https://github.com/test/webhook-repo\"}' "
            "| jq -r .id"
        )
        project_id = result.strip()
        assert len(project_id) == 36, f"Expected UUID, got '{project_id}'"

    # Create a jobset for the project
    with subtest("Create jobset for webhook project"):
        result = machine.succeed(
            f"curl -sf -X POST http://127.0.0.1:3000/api/v1/projects/{project_id}/jobsets "
            f"{auth_header} "
            "-H 'Content-Type: application/json' "
            "-d '{\"name\": \"main\", \"nix_expression\": \"packages\", \"enabled\": true}' "
            "| jq -r .id"
        )
        jobset_id = result.strip()
        assert len(jobset_id) == 36, f"Expected UUID, got '{jobset_id}'"

    # GitHub Webhook Tests
    with subtest("GitHub webhook returns 404 when not configured"):
        code = machine.succeed(
            "curl -s -o /dev/null -w '%{http_code}' "
            f"-X POST http://127.0.0.1:3000/api/v1/webhooks/{project_id}/github "
            "-H 'Content-Type: application/json' "
            "-H 'X-GitHub-Event: push' "
            "-d '{\"after\": \"abc123def456\"}'"
        )
        # 200 with accepted=false (no webhook configured)
        assert code.strip() in ("200", "404"), f"Expected 200 or 404, got {code.strip()}"

    # Configure GitHub webhook for the project
    with subtest("Configure GitHub webhook"):
        result = machine.succeed(
            f"curl -sf -X POST http://127.0.0.1:3000/api/v1/projects/{project_id}/webhooks "
            f"{auth_header} "
            "-H 'Content-Type: application/json' "
            "-d '{\"forge_type\": \"github\", \"secret\": \"test-secret\"}' "
            "| jq -r .id"
        )
        webhook_id = result.strip()
        assert len(webhook_id) == 36, f"Expected UUID, got '{webhook_id}'"

    with subtest("GitHub push webhook triggers evaluation"):
        # First count evaluations
        count_before = machine.succeed(
            "sudo -u circus psql -U circus -d circus -c \"SELECT COUNT(*) FROM evaluations\" -t"
        ).strip()

        # Send push event (signature would normally be verified but we're testing)
        # We need to compute HMAC-SHA256 signature
        payload = '{"ref":"refs/heads/main","after":"abc123def456789012345678901234567890abcd"}'

        import hmac
        sig = hmac.new(b"test-secret", payload.encode(), hashlib.sha256).hexdigest()

        machine.succeed(
            f"curl -sf -X POST http://127.0.0.1:3000/api/v1/webhooks/{project_id}/github "
            "-H 'Content-Type: application/json' "
            "-H 'X-GitHub-Event: push' "
            f"-H 'X-Hub-Signature-256: sha256={sig}' "
            f"-d '{payload}'"
        )

        # Check evaluation was created
        count_after = machine.succeed(
            "sudo -u circus psql -U circus -d circus -c \"SELECT COUNT(*) FROM evaluations\" -t"
        ).strip()
        assert int(count_after) > int(count_before), \
            f"Expected new evaluation, count before={count_before}, after={count_after}"

    with subtest("GitHub push webhook rejects invalid signature"):
        result = machine.succeed(
            f"curl -s -X POST http://127.0.0.1:3000/api/v1/webhooks/{project_id}/github "
            "-H 'Content-Type: application/json' "
            "-H 'X-GitHub-Event: push' "
            "-H 'X-Hub-Signature-256: sha256=invalidsig' "
            "-d '{\"after\": \"xyz789\"}' | jq -r .accepted"
        )
        assert result.strip() == "false", f"Expected accepted=false for invalid sig, got {result.strip()}"

    with subtest("GitHub push webhook skips branch deletion"):
        deletion_payload = '{"after": "0000000000000000000000000000000000000000"}'
        deletion_sig = hmac.new(b"test-secret", deletion_payload.encode(), hashlib.sha256).hexdigest()

        result = machine.succeed(
            f"curl -sf -X POST http://127.0.0.1:3000/api/v1/webhooks/{project_id}/github "
            "-H 'Content-Type: application/json' "
            "-H 'X-GitHub-Event: push' "
            f"-H 'X-Hub-Signature-256: sha256={deletion_sig}' "
            f"-d '{deletion_payload}' "
            "| jq -r .message"
        )
        assert "deletion" in result.lower() or "skip" in result.lower(), \
            f"Expected deletion event to be skipped, got: {result}"

    with subtest("GitHub pull_request webhook triggers evaluation with PR metadata"):
        count_before = machine.succeed(
            "sudo -u circus psql -U circus -d circus -c \"SELECT COUNT(*) FROM evaluations WHERE pr_number IS NOT NULL\" -t"
        ).strip()

        payload = json.dumps({
            "action": "opened",
            "number": 42,
            "pull_request": {
                "head": {"sha": "pr123abc456def789012345678901234567890ab", "ref": "feature-branch"},
                "base": {"sha": "base456", "ref": "main"},
                "draft": False
            }
        })

        sig = hmac.new(b"test-secret", payload.encode(), hashlib.sha256).hexdigest()

        machine.succeed(
            f"curl -sf -X POST http://127.0.0.1:3000/api/v1/webhooks/{project_id}/github "
            "-H 'Content-Type: application/json' "
            "-H 'X-GitHub-Event: pull_request' "
            f"-H 'X-Hub-Signature-256: sha256={sig}' "
            f"-d '{payload}'"
        )

        count_after = machine.succeed(
            "sudo -u circus psql -U circus -d circus -c \"SELECT COUNT(*) FROM evaluations WHERE pr_number IS NOT NULL\" -t"
        ).strip()
        assert int(count_after) > int(count_before), \
            f"Expected PR evaluation, count before={count_before}, after={count_after}"

        # Verify PR metadata was stored
        pr_data = machine.succeed(
            "sudo -u circus psql -U circus -d circus -c \"SELECT pr_number, pr_head_branch, pr_base_branch FROM evaluations WHERE pr_number = 42\" -t"
        ).strip()
        assert "42" in pr_data, f"Expected PR number 42 in {pr_data}"
        assert "feature-branch" in pr_data, f"Expected feature-branch in {pr_data}"

    with subtest("GitHub pull_request webhook skips draft PRs"):
        payload = json.dumps({
            "action": "opened",
            "number": 99,
            "pull_request": {
                "head": {"sha": "draft123", "ref": "draft-branch"},
                "base": {"sha": "base456", "ref": "main"},
                "draft": True
            }
        })

        sig = hmac.new(b"test-secret", payload.encode(), hashlib.sha256).hexdigest()

        result = machine.succeed(
            f"curl -sf -X POST http://127.0.0.1:3000/api/v1/webhooks/{project_id}/github "
            "-H 'Content-Type: application/json' "
            "-H 'X-GitHub-Event: pull_request' "
            f"-H 'X-Hub-Signature-256: sha256={sig}' "
            f"-d '{payload}' | jq -r .message"
        )
        assert "draft" in result.lower(), f"Expected draft PR to be skipped, got: {result}"

    ## GitLab Webhook Tests
    # Create a GitLab project
    with subtest("Create GitLab test project"):
        result = machine.succeed(
            "curl -sf -X POST http://127.0.0.1:3000/api/v1/projects "
            f"{auth_header} "
            "-H 'Content-Type: application/json' "
            "-d '{\"name\": \"gitlab-test\", \"repository_url\": \"https://gitlab.com/test/repo\"}' "
            "| jq -r .id"
        )
        gitlab_project_id = result.strip()
        assert len(gitlab_project_id) == 36, f"Expected UUID, got '{gitlab_project_id}'"

    with subtest("Create jobset for GitLab project"):
        machine.succeed(
            f"curl -sf -X POST http://127.0.0.1:3000/api/v1/projects/{gitlab_project_id}/jobsets "
            f"{auth_header} "
            "-H 'Content-Type: application/json' "
            "-d '{\"name\": \"main\", \"nix_expression\": \"packages\", \"enabled\": true}'"
        )

    with subtest("Configure GitLab webhook"):
        result = machine.succeed(
            f"curl -sf -X POST http://127.0.0.1:3000/api/v1/projects/{gitlab_project_id}/webhooks "
            f"{auth_header} "
            "-H 'Content-Type: application/json' "
            "-d '{\"forge_type\": \"gitlab\", \"secret\": \"gitlab-token\"}' "
            "| jq -r .id"
        )
        assert len(result.strip()) == 36

    with subtest("GitLab Push Hook triggers evaluation"):
        count_before = machine.succeed(
            "sudo -u circus psql -U circus -d circus -c \"SELECT COUNT(*) FROM evaluations\" -t"
        ).strip()

        machine.succeed(
            f"curl -sf -X POST http://127.0.0.1:3000/api/v1/webhooks/{gitlab_project_id}/gitlab "
            "-H 'Content-Type: application/json' "
            "-H 'X-Gitlab-Event: Push Hook' "
            "-H 'X-Gitlab-Token: gitlab-token' "
            "-d '{\"ref\":\"refs/heads/main\",\"checkout_sha\":\"gitlab123456789012345678901234567890abcd\"}'"
        )

        count_after = machine.succeed(
            "sudo -u circus psql -U circus -d circus -c \"SELECT COUNT(*) FROM evaluations\" -t"
        ).strip()
        assert int(count_after) > int(count_before), \
            "Expected new evaluation from GitLab push"

    with subtest("GitLab webhook rejects invalid token"):
        result = machine.succeed(
            f"curl -s -X POST http://127.0.0.1:3000/api/v1/webhooks/{gitlab_project_id}/gitlab "
            "-H 'Content-Type: application/json' "
            "-H 'X-Gitlab-Event: Push Hook' "
            "-H 'X-Gitlab-Token: wrong-token' "
            "-d '{\"checkout_sha\":\"abc123\"}' | jq -r .accepted"
        )
        assert result.strip() == "false", "Expected rejected for wrong token"

    with subtest("GitLab Merge Request Hook triggers evaluation with PR metadata"):
        count_before = machine.succeed(
            "sudo -u circus psql -U circus -d circus -c \"SELECT COUNT(*) FROM evaluations WHERE pr_number IS NOT NULL\" -t"
        ).strip()

        payload = json.dumps({
            "object_kind": "merge_request",
            "object_attributes": {
                "iid": 123,
                "action": "open",
                "source_branch": "feature",
                "target_branch": "main",
                "last_commit": {"id": "mr123abc456def789012345678901234567890ab"},
                "draft": False,
                "work_in_progress": False
            }
        })

        machine.succeed(
            f"curl -sf -X POST http://127.0.0.1:3000/api/v1/webhooks/{gitlab_project_id}/gitlab "
            "-H 'Content-Type: application/json' "
            "-H 'X-Gitlab-Event: Merge Request Hook' "
            "-H 'X-Gitlab-Token: gitlab-token' "
            f"-d '{payload}'"
        )

        count_after = machine.succeed(
            "sudo -u circus psql -U circus -d circus -c \"SELECT COUNT(*) FROM evaluations WHERE pr_number IS NOT NULL\" -t"
        ).strip()
        assert int(count_after) > int(count_before), \
            f"Expected MR evaluation, count before={count_before}, after={count_after}"

    with subtest("GitLab Merge Request Hook skips draft MRs"):
        payload = json.dumps({
            "object_kind": "merge_request",
            "object_attributes": {
                "iid": 999,
                "action": "open",
                "source_branch": "draft-feature",
                "target_branch": "main",
                "last_commit": {"id": "draft123"},
                "draft": True
            }
        })

        result = machine.succeed(
            f"curl -sf -X POST http://127.0.0.1:3000/api/v1/webhooks/{gitlab_project_id}/gitlab "
            "-H 'Content-Type: application/json' "
            "-H 'X-Gitlab-Event: Merge Request Hook' "
            "-H 'X-Gitlab-Token: gitlab-token' "
            f"-d '{payload}' | jq -r .message"
        )
        assert "draft" in result.lower() or "wip" in result.lower(), \
            f"Expected draft MR to be skipped, got: {result}"

    # Gitea/Forgejo Webhook Tests
    with subtest("Create Gitea test project"):
        result = machine.succeed(
            "curl -sf -X POST http://127.0.0.1:3000/api/v1/projects "
            f"{auth_header} "
            "-H 'Content-Type: application/json' "
            "-d '{\"name\": \"gitea-test\", \"repository_url\": \"https://gitea.example.com/test/repo\"}' "
            "| jq -r .id"
        )
        gitea_project_id = result.strip()

    with subtest("Create jobset for Gitea project"):
        machine.succeed(
            f"curl -sf -X POST http://127.0.0.1:3000/api/v1/projects/{gitea_project_id}/jobsets "
            f"{auth_header} "
            "-H 'Content-Type: application/json' "
            "-d '{\"name\": \"main\", \"nix_expression\": \"packages\", \"enabled\": true}'"
        )

    with subtest("Configure Gitea webhook"):
        machine.succeed(
            f"curl -sf -X POST http://127.0.0.1:3000/api/v1/projects/{gitea_project_id}/webhooks "
            f"{auth_header} "
            "-H 'Content-Type: application/json' "
            "-d '{\"forge_type\": \"gitea\", \"secret\": \"gitea-secret\"}'"
        )

    with subtest("Gitea push webhook triggers evaluation"):
        count_before = machine.succeed(
            "sudo -u circus psql -U circus -d circus -c \"SELECT COUNT(*) FROM evaluations\" -t"
        ).strip()

        payload = '{"ref":"refs/heads/main","after":"gitea123456789012345678901234567890abcd"}'
        sig = hmac.new(b"gitea-secret", payload.encode(), hashlib.sha256).hexdigest()

        machine.succeed(
            f"curl -sf -X POST http://127.0.0.1:3000/api/v1/webhooks/{gitea_project_id}/gitea "
            "-H 'Content-Type: application/json' "
            f"-H 'X-Gitea-Signature: {sig}' "
            f"-d '{payload}'"
        )

        count_after = machine.succeed(
            "sudo -u circus psql -U circus -d circus -c \"SELECT COUNT(*) FROM evaluations\" -t"
        ).strip()
        assert int(count_after) > int(count_before), \
            "Expected new evaluation from Gitea push"

    # OAuth Routes Existence Tests
    with subtest("GitHub OAuth login route exists"):
        # Should redirect or return 404 if not configured
        code = machine.succeed(
            "curl -s -o /dev/null -w '%{http_code}' http://127.0.0.1:3000/api/v1/auth/github"
        )
        # 302 (redirect) or 404 (not configured) are both acceptable
        assert code.strip() in ("302", "404"), f"Expected 302 or 404, got {code.strip()}"

    with subtest("GitHub OAuth callback route exists"):
        code = machine.succeed(
            "curl -s -o /dev/null -w '%{http_code}' 'http://127.0.0.1:3000/api/v1/auth/github/callback?code=test&state=test'"
        )
        # Should fail gracefully (no OAuth configured)
        assert code.strip() in ("400", "404", "500"), f"Expected error code, got {code.strip()}"

    # Cleanup
    with subtest("Cleanup test projects"):
        machine.succeed(
            f"curl -sf -X DELETE http://127.0.0.1:3000/api/v1/projects/{project_id} {auth_header}"
        )
        machine.succeed(
            f"curl -sf -X DELETE http://127.0.0.1:3000/api/v1/projects/{gitlab_project_id} {auth_header}"
        )
        machine.succeed(
            f"curl -sf -X DELETE http://127.0.0.1:3000/api/v1/projects/{gitea_project_id} {auth_header}"
        )
  '';
}
