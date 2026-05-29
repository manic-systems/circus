#![expect(clippy::expect_used, reason = "Fine in build scripts")]
use std::{env, fs, path::PathBuf};

fn main() {
  println!("cargo:rerun-if-changed=schema/circus.capnp");

  let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR is set"));

  capnpc::CompilerCommand::new()
    .src_prefix("schema")
    .file("schema/circus.capnp")
    .output_path(&out_dir)
    .run()
    .expect("capnpc failed; is the capnp tool on PATH?");

  // We re-export those at the crate root, so flip them to `pub mod`
  // before include!() picks the file up. This is a one-pass text
  // rewrite over module declarations only; struct fields and the
  // `mod tests {}` skeleton are left alone.
  let generated_path = out_dir.join("circus_capnp.rs");
  let mut text = fs::read_to_string(&generated_path)
    .expect("read generated circus_capnp.rs");
  text = text
    .lines()
    .map(|l| {
      l.strip_prefix("mod ")
        .map_or_else(|| l.to_owned(), |rest| format!("pub mod {rest}"))
    })
    .collect::<Vec<_>>()
    .join("\n");
  fs::write(&generated_path, text).expect("rewrite generated circus_capnp.rs");
}
