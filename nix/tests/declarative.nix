{
  pkgs,
  lib,
  testers,
  self,
}: let
  # Password files for testing passwordFile option.
  # Passwords must be at least 12 characters with at least one uppercase letter.
  adminPasswordFile = pkgs.writeText "admin-password" "SecretAdmin123!";
  userPasswordFile = pkgs.writeText "user-password" "SecretUser123!";
  disabledPasswordFile = pkgs.writeText "disabled-password" "DisabledPass123!";
in
  testers.runNixOSTest {
    name = "circus-declarative";

    nodes.machine = {
      imports = [
        self.nixosModules.circus
        ../vm-common.nix
      ];
      _module.args.self = self;

      services.circus = {
        declarative.users = {
          # Admin user with passwordFile
          decl-admin = {
            email = "admin@test.local";
            passwordFile = toString adminPasswordFile;
            role = "admin";
          };
          # Regular user with passwordFile
          decl-user = {
            email = "user@test.local";
            passwordFile = toString userPasswordFile;
            role = "read-only";
          };
          # User with passwordFile
          decl-user2 = {
            email = "user2@test.local";
            passwordFile = toString userPasswordFile;
            role = "read-only";
          };
          # Disabled user with passwordFile
          decl-disabled = {
            email = "disabled@test.local";
            passwordFile = toString disabledPasswordFile;
            role = "read-only";
            enabled = false;
          };
        };

        # Replace vm-common's bootstrap key list entirely.
        declarative.apiKeys = lib.mkForce [
          {
            name = "decl-admin-key";
            key = "circus_decl_admin";
            role = "admin";
          }
          {
            name = "decl-readonly-key";
            key = "circus_decl_readonly";
            role = "read-only";
          }
        ];

        # Replace vm-common's placeholder project list entirely.
        declarative.projects = lib.mkForce [
          {
            name = "decl-project-1";
            repositoryUrl = "https://github.com/test/decl1";
            description = "First declarative project";
            jobsets = [
              {
                name = "enabled-jobset";
                nixExpression = "packages";
                enabled = true;
                flakeMode = true;
                checkInterval = 300;
                state = "enabled";
              }
              {
                name = "disabled-jobset";
                nixExpression = "disabled";
                state = "disabled";
              }
              {
                name = "oneshot-jobset";
                nixExpression = "oneshot";
                state = "one_shot";
              }
              {
                name = "oneatatime-jobset";
                nixExpression = "exclusive";
                state = "one_at_a_time";
                checkInterval = 60;
              }
            ];
          }
          {
            name = "decl-project-2";
            repositoryUrl = "https://github.com/test/decl2";
            jobsets = [
              {
                name = "main";
                nixExpression = ".";
                flakeMode = true;
              }
            ];
          }
          # Unlike the projects above (fake URLs that can never resolve in a
          # network-less VM), this one points at a local repo populated at
          # runtime, so the declarative path drives a real evaluation + build.
          {
            name = "decl-e2e";
            repositoryUrl = "file:///var/lib/circus/test-repos/decl-flake.git";
            description = "Declarative project that actually evaluates";
            jobsets = [
              {
                name = "packages";
                nixExpression = "packages";
                flakeMode = true;
                branch = "master";
                state = "enabled";
                checkInterval = 5;
              }
              # Same repo, disabled: the evaluator must never touch it.
              {
                name = "off";
                nixExpression = "packages";
                flakeMode = true;
                branch = "master";
                state = "disabled";
                checkInterval = 5;
              }
            ];
          }
        ];
      };
    };

    testScript = ''
      import time

      machine.start()
      machine.wait_for_unit("postgresql.service")
      machine.wait_until_succeeds("sudo -u circus psql -U circus -d circus -c 'SELECT 1'", timeout=30)
      machine.wait_for_unit("circus-server.service")
      machine.wait_until_succeeds("curl -sf http://127.0.0.1:3000/health", timeout=30)

      # DECLARATIVE USERS
      with subtest("Declarative users are created in database"):
          result = machine.succeed(
              "sudo -u circus psql -U circus -d circus -t -c \"SELECT COUNT(*) FROM users WHERE username LIKE 'decl-%'\""
          )
          count = int(result.strip())
          assert count == 4, f"Expected 4 declarative users, got {count}"

      with subtest("Declarative admin user has admin role"):
          result = machine.succeed(
              "sudo -u circus psql -U circus -d circus -t -c \"SELECT role FROM users WHERE username = 'decl-admin'\""
          )
          assert result.strip() == "admin", f"Expected admin role, got '{result.strip()}'"

      with subtest("Declarative regular users have read-only role"):
          result = machine.succeed(
              "sudo -u circus psql -U circus -d circus -t -c \"SELECT role FROM users WHERE username = 'decl-user'\""
          )
          assert result.strip() == "read-only", f"Expected read-only role, got '{result.strip()}'"

      with subtest("Declarative disabled user is disabled"):
          result = machine.succeed(
              "sudo -u circus psql -U circus -d circus -t -c \"SELECT enabled FROM users WHERE username = 'decl-disabled'\""
          )
          assert result.strip() == "f", f"Expected disabled (f), got '{result.strip()}'"

      with subtest("Declarative enabled users are enabled"):
          result = machine.succeed(
              "sudo -u circus psql -U circus -d circus -t -c \"SELECT enabled FROM users WHERE username = 'decl-admin'\""
          )
          assert result.strip() == "t", f"Expected enabled (t), got '{result.strip()}'"

      with subtest("Declarative users have password hashes set"):
          result = machine.succeed(
              "sudo -u circus psql -U circus -d circus -t -c \"SELECT password_hash FROM users WHERE username = 'decl-admin'\""
          )
          # Argon2 hashes start with $argon2
          assert result.strip().startswith("$argon2"), f"Expected argon2 hash, got '{result.strip()[:20]}...'"

      # DECLARATIVE USER WEB LOGIN
      with subtest("Web login with declarative admin user succeeds"):
          result = machine.succeed(
              "curl -s -w '\\n%{http_code}' "
              "-X POST http://127.0.0.1:3000/login "
              "-d 'username=decl-admin&password=SecretAdmin123!'"
          )
          lines = result.strip().split('\n')
          code = lines[-1]
          assert code in ("200", "302", "303"), f"Expected redirect on login, got {code}"

      with subtest("Web login with declarative user (passwordFile) succeeds"):
          result = machine.succeed(
              "curl -s -w '\\n%{http_code}' "
              "-X POST http://127.0.0.1:3000/login "
              "-d 'username=decl-user&password=SecretUser123!'"
          )
          lines = result.strip().split('\n')
          code = lines[-1]
          assert code in ("200", "302", "303"), f"Expected redirect on login, got {code}"

      with subtest("Web login with declarative user2 (passwordFile) succeeds"):
          result = machine.succeed(
              "curl -s -w '\\n%{http_code}' "
              "-X POST http://127.0.0.1:3000/login "
              "-d 'username=decl-user2&password=SecretUser123!'"
          )
          lines = result.strip().split('\n')
          code = lines[-1]
          assert code in ("200", "302", "303"), f"Expected redirect on login, got {code}"

      with subtest("Web login with wrong password fails"):
          result = machine.succeed(
              "curl -s -w '\\n%{http_code}' "
              "-X POST http://127.0.0.1:3000/login "
              "-d 'username=decl-admin&password=wrongpassword'"
          )
          lines = result.strip().split('\n')
          code = lines[-1]
          assert code in ("401",), f"Expected 401 for wrong password, got {code}"

      with subtest("Web login with disabled user fails"):
          result = machine.succeed(
              "curl -s -w '\\n%{http_code}' "
              "-X POST http://127.0.0.1:3000/login "
              "-d 'username=decl-disabled&password=DisabledPass123!'"
          )
          lines = result.strip().split('\n')
          code = lines[-1]
          assert code in ("401", "403"), f"Expected login failure for disabled user, got {code}"

      # DECLARATIVE API KEYS
      with subtest("Declarative API keys are created"):
          result = machine.succeed(
              "sudo -u circus psql -U circus -d circus -t -c \"SELECT COUNT(*) FROM api_keys WHERE name LIKE 'decl-%'\""
          )
          count = int(result.strip())
          assert count == 2, f"Expected 2 declarative API keys, got {count}"

      with subtest("Declarative admin API key works"):
          code = machine.succeed(
              "curl -s -o /dev/null -w '%{http_code}' "
              "-H 'Authorization: Bearer circus_decl_admin' "
              "http://127.0.0.1:3000/api/v1/projects"
          )
          assert code.strip() == "200", f"Expected 200, got {code.strip()}"

      with subtest("Declarative admin API key can create resources"):
          code = machine.succeed(
              "curl -s -o /dev/null -w '%{http_code}' "
              "-X POST http://127.0.0.1:3000/api/v1/projects "
              "-H 'Authorization: Bearer circus_decl_admin' "
              "-H 'Content-Type: application/json' "
              "-d '{\"name\": \"api-created\", \"repository_url\": \"https://example.com/api\"}'"
          )
          assert code.strip() == "200", f"Expected 200, got {code.strip()}"

      with subtest("Declarative read-only API key works for GET"):
          code = machine.succeed(
              "curl -s -o /dev/null -w '%{http_code}' "
              "-H 'Authorization: Bearer circus_decl_readonly' "
              "http://127.0.0.1:3000/api/v1/projects"
          )
          assert code.strip() == "200", f"Expected 200, got {code.strip()}"

      with subtest("Declarative read-only API key cannot create resources"):
          code = machine.succeed(
              "curl -s -o /dev/null -w '%{http_code}' "
              "-X POST http://127.0.0.1:3000/api/v1/projects "
              "-H 'Authorization: Bearer circus_decl_readonly' "
              "-H 'Content-Type: application/json' "
              "-d '{\"name\": \"should-fail\", \"repository_url\": \"https://example.com/fail\"}'"
          )
          assert code.strip() == "403", f"Expected 403, got {code.strip()}"

      # DECLARATIVE PROJECTS
      with subtest("Declarative projects are created"):
          result = machine.succeed(
              "curl -sf http://127.0.0.1:3000/api/v1/projects | jq '.items | map(select(.name | startswith(\"decl-project\"))) | length'"
          )
          count = int(result.strip())
          assert count == 2, f"Expected 2 declarative projects, got {count}"

      with subtest("Declarative project has correct repository URL"):
          result = machine.succeed(
              "curl -sf http://127.0.0.1:3000/api/v1/projects | jq -r '.items[] | select(.name==\"decl-project-1\") | .repository_url'"
          )
          assert result.strip() == "https://github.com/test/decl1", f"Got '{result.strip()}'"

      with subtest("Declarative project has description"):
          result = machine.succeed(
              "curl -sf http://127.0.0.1:3000/api/v1/projects | jq -r '.items[] | select(.name==\"decl-project-1\") | .description'"
          )
          assert result.strip() == "First declarative project", f"Got '{result.strip()}'"

      # DECLARATIVE JOBSETS WITH STATES
      with subtest("Declarative project has all jobsets"):
          project_id = machine.succeed(
              "curl -sf http://127.0.0.1:3000/api/v1/projects | jq -r '.items[] | select(.name==\"decl-project-1\") | .id'"
          ).strip()
          result = machine.succeed(
              f"curl -sf http://127.0.0.1:3000/api/v1/projects/{project_id}/jobsets | jq '.items | length'"
          )
          count = int(result.strip())
          assert count == 4, f"Expected 4 jobsets, got {count}"

      with subtest("Enabled jobset has state 'enabled'"):
          project_id = machine.succeed(
              "curl -sf http://127.0.0.1:3000/api/v1/projects | jq -r '.items[] | select(.name==\"decl-project-1\") | .id'"
          ).strip()
          result = machine.succeed(
              f"curl -sf http://127.0.0.1:3000/api/v1/projects/{project_id}/jobsets | jq -r '.items[] | select(.name==\"enabled-jobset\") | .state'"
          )
          assert result.strip() == "enabled", f"Expected 'enabled', got '{result.strip()}'"

      with subtest("Disabled jobset has state 'disabled'"):
          project_id = machine.succeed(
              "curl -sf http://127.0.0.1:3000/api/v1/projects | jq -r '.items[] | select(.name==\"decl-project-1\") | .id'"
          ).strip()
          result = machine.succeed(
              f"curl -sf http://127.0.0.1:3000/api/v1/projects/{project_id}/jobsets | jq -r '.items[] | select(.name==\"disabled-jobset\") | .state'"
          )
          assert result.strip() == "disabled", f"Expected 'disabled', got '{result.strip()}'"

      with subtest("One-shot jobset has state 'one_shot'"):
          project_id = machine.succeed(
              "curl -sf http://127.0.0.1:3000/api/v1/projects | jq -r '.items[] | select(.name==\"decl-project-1\") | .id'"
          ).strip()
          result = machine.succeed(
              f"curl -sf http://127.0.0.1:3000/api/v1/projects/{project_id}/jobsets | jq -r '.items[] | select(.name==\"oneshot-jobset\") | .state'"
          )
          assert result.strip() == "one_shot", f"Expected 'one_shot', got '{result.strip()}'"

      with subtest("One-at-a-time jobset has state 'one_at_a_time'"):
          project_id = machine.succeed(
              "curl -sf http://127.0.0.1:3000/api/v1/projects | jq -r '.items[] | select(.name==\"decl-project-1\") | .id'"
          ).strip()
          result = machine.succeed(
              f"curl -sf http://127.0.0.1:3000/api/v1/projects/{project_id}/jobsets | jq -r '.items[] | select(.name==\"oneatatime-jobset\") | .state'"
          )
          assert result.strip() == "one_at_a_time", f"Expected 'one_at_a_time', got '{result.strip()}'"

      with subtest("Disabled jobset is not in active_jobsets view"):
          result = machine.succeed(
              "sudo -u circus psql -U circus -d circus -t -c \"SELECT COUNT(*) FROM active_jobsets WHERE name = 'disabled-jobset'\""
          )
          count = int(result.strip())
          assert count == 0, f"Disabled jobset should not be in active_jobsets, got {count}"

      with subtest("Enabled jobsets are in active_jobsets view"):
          result = machine.succeed(
              "sudo -u circus psql -U circus -d circus -t -c \"SELECT COUNT(*) FROM active_jobsets WHERE name = 'enabled-jobset'\""
          )
          count = int(result.strip())
          assert count == 1, f"Enabled jobset should be in active_jobsets, got {count}"

      with subtest("One-shot jobset is in active_jobsets view"):
          result = machine.succeed(
              "sudo -u circus psql -U circus -d circus -t -c \"SELECT COUNT(*) FROM active_jobsets WHERE name = 'oneshot-jobset'\""
          )
          count = int(result.strip())
          assert count == 1, f"One-shot jobset should be in active_jobsets, got {count}"

      with subtest("One-at-a-time jobset is in active_jobsets view"):
          result = machine.succeed(
              "sudo -u circus psql -U circus -d circus -t -c \"SELECT COUNT(*) FROM active_jobsets WHERE name = 'oneatatime-jobset'\""
          )
          count = int(result.strip())
          assert count == 1, f"One-at-a-time jobset should be in active_jobsets, got {count}"

      with subtest("Jobset check_interval is correctly set"):
          project_id = machine.succeed(
              "curl -sf http://127.0.0.1:3000/api/v1/projects | jq -r '.items[] | select(.name==\"decl-project-1\") | .id'"
          ).strip()
          result = machine.succeed(
              f"curl -sf http://127.0.0.1:3000/api/v1/projects/{project_id}/jobsets | jq -r '.items[] | select(.name==\"oneatatime-jobset\") | .check_interval'"
          )
          assert result.strip() == "60", f"Expected check_interval 60, got '{result.strip()}'"

      # USER MANAGEMENT UI (admin-only)
      with subtest("Users page requires admin access"):
          htmlResp = machine.succeed(
              "curl -sf -H 'Authorization: Bearer circus_decl_admin' http://127.0.0.1:3000/users"
          )
          assert "User Management" in htmlResp or "Users" in htmlResp

          machine.fail(
              "curl -sf -H 'Authorization: Bearer circus_decl_readonly' http://127.0.0.1:3000/api/v1/users | grep 'decl-admin'"
          )
          adminApiResp = machine.succeed(
              "curl -sf -H 'Authorization: Bearer circus_decl_admin' http://127.0.0.1:3000/api/v1/users"
          )
          assert "decl-admin" in adminApiResp, "Expected decl-admin in API response"
          assert "decl-user" in adminApiResp, "Expected decl-user in API response"

      # DECLARATIVE JOBSET END-TO-END
      with subtest("Local flake repo for the decl-e2e project is populated"):
          machine.succeed("mkdir -p /var/lib/circus/test-repos")
          machine.succeed("git init --bare /var/lib/circus/test-repos/decl-flake.git")
          machine.succeed("git config --global --add safe.directory /var/lib/circus/test-repos/decl-flake.git")

          machine.succeed("mkdir -p /tmp/decl-flake-work")
          machine.succeed("cd /tmp/decl-flake-work && git init")
          machine.succeed("cd /tmp/decl-flake-work && git config user.email 'test@circus' && git config user.name 'circus Test'")
          machine.succeed(
              "cat > /tmp/decl-flake-work/flake.nix << 'FLAKE'\n"
              "{\n"
              '  description = "circus declarative test flake";\n'
              '  outputs = { self, ... }: {\n'
              '    packages.x86_64-linux.decl-hello = derivation {\n'
              '      name = "circus-decl-hello";\n'
              '      system = "x86_64-linux";\n'
              '      builder = "/bin/sh";\n'
              '      args = [ "-c" "echo decl-hello > $out" ];\n'
              "    };\n"
              "  };\n"
              "}\n"
              "FLAKE\n"
          )
          machine.succeed("cd /tmp/decl-flake-work && git add -A && git commit -m 'initial declarative flake'")
          machine.succeed("cd /tmp/decl-flake-work && git remote add origin /var/lib/circus/test-repos/decl-flake.git")
          machine.succeed("cd /tmp/decl-flake-work && git push origin HEAD:refs/heads/master")
          machine.succeed("chown -R circus:circus /var/lib/circus/test-repos")

      with subtest("Resolve decl-e2e jobset IDs"):
          decl_e2e_id = machine.succeed(
              "curl -sf http://127.0.0.1:3000/api/v1/projects "
              "| jq -r '.items[] | select(.name==\"decl-e2e\") | .id'"
          ).strip()
          assert len(decl_e2e_id) == 36, f"Expected decl-e2e UUID, got '{decl_e2e_id}'"

          enabled_jobset = machine.succeed(
              f"curl -sf http://127.0.0.1:3000/api/v1/projects/{decl_e2e_id}/jobsets "
              "| jq -r '.items[] | select(.name==\"packages\") | .id'"
          ).strip()
          off_jobset = machine.succeed(
              f"curl -sf http://127.0.0.1:3000/api/v1/projects/{decl_e2e_id}/jobsets "
              "| jq -r '.items[] | select(.name==\"off\") | .id'"
          ).strip()

      with subtest("Evaluator completes an evaluation for the declarative jobset"):
          machine.wait_until_succeeds(
              f"curl -sf 'http://127.0.0.1:3000/api/v1/evaluations?jobset_id={enabled_jobset}' "
              "| jq -e '.items[] | select(.status==\"completed\")'",
              timeout=120
          )

      with subtest("Declarative evaluation produced a build with a real drv_path"):
          eval_id = machine.succeed(
              f"curl -sf 'http://127.0.0.1:3000/api/v1/evaluations?jobset_id={enabled_jobset}' "
              "| jq -r '.items[] | select(.status==\"completed\") | .id' | head -1"
          ).strip()
          build_count = int(machine.succeed(
              f"curl -sf 'http://127.0.0.1:3000/api/v1/builds?evaluation_id={eval_id}' | jq '.items | length'"
          ).strip())
          assert build_count >= 1, f"Expected >= 1 build, got {build_count}"

          drv_path = machine.succeed(
              f"curl -sf 'http://127.0.0.1:3000/api/v1/builds?evaluation_id={eval_id}' | jq -r '.items[0].drv_path'"
          ).strip()
          assert drv_path.startswith("/nix/store/"), f"Expected /nix/store drv_path, got '{drv_path}'"

          decl_build_id = machine.succeed(
              f"curl -sf 'http://127.0.0.1:3000/api/v1/builds?evaluation_id={eval_id}' | jq -r '.items[0].id'"
          ).strip()

      with subtest("Queue runner builds the declarative derivation to success"):
          machine.wait_until_succeeds(
              f"curl -sf http://127.0.0.1:3000/api/v1/builds/{decl_build_id} | jq -e 'select(.status==\"succeeded\")'",
              timeout=120
          )
          output_path = machine.succeed(
              f"curl -sf http://127.0.0.1:3000/api/v1/builds/{decl_build_id} | jq -r .build_output_path"
          ).strip()
          assert output_path.startswith("/nix/store/"), f"Expected /nix/store output, got '{output_path}'"

      with subtest("Disabled declarative jobset produced no evaluations"):
          off_evals = int(machine.succeed(
              f"curl -sf 'http://127.0.0.1:3000/api/v1/evaluations?jobset_id={off_jobset}' | jq '.items | length'"
          ).strip())
          assert off_evals == 0, f"Disabled jobset should have 0 evaluations, got {off_evals}"

      with subtest("Disabled declarative jobset produced no builds"):
          # Join through evaluations since builds reference evaluation_id, not
          # jobset_id directly.
          off_builds = int(machine.succeed(
              f"sudo -u circus psql -U circus -d circus -tAc \"SELECT count(*) FROM builds b JOIN evaluations e ON b.evaluation_id = e.id WHERE e.jobset_id = '{off_jobset}'\""
          ).strip())
          assert off_builds == 0, f"Disabled jobset should have 0 builds, got {off_builds}"

      # BUILD PIPELINE CORRECTNESS
      # These subtests verify that the distributed build pipeline (evaluator ->
      # queue runner -> builder -> result sink) runs correctly end to end.
      with subtest("Build log is non-empty after completion"):
          # The queue runner writes the build log to disk; the server exposes it
          # at /api/v1/builds/{id}/log. An empty or missing log means the log
          # sink or the streaming path is broken even if the build succeeded.
          log_body = machine.succeed(
              f"curl -sf http://127.0.0.1:3000/api/v1/builds/{decl_build_id}/log"
          )
          assert len(log_body.strip()) > 0, "Build log must not be empty"

      with subtest("Build output path is realised in the Nix store"):
          # nix-store --check-validity exits non-zero if the path is missing or
          # its hash does not match, confirming the output is a real realised path
          # and not just a string the server fabricated.
          machine.succeed(f"nix-store --check-validity {output_path}")

      with subtest("API build list matches database build count for evaluation"):
          db_count = int(machine.succeed(
              f"sudo -u circus psql -U circus -d circus -tAc \"SELECT count(*) FROM builds WHERE evaluation_id = '{eval_id}'\""
          ).strip())
          api_count = int(machine.succeed(
              f"curl -sf 'http://127.0.0.1:3000/api/v1/builds?evaluation_id={eval_id}' "
              "| jq '.items | length'"
          ).strip())
          assert db_count == api_count, \
              f"DB build count ({db_count}) != API build count ({api_count})"

      with subtest("Re-evaluation with unchanged source produces no second evaluation"):
          # The evaluator caches by (jobset_id, source_commit). A second poll
          # against the same commit must not create a duplicate evaluation row.
          # We wait one poll cycle (checkInterval = 5s) then assert the count
          # is still 1.
          time.sleep(8)
          eval_count = int(machine.succeed(
              f"curl -sf 'http://127.0.0.1:3000/api/v1/evaluations?jobset_id={enabled_jobset}' "
              "| jq '.items | map(select(.status==\"completed\")) | length'"
          ).strip())
          assert eval_count == 1, \
              f"Expected 1 completed evaluation for unchanged source, got {eval_count}"
    '';
  }
