{
  pkgs,
  self,
}:
pkgs.testers.nixosTest {
  name = "circus-s3-cache-upload";

  nodes.machine = {
    imports = [
      self.nixosModules.circus
      ../vm-common.nix
    ];
    _module.args.self = self;

    # Add MinIO for S3-compatible storage
    services.minio = {
      enable = true;
      listenAddress = "127.0.0.1:9000";
      rootCredentialsFile = pkgs.writeText "minio-root-credentials" ''
        MINIO_ROOT_USER=minioadmin
        MINIO_ROOT_PASSWORD=minioadmin
      '';
    };

    # Configure circus to upload to the local MinIO instance
    services.circus = {
      settings = {
        cache_upload = {
          enabled = true;
          store_uri = "s3://circus-cache?endpoint=http://127.0.0.1:9000&region=us-east-1";
          s3 = {
            region = "us-east-1";
            access_key_id = "minioadmin";
            secret_access_key = "minioadmin";
            endpoint_url = "http://127.0.0.1:9000";
            use_path_style = true;
          };
        };
      };
    };
  };

  testScript = ''
    import hashlib
    import json
    import time

    machine.start()

    # Wait for PostgreSQL
    machine.wait_for_unit("postgresql.service")
    machine.wait_until_succeeds("sudo -u circus psql -U circus -d circus -c 'SELECT 1'", timeout=30)

    # Wait for MinIO to be ready
    machine.wait_for_unit("minio.service")
    machine.wait_until_succeeds("curl -sf http://127.0.0.1:9000/minio/health/live", timeout=30)

    # Configure MinIO client and create bucket
    machine.succeed("${pkgs.minio-client}/bin/mc alias set local http://127.0.0.1:9000 minioadmin minioadmin")
    machine.succeed("${pkgs.minio-client}/bin/mc mb local/circus-cache")
    machine.succeed("${pkgs.minio-client}/bin/mc policy set public local/circus-cache")

    machine.wait_for_unit("circus-server.service")
    machine.wait_until_succeeds("curl -sf http://127.0.0.1:3000/health", timeout=30)

    # Seed an API key for write operations
    api_token = "circus_testkey123"
    api_hash = hashlib.sha256(api_token.encode()).hexdigest()
    machine.succeed(
        f"sudo -u circus psql -U circus -d circus -c \"INSERT INTO api_keys (name, key_hash, role) VALUES ('test', '{api_hash}', 'admin')\""
    )
    auth_header = f"-H 'Authorization: Bearer {api_token}'"

    # Create a test flake inside the VM
    with subtest("Create bare git repo with test flake"):
        machine.succeed("mkdir -p /var/lib/circus/test-repos")
        machine.succeed("git init --bare /var/lib/circus/test-repos/s3-test-flake.git")

        # Create a working copy, write the flake, commit, push
        machine.succeed("mkdir -p /tmp/s3-test-flake")
        machine.succeed("cd /tmp/s3-test-flake && git init")
        machine.succeed("cd /tmp/s3-test-flake && git config user.email 'test@circus' && git config user.name 'circus Test'")

        # Write a minimal flake.nix that builds a simple derivation
        machine.succeed("""
            cat > /tmp/s3-test-flake/flake.nix << 'FLAKE'
            {
              description = "circus S3 cache test flake";
              outputs = { self, ... }: {
                packages.x86_64-linux.s3-test = derivation {
                  name = "circus-s3-test";
                  system = "x86_64-linux";
                  builder = "/bin/sh";
                  args = [ "-c" "echo s3-cache-test-content > $out" ];
                };
              };
            }
            FLAKE
        """)
        machine.succeed("cd /tmp/s3-test-flake && git add -A && git commit -m 'initial flake'")
        machine.succeed("cd /tmp/s3-test-flake && git remote add origin /var/lib/circus/test-repos/s3-test-flake.git")
        machine.succeed("cd /tmp/s3-test-flake && git push origin HEAD:refs/heads/master")
        machine.succeed("chown -R circus:circus /var/lib/circus/test-repos")

    # Create project + jobset
    with subtest("Create S3 test project and jobset"):
        result = machine.succeed(
            "curl -sf -X POST http://127.0.0.1:3000/api/v1/projects "
            f"{auth_header} "
            "-H 'Content-Type: application/json' "
            "-d '{\"name\": \"s3-test-project\", \"repository_url\": \"file:///var/lib/circus/test-repos/s3-test-flake.git\"}' "
            "| jq -r .id"
        )
        project_id = result.strip()
        assert len(project_id) == 36, f"Expected UUID, got '{project_id}'"

        result = machine.succeed(
            f"curl -sf -X POST http://127.0.0.1:3000/api/v1/projects/{project_id}/jobsets "
            f"{auth_header} "
            "-H 'Content-Type: application/json' "
            "-d '{\"name\": \"packages\", \"nix_expression\": \"packages\", \"flake_mode\": true, \"enabled\": true, \"check_interval\": 60}' "
            "| jq -r .id"
        )
        jobset_id = result.strip()
        assert len(jobset_id) == 36, f"Expected UUID for jobset, got '{jobset_id}'"

    # Wait for evaluator to create evaluation and builds
    with subtest("Evaluator discovers and evaluates the flake"):
        machine.wait_until_succeeds(
            f"curl -sf 'http://127.0.0.1:3000/api/v1/evaluations?jobset_id={jobset_id}' "
            "| jq -e '.items[] | select(.status==\"completed\")'",
            timeout=90
        )

    # Get the build ID
    with subtest("Get build ID for s3-test job"):
        build_id = machine.succeed(
            "curl -sf 'http://127.0.0.1:3000/api/v1/builds?job_name=s3-test' | jq -r '.items[0].id'"
        ).strip()
        assert len(build_id) == 36, f"Expected UUID for build, got '{build_id}'"

    # Wait for queue runner to build it
    with subtest("Queue runner builds pending derivation"):
        machine.wait_until_succeeds(
            f"curl -sf http://127.0.0.1:3000/api/v1/builds/{build_id} | jq -e 'select(.status==\"completed\")'",
            timeout=120
        )

    # Verify build completed successfully
    with subtest("Build completed successfully"):
        result = machine.succeed(
            f"curl -sf http://127.0.0.1:3000/api/v1/builds/{build_id} | jq -r .status"
        ).strip()
        assert result == "completed", f"Expected completed status, got '{result}'"

        output_path = machine.succeed(
            f"curl -sf http://127.0.0.1:3000/api/v1/builds/{build_id} | jq -r .build_output_path"
        ).strip()
        assert output_path.startswith("/nix/store/"), f"Expected /nix/store/ output path, got '{output_path}'"

    # Wait a bit for cache upload to complete (it's async after build)
    with subtest("Wait for cache upload to complete"):
        time.sleep(5)

    # Verify the build output was uploaded to S3
    with subtest("Build output was uploaded to S3 cache"):
        # List objects in the S3 bucket
        bucket_contents = machine.succeed("${pkgs.minio-client}/bin/mc ls --recursive local/circus-cache/")

        # Should have the .narinfo file and the .nar file
        assert ".narinfo" in bucket_contents, f"Expected .narinfo file in bucket, got: {bucket_contents}"
        assert ".nar" in bucket_contents, f"Expected .nar file in bucket, got: {bucket_contents}"

    # Verify we can download the narinfo from the S3 bucket
    with subtest("Can download narinfo from S3 bucket"):
        # Get the store hash from the output path
        store_hash = output_path.split('/')[3].split('-')[0]

        # Try to get the narinfo from S3
        narinfo_content = machine.succeed(
            f"curl -sf http://127.0.0.1:9000/circus-cache/{store_hash}.narinfo"
        )
        assert "StorePath:" in narinfo_content, f"Expected StorePath in narinfo: {narinfo_content}"
        assert "NarHash:" in narinfo_content, f"Expected NarHash in narinfo: {narinfo_content}"

    # Verify build log mentions cache upload
    with subtest("Build log mentions cache upload"):
        build_log = machine.succeed(
            f"curl -sf http://127.0.0.1:3000/api/v1/builds/{build_id}/log"
        )
        # The nix copy output should appear in the log or the system log
        # We'll check that the cache upload was attempted by looking at system logs
        journal_log = machine.succeed("journalctl -u circus-queue-runner --since '5 minutes ago' --no-pager")
        assert "Pushed to binary cache" in journal_log or "nix copy" in journal_log, \
            f"Expected cache upload in logs: {journal_log}"

    # Cleanup
    with subtest("Delete S3 test project"):
        code = machine.succeed(
            "curl -s -o /dev/null -w '%{http_code}' "
            f"-X DELETE http://127.0.0.1:3000/api/v1/projects/{project_id} "
            f"{auth_header}"
        )
        assert code.strip() == "200", f"Expected 200 for project delete, got {code.strip()}"
  '';
}
