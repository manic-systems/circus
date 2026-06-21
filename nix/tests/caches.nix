{
  pkgs,
  self,
}:
pkgs.testers.nixosTest {
  name = "circus-caches";

  nodes.machine = {lib, ...}: {
    imports = [
      self.nixosModules.circus
      ../common/vm.nix
    ];
    _module.args.self = self;

    # Signing lets the "How to use" panel derive a public key. base.nix sets the
    # global cache on and signing off, so force signing here. The secret key is
    # a throwaway generated for this test only.
    services.circus.settings.signing = {
      enabled = lib.mkForce true;
      key_file = "/etc/circus/cache-priv-key";
    };
    services.circus.settings.cache.cache_url = "http://127.0.0.1:3000/nix-cache";

    environment.etc."circus/cache-priv-key".text = "circus-test-cache-1:Ei+yhiBZ1xVda0WgmQJ+Wc+oFc5FFHlcvsvzzGIRUqJToBvhZRoQCfyp+sxwvzuLcDJfY+Ud9N/O8v9oBKOnnw==";
  };

  testScript = ''
    import json
    import hashlib

    expected_public_key = "circus-test-cache-1:U6Ab4WUaEAn8qfrMcL87i3AyX2PlHfTfzvL/aASjp58="

    machine.start()
    machine.wait_for_unit("postgresql.service")
    machine.wait_until_succeeds("setpriv --reuid=circus --regid=circus --init-groups psql -U circus -d circus -c 'SELECT 1'", timeout=30)
    machine.wait_for_unit("circus-server.service")
    machine.wait_until_succeeds("curl -sf http://127.0.0.1:3000/health", timeout=30)

    # Keep background workers from racing the assertions.
    machine.succeed("systemctl stop circus-evaluator.service circus-queue-runner.service")

    def psql(sql):
        machine.succeed(
            "setpriv --reuid=circus --regid=circus --init-groups "
            f"psql -U circus -d circus -c \"{sql}\""
        )

    # Admin and read-only API keys.
    api_token = "circus_testkey123"
    api_hash = hashlib.sha256(api_token.encode()).hexdigest()
    psql(f"INSERT INTO api_keys (name, key_hash, role) VALUES ('test', '{api_hash}', 'admin')")
    auth_header = f"-H 'Authorization: Bearer {api_token}'"

    ro_token = "circus_readonly_key"
    ro_hash = hashlib.sha256(ro_token.encode()).hexdigest()
    psql(f"INSERT INTO api_keys (name, key_hash, role) VALUES ('readonly', '{ro_hash}', 'read-only')")
    ro_header = f"-H 'Authorization: Bearer {ro_token}'"

    # Two signed global NARs with distinct hash prefixes and package names. The
    # 32-char hashes use Nix's base32 alphabet so the cache route accepts them.
    foo_hash = "00000000000000000000000000000000"
    bar_hash = "11111111111111111111111111111111"
    # The "references" column has a '{}' default, so it is left unset here to
    # keep the literal SQL free of nested identifier quoting.
    psql(
        "INSERT INTO narinfo_cache "
        "(store_path, nar_hash, nar_size, file_size, compression, url, sig) VALUES "
        f"('/nix/store/{foo_hash}-foopkg', 'sha256:aaaa', 200, 100, 'zstd', 'nar/{foo_hash}.nar.zst', 'circus:testsig')"
    )
    psql(
        "INSERT INTO narinfo_cache "
        "(store_path, nar_hash, nar_size, file_size, compression, url, sig) VALUES "
        f"('/nix/store/{bar_hash}-barpkg', 'sha256:bbbb', 300, 150, 'zstd', 'nar/{bar_hash}.nar.zst', 'circus:testsig')"
    )

    with subtest("Admin cache list reports the global cache with storage"):
        caches = json.loads(machine.succeed(f"curl -sf {auth_header} http://127.0.0.1:3000/api/v1/admin/caches"))
        glob = next(c for c in caches if c["name"] == "global")
        assert glob["scope"] == "global", glob
        assert glob["active"] is True, glob
        assert glob["nar_count"] >= 2, glob
        assert glob["compressed_bytes"] >= 250, glob

    with subtest("Admin cache detail derives the public key and substituter"):
        detail = json.loads(machine.succeed(f"curl -sf {auth_header} http://127.0.0.1:3000/api/v1/admin/caches/global"))
        assert detail["public_key"] == expected_public_key, detail
        assert detail["substituter_url"] == "http://127.0.0.1:3000/nix-cache", detail
        assert detail["nix_conf_snippet"] is not None, detail
        assert "trusted-public-keys" in detail["nix_conf_snippet"], detail
        assert detail["storage"]["nar_count"] >= 2, detail

    with subtest("NAR search filters by package name"):
        res = json.loads(machine.succeed(f"curl -sf {auth_header} 'http://127.0.0.1:3000/api/v1/admin/caches/global/nars?package=foopkg'"))
        assert res["total"] == 1, res
        assert res["items"][0]["package_name"] == "foopkg", res

    with subtest("NAR search filters by hash prefix"):
        res = json.loads(machine.succeed(f"curl -sf {auth_header} 'http://127.0.0.1:3000/api/v1/admin/caches/global/nars?hash=11111'"))
        assert res["total"] == 1, res
        assert res["items"][0]["package_name"] == "barpkg", res

    with subtest("Timeseries endpoints return JSON arrays"):
        storage = json.loads(machine.succeed(f"curl -sf {auth_header} 'http://127.0.0.1:3000/api/v1/admin/caches/global/storage-timeseries?granularity=hours'"))
        assert "timestamps" in storage and "bytes_added" in storage, storage
        traffic = json.loads(machine.succeed(f"curl -sf {auth_header} 'http://127.0.0.1:3000/api/v1/admin/caches/global/traffic-timeseries?granularity=hours'"))
        assert "timestamps" in traffic and "requests" in traffic, traffic

    with subtest("Cache endpoints are admin-only"):
        code = machine.succeed(
            f"curl -s -o /dev/null -w '%{{http_code}}' {ro_header} http://127.0.0.1:3000/api/v1/admin/caches"
        )
        assert code.strip() == "403", f"read-only key should be forbidden, got {code.strip()}"

    with subtest("Dashboard Caches pages render for admins"):
        body = machine.succeed(f"curl -sf {auth_header} http://127.0.0.1:3000/caches")
        assert "Binary Caches" in body, "caches list missing heading"
        assert "Total NARs" in body, "caches list missing stat strip"

        detail = machine.succeed(f"curl -sf {auth_header} http://127.0.0.1:3000/caches/global")
        assert "How to use this cache" in detail, "detail page missing how-to-use panel"
        assert expected_public_key in detail, "detail page missing derived public key"

        nars = machine.succeed(f"curl -sf {auth_header} http://127.0.0.1:3000/caches/global/nars")
        assert "foopkg" in nars, "NARs page missing seeded package"

    with subtest("Serving a narinfo records cache traffic via the flush worker"):
        narinfo = machine.succeed(f"curl -sf http://127.0.0.1:3000/nix-cache/{foo_hash}.narinfo")
        assert "StorePath:" in narinfo, narinfo
        # The flush worker drains in-memory counters every 60s.
        machine.wait_until_succeeds(
            "setpriv --reuid=circus --regid=circus --init-groups "
            "psql -U circus -d circus -tAc "
            "\"SELECT COALESCE(SUM(requests), 0) FROM cache_traffic WHERE cache_name = 'global'\" "
            "| grep -qE '^[1-9]'",
            timeout=120,
        )
        detail = json.loads(machine.succeed(f"curl -sf {auth_header} http://127.0.0.1:3000/api/v1/admin/caches/global"))
        assert detail["traffic_last_hour"]["requests"] >= 1, detail
  '';
}
