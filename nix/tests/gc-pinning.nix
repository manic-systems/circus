{
  self,
  pkgs,
  lib,
}: let
  inherit (lib.modules) mkForce;
in
  pkgs.testers.nixosTest {
    name = "circus-gc-pinning";

    nodes.machine = {
      imports = [
        self.nixosModules.circus
        ../vm-common.nix
      ];
      _module.args.self = self;

      services.circus.settings.gc = {
        enabled = mkForce true;
        gc_roots_dir = "/var/lib/circus/gc-roots";
        cleanup_interval = 9999;
        max_age_days = 1;
      };
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

      ro_token = "circus_readonly_key"
      ro_hash = hashlib.sha256(ro_token.encode()).hexdigest()
      machine.succeed(
          f"sudo -u circus psql -U circus -d circus -c \"INSERT INTO api_keys (name, key_hash, role) VALUES ('readonly', '{ro_hash}', 'read-only')\""
      )
      ro_header = f"-H 'Authorization: Bearer {ro_token}'"

      # Create project
      project_id = machine.succeed(
          "curl -sf -X POST http://127.0.0.1:3000/api/v1/projects "
          f"{auth_header} "
          "-H 'Content-Type: application/json' "
          "-d '{\"name\": \"gc-pin-test\", \"repository_url\": \"https://github.com/test/gc\"}' "
          "| jq -r .id"
      ).strip()

      with subtest("Jobset has default keep_nr of 3"):
          result = machine.succeed(
              f"curl -sf -X POST 'http://127.0.0.1:3000/api/v1/projects/{project_id}/jobsets' "
              f"{auth_header} "
              "-H 'Content-Type: application/json' "
              "-d '{\"name\": \"default\", \"nix_expression\": \"packages\"}' "
              "| jq -r .keep_nr"
          )
          assert result.strip() == "3", f"Expected default keep_nr=3, got {result.strip()}"

      with subtest("keep_nr persists in database"):
          machine.succeed(
              "sudo -u circus psql -U circus -d circus -c "
              "\"UPDATE jobsets SET keep_nr = 7 WHERE name = 'default'\""
          )
          result = machine.succeed(
              "sudo -u circus psql -U circus -d circus -tA -c "
              "\"SELECT keep_nr FROM jobsets WHERE name = 'default'\""
          )
          assert result.strip() == "7", f"Expected keep_nr=7, got {result.strip()}"

      with subtest("keep_nr visible in active_jobsets view"):
          result = machine.succeed(
              "sudo -u circus psql -U circus -d circus -tA -c "
              "\"SELECT keep_nr FROM active_jobsets WHERE name = 'default' LIMIT 1\""
          )
          assert result.strip() == "7", f"Expected keep_nr=7 in view, got {result.strip()}"

      # Create evaluation + build for keep flag tests
      jobset_id = machine.succeed(
          "sudo -u circus psql -U circus -d circus -tA -c "
          f"\"SELECT id FROM jobsets WHERE project_id = '{project_id}' AND name = 'default'\""
      ).strip()

      eval_id = machine.succeed(
          "sudo -u circus psql -U circus -d circus -tA -c "
          f"\"INSERT INTO evaluations (jobset_id, commit_hash, status) VALUES ('{jobset_id}', 'abc123', 'completed') RETURNING id\" | head -1"
      ).strip()

      build_id = machine.succeed(
          "sudo -u circus psql -U circus -d circus -tA -c "
          f"\"INSERT INTO builds (evaluation_id, job_name, drv_path, status, system) "
          f"VALUES ('{eval_id}', 'hello', '/nix/store/fake.drv', 'succeeded', 'x86_64-linux') RETURNING id\" | head -1"
      ).strip()

      with subtest("Build starts with keep=false"):
          result = machine.succeed(
              f"curl -sf http://127.0.0.1:3000/api/v1/builds/{build_id} | jq -r .keep"
          )
          assert result.strip() == "false", f"Expected keep=false, got {result.strip()}"

      with subtest("PUT /builds/id/keep/true sets keep flag"):
          code = machine.succeed(
              "curl -s -o /dev/null -w '%{http_code}' "
              f"-X PUT http://127.0.0.1:3000/api/v1/builds/{build_id}/keep/true "
              f"{auth_header}"
          )
          assert code.strip() == "200", f"Expected 200, got {code.strip()}"

          result = machine.succeed(
              f"curl -sf http://127.0.0.1:3000/api/v1/builds/{build_id} | jq -r .keep"
          )
          assert result.strip() == "true", f"Expected keep=true, got {result.strip()}"

      with subtest("PUT /builds/id/keep/false clears keep flag"):
          machine.succeed(
              f"curl -sf -X PUT http://127.0.0.1:3000/api/v1/builds/{build_id}/keep/false "
              f"{auth_header}"
          )
          result = machine.succeed(
              f"curl -sf http://127.0.0.1:3000/api/v1/builds/{build_id} | jq -r .keep"
          )
          assert result.strip() == "false", f"Expected keep=false, got {result.strip()}"

      with subtest("Read-only key cannot set keep flag"):
          code = machine.succeed(
              "curl -s -o /dev/null -w '%{http_code}' "
              f"-X PUT http://127.0.0.1:3000/api/v1/builds/{build_id}/keep/true "
              f"{ro_header}"
          )
          assert code.strip() == "403", f"Expected 403, got {code.strip()}"

      with subtest("keep=true visible in API response"):
          machine.succeed(
              f"curl -sf -X PUT http://127.0.0.1:3000/api/v1/builds/{build_id}/keep/true "
              f"{auth_header}"
          )
          result = machine.succeed(
              f"curl -sf http://127.0.0.1:3000/api/v1/builds/{build_id}"
          )
          build_json = json.loads(result)
          assert build_json["keep"] is True, f"Expected keep=true in JSON, got {build_json.get('keep')}"

      with subtest("Nonexistent build returns 404 for keep"):
          code = machine.succeed(
              "curl -s -o /dev/null -w '%{http_code}' "
              "-X PUT http://127.0.0.1:3000/api/v1/builds/00000000-0000-0000-0000-000000000000/keep/true "
              f"{auth_header}"
          )
          assert code.strip() == "404", f"Expected 404, got {code.strip()}"

      # Cleanup
      machine.succeed(
          f"curl -sf -X DELETE http://127.0.0.1:3000/api/v1/projects/{project_id} {auth_header}"
      )
    '';
  }
