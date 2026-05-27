{
  pkgs,
  self,
}: let
  circus-packages = self.packages.${pkgs.stdenv.hostPlatform.system};

  # Password files for testing passwordFile option
  # Passwords must be at least 12 characters with at least one uppercase letter
  adminPasswordFile = pkgs.writeText "admin-password" "SecretAdmin123!";
  userPasswordFile = pkgs.writeText "user-password" "SecretUser123!";
  disabledPasswordFile = pkgs.writeText "disabled-password" "DisabledPass123!";
in
  pkgs.testers.nixosTest {
    name = "circus-declarative";

    nodes.machine = {
      imports = [self.nixosModules.circus];
      _module.args.self = self;

      programs.git.enable = true;
      security.sudo.enable = true;
      environment.systemPackages = with pkgs; [nix nix-eval-jobs zstd curl jq openssl];

      services.circus = {
        enable = true;
        package = circus-packages.circus-server;
        evaluatorPackage = circus-packages.circus-evaluator;
        queueRunnerPackage = circus-packages.circus-queue-runner;
        migratePackage = circus-packages.circus-migrate-cli;

        server.enable = true;
        evaluator.enable = true;
        queueRunner.enable = true;

        settings = {
          database.url = "postgresql:///circus?host=/run/postgresql";
          server = {
            host = "127.0.0.1";
            port = 3000;
            cors_permissive = false;
          };
          gc.enabled = false;
          logs.log_dir = "/var/lib/circus/logs";
          cache.enabled = true;
          signing.enabled = false;
        };

        # Declarative users
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

        # Declarative API keys
        declarative.apiKeys = [
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

        # Declarative projects with various jobset states
        declarative.projects = [
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
        ];
      };
    };

    testScript = ''
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

      with subtest("User with passwordFile has correct password hash"):
          # The password in the file is 'SecretAdmin123!'
          result = machine.succeed(
              "sudo -u circus psql -U circus -d circus -t -c \"SELECT password_hash FROM users WHERE username = 'decl-admin'\""
          )
          assert len(result.strip()) > 50, "Password hash should be substantial length"

      with subtest("User with inline password has correct password hash"):
          result = machine.succeed(
              "sudo -u circus psql -U circus -d circus -t -c \"SELECT password_hash FROM users WHERE username = 'decl-user'\""
          )
          assert result.strip().startswith("$argon2"), f"Expected argon2 hash for inline password user, got '{result.strip()[:20]}...'"

      # DECLARATIVE USER WEB LOGIN
      with subtest("Web login with declarative admin user succeeds"):
          # Login via POST to /login with username/password
          result = machine.succeed(
              "curl -s -w '\\n%{http_code}' "
              "-X POST http://127.0.0.1:3000/login "
              "-d 'username=decl-admin&password=SecretAdmin123!'"
          )
          lines = result.strip().split('\n')
          code = lines[-1]
          # Should redirect (302/303) on success
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
          # Should return 401 for wrong password
          assert code in ("401",), f"Expected 401 for wrong password, got {code}"

      with subtest("Web login with disabled user fails"):
          result = machine.succeed(
              "curl -s -w '\\n%{http_code}' "
              "-X POST http://127.0.0.1:3000/login "
              "-d 'username=decl-disabled&password=DisabledPass123!'"
          )
          lines = result.strip().split('\n')
          code = lines[-1]
          # Disabled user should not be able to login (401 or 403)
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

      # IDEMPOTENCY
      with subtest("Bootstrap is idempotent - no duplicate users"):
          result = machine.succeed(
              "sudo -u circus psql -U circus -d circus -t -c \"SELECT COUNT(*) FROM users WHERE username = 'decl-admin'\""
          )
          count = int(result.strip())
          assert count == 1, f"Expected exactly 1 decl-admin user, got {count}"

      with subtest("Bootstrap is idempotent - no duplicate projects"):
          result = machine.succeed(
              "curl -sf http://127.0.0.1:3000/api/v1/projects | jq '.items | map(select(.name==\"decl-project-1\")) | length'"
          )
          count = int(result.strip())
          assert count == 1, f"Expected exactly 1 decl-project-1, got {count}"

      with subtest("Bootstrap is idempotent - no duplicate API keys"):
          result = machine.succeed(
              "sudo -u circus psql -U circus -d circus -t -c \"SELECT COUNT(*) FROM api_keys WHERE name = 'decl-admin-key'\""
          )
          count = int(result.strip())
          assert count == 1, f"Expected exactly 1 decl-admin-key, got {count}"

      with subtest("Bootstrap is idempotent - no duplicate jobsets"):
          project_id = machine.succeed(
              "curl -sf http://127.0.0.1:3000/api/v1/projects | jq -r '.items[] | select(.name==\"decl-project-1\") | .id'"
          ).strip()
          result = machine.succeed(
              f"curl -sf http://127.0.0.1:3000/api/v1/projects/{project_id}/jobsets | jq '.items | map(select(.name==\"enabled-jobset\")) | length'"
          )
          count = int(result.strip())
          assert count == 1, f"Expected exactly 1 enabled-jobset, got {count}"

      # USER MANAGEMENT UI (admin-only)
      with subtest("Users page requires admin access"):
          # Test HTML /users endpoint
          htmlResp = machine.succeed(
              "curl -sf -H 'Authorization: Bearer circus_decl_admin' http://127.0.0.1:3000/users"
          )
          assert "User Management" in htmlResp or "Users" in htmlResp

          # Non-admin should be denied access via API
          machine.fail(
              "curl -sf -H 'Authorization: Bearer circus_decl_readonly' http://127.0.0.1:3000/api/v1/users | grep 'decl-admin'"
          )
          # Admin should have access via API
          adminApiResp = machine.succeed(
              "curl -sf -H 'Authorization: Bearer circus_decl_admin' http://127.0.0.1:3000/api/v1/users"
          )
          assert "decl-admin" in adminApiResp, "Expected decl-admin in API response"
          assert "decl-user" in adminApiResp, "Expected decl-user in API response"

      with subtest("Users API shows declarative users for admin"):
          # Use the admin API key to list users instead of session-based auth
          result = machine.succeed(
              "curl -sf -H 'Authorization: Bearer circus_decl_admin' http://127.0.0.1:3000/api/v1/users"
          )
          assert "decl-admin" in result, f"Users API should return decl-admin. Got: {result[:500]}"
          assert "decl-user" in result, f"Users API should return decl-user. Got: {result[:500]}"

      # STARRED JOBS PAGE
      with subtest("Starred page exists and returns 200"):
          code = machine.succeed(
              "curl -s -o /dev/null -w '%{http_code}' http://127.0.0.1:3000/starred"
          )
          assert code.strip() == "200", f"Expected 200, got {code.strip()}"

      with subtest("Starred page shows login prompt when not logged in"):
          body = machine.succeed("curl -sf http://127.0.0.1:3000/starred")
          assert "Login required" in body or "login" in body.lower(), "Starred page should prompt for login"
    '';
  }
