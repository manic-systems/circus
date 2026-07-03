fn main() {
  // Keep the vendored git2 copy from interposing nix-bindings' libgit2.so.
  if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("linux") {
    println!("cargo:rustc-link-arg-bins=-Wl,--exclude-libs,ALL");
  }
}
