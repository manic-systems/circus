//! Format Askama HTML templates with `askama_fmt`.

use std::{
  fs,
  path::{Path, PathBuf},
};

use color_eyre::{
  Result,
  eyre::{bail, eyre},
};

pub fn run(check: bool) -> Result<()> {
  #![expect(
    clippy::print_stdout,
    clippy::print_stderr,
    reason = "xtask progress output"
  )]
  let workspace_root = workspace_root()?;
  let templates_dir = workspace_root.join("crates/server/templates");

  let opts = askama_fmt::FormatOptions::find_and_load(&workspace_root);

  let mut files: Vec<PathBuf> = fs::read_dir(&templates_dir)
    .map_err(|e| eyre!("cannot read {}: {e}", templates_dir.display()))?
    .filter_map(|entry| {
      let entry = entry.ok()?;
      let path = entry.path();
      if path.extension()?.to_str()? == "html" {
        Some(path)
      } else {
        None
      }
    })
    .collect();
  files.sort();

  let mut reformatted = 0usize;
  let mut needs_fmt: Vec<PathBuf> = Vec::new();

  for path in &files {
    let original =
      fs::read_to_string(path).map_err(|e| eyre!("{}: {e}", path.display()))?;
    let formatted = askama_fmt::format(&original, &opts);
    if formatted != original {
      if check {
        needs_fmt.push(path.clone());
      } else {
        fs::write(path, &formatted)
          .map_err(|e| eyre!("{}: {e}", path.display()))?;
        println!(
          "  formatted {}",
          path.file_name().unwrap_or_default().to_string_lossy()
        );
        reformatted += 1;
      }
    }
  }

  if check {
    if needs_fmt.is_empty() {
      println!("all {} templates already formatted", files.len());
    } else {
      for path in &needs_fmt {
        eprintln!(
          "  needs formatting: {}",
          path.file_name().unwrap_or_default().to_string_lossy()
        );
      }
      bail!(
        "{} template(s) need formatting (run `cargo xtask fmt-templates` to \
         fix)",
        needs_fmt.len()
      );
    }
  } else {
    let n = files.len();
    let already_ok = n - reformatted;
    println!(
      "{n} templates checked, {reformatted} reformatted, {already_ok} already \
       up-to-date"
    );
  }

  Ok(())
}

fn workspace_root() -> Result<PathBuf> {
  let manifest = std::env::var("CARGO_MANIFEST_DIR")
    .map_err(|_| eyre!("CARGO_MANIFEST_DIR not set"))?;
  // xtask manifest lives at crates/xtask; go up two levels to the workspace
  // root
  Path::new(&manifest)
    .parent()
    .and_then(|p| p.parent())
    .map(PathBuf::from)
    .ok_or_else(|| eyre!("cannot resolve workspace root from {manifest}"))
}
