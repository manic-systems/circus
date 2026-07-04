{
  pkgs,
  self,
}:
pkgs.testers.runNixOSTest ({config, ...}: let
  inherit (config.node.pkgs.stdenv.hostPlatform) system;
in {
  name = "circus-s3-cache-upload";

  nodes.machine = {pkgs, ...}: {
    imports = [
      self.nixosModules.circus
      ../common/vm.nix
    ];
    _module.args.self = self;

    environment.systemPackages = with pkgs; [garage_2 minio-client];

    # Add Garage for S3-compatible storage
    services.garage = {
      enable = true;
      package = pkgs.garage_2;
      settings = {
        rpc_bind_addr = "127.0.0.1:3901";
        rpc_secret = "0000000000000000000000000000000000000000000000000000000000000000";
        replication_factor = 1;
        s3_api = {
          api_bind_addr = "127.0.0.1:3900";
          s3_region = "garage";
        };
        s3_web = {
          bind_addr = "127.0.0.1:3902";
          root_domain = "web.garage.test";
        };
      };
    };

    # Configure circus to upload to the local Garage instance
    services.circus = {
      settings = {
        cache_upload = {
          enabled = true;
          store_uri = "s3://circus-cache?region=garage&endpoint=http://127.0.0.1:3900";
          s3 = {
            region = "garage";
            access_key_id = "GKcircus";
            secret_access_key = "0000000000000000";
            endpoint_url = "http://127.0.0.1:3900";
            use_path_style = true;
          };
        };
      };
    };

    systemd.services.circus-queue-runner.environment = {
      AWS_ACCESS_KEY_ID = "GKcircus";
      AWS_SECRET_ACCESS_KEY = "0000000000000000";
    };
  };

  testScript = ''
    import hashlib
    import time

    machine.start()

    # Wait for PostgreSQL
    machine.wait_for_unit("postgresql.service")
    machine.wait_until_succeeds("setpriv --reuid=circus --regid=circus --init-groups psql -U circus -d circus -c 'SELECT 1'", timeout=30)

    # Wait for Garage to be ready
    machine.wait_for_unit("garage.service")
    machine.wait_for_open_port(3901)

    # Configure MinIO client and create bucket
    machine.succeed("garage layout assign -z test -c 1G $(garage node id | cut -d@ -f1)")
    machine.succeed("garage layout apply --version 1")
    machine.succeed("garage bucket create circus-cache")
    machine.succeed("garage key import --yes -n circus-key GKcircus 0000000000000000")
    machine.succeed("garage bucket allow circus-cache --key circus-key --read --write")
    machine.succeed("garage bucket website circus-cache --allow")
    machine.succeed("mc alias set local http://127.0.0.1:3900 GKcircus 0000000000000000")
    machine.succeed("echo StoreDir: /nix/store > nix-cache-info && mc cp nix-cache-info local/circus-cache/nix-cache-info")

    machine.wait_for_unit("circus-server.service")
    machine.wait_until_succeeds("curl -sf http://127.0.0.1:3000/health", timeout=30)

    # Seed an API key for write operations
    api_token = "circus_testkey123"
    api_hash = hashlib.sha256(api_token.encode()).hexdigest()
    machine.succeed(
        f"setpriv --reuid=circus --regid=circus --init-groups psql -U circus -d circus -c \"INSERT INTO api_keys (name, key_hash, role) VALUES ('test', '{api_hash}', 'admin')\""
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
            echo > /tmp/s3-test-flake/flake.nix '
            {
              description = "circus S3 cache test flake";
              outputs = { self, ... }: {
                packages.${system}.s3-test = derivation {
                  name = "circus-s3-test";
                  system = "${system}";
                  builder = "/bin/sh";
                  args = [ "-c" "echo s3-cache-test-content > $out" ];
                };
              };
            }
            '
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
            f"curl -sf http://127.0.0.1:3000/api/v1/builds/{build_id} | jq -e 'select(.status==\"succeeded\")'",
            timeout=120
        )

    # Verify build completed successfully
    with subtest("Build completed successfully"):
        result = machine.succeed(
            f"curl -sf http://127.0.0.1:3000/api/v1/builds/{build_id} | jq -r .status"
        ).strip()
        assert result == "succeeded", f"Expected succeeded status, got '{result}'"

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
        bucket_contents = machine.succeed("mc ls --recursive local/circus-cache/")

        # Should have the .narinfo file and the .nar file
        assert ".narinfo" in bucket_contents, f"Expected .narinfo file in bucket, got: {bucket_contents}"
        assert ".nar" in bucket_contents, f"Expected .nar file in bucket, got: {bucket_contents}"

    # Verify we can download the narinfo from the S3 bucket
    with subtest("Can download narinfo from S3 bucket"):
        # Get the store hash from the output path
        store_hash = output_path.split('/')[3].split('-')[0]

        # Try to get the narinfo from S3
        narinfo_content = machine.succeed(
            f"curl -sf http://127.0.0.1:3902/{store_hash}.narinfo -H 'Host: circus-cache.web.garage.test'"
        )
        assert "StorePath:" in narinfo_content, f"Expected StorePath in narinfo: {narinfo_content}"
        assert "NarHash:" in narinfo_content, f"Expected NarHash in narinfo: {narinfo_content}"
  '';
})
