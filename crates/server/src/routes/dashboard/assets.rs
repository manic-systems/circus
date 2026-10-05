//! Bundles Topcoat's assets without its bundler, which pulls an HTTP client.

use std::{collections::HashSet, fs, io, path::Path};

use data_encoding::HEXLOWER;
use sha2::{Digest, Sha256};
use topcoat::asset::{
  AssetBundle,
  MANIFEST_NAME,
  MANIFEST_VERSION,
  Manifest,
  ManifestEntry,
  RawAsset,
  Source,
};

/// Loads the installed bundle, or the one next to the executable. Only debug
/// builds bundle their own sources there, since a release binary may run
/// where its build-time source paths do not exist.
///
/// # Errors
///
/// Returns an error if no bundle exists, see `circus-server --bundle-assets`.
pub fn load() -> io::Result<AssetBundle> {
  let exe = std::env::current_exe()?;
  let bin = exe
    .parent()
    .ok_or_else(|| io::Error::other("executable has no parent directory"))?;
  let installed = bin.join("../share/circus-server/assets");

  if installed.join(MANIFEST_NAME).is_file() {
    return AssetBundle::load_dir(installed);
  }

  let local = bin.join("assets");

  if cfg!(debug_assertions) {
    bundle(&fs::read(&exe)?, &local)?;
  }

  AssetBundle::load_dir(local)
}

/// Copies every asset `binary` declares into `out` under content-hashed names.
///
/// # Errors
///
/// Returns an error for remote assets or unreadable or unwritable files.
pub fn bundle(binary: &[u8], out: &Path) -> io::Result<()> {
  fs::create_dir_all(out)?;

  let mut seen = HashSet::new();
  let mut assets = Vec::new();

  for asset in RawAsset::find_in_binary(binary) {
    if !seen.insert(asset.id()) {
      continue;
    }

    let Source::Path(path) = asset.source() else {
      return Err(io::Error::other(format!(
        "remote asset {} is unsupported",
        asset.path()
      )));
    };
    let bytes = fs::read(&path).map_err(|error| {
      io::Error::new(error.kind(), format!("{}: {error}", path.display()))
    })?;
    let hash = HEXLOWER.encode(&Sha256::digest(&bytes));
    let file = file_name(&asset, &path, &hash);
    let content_type = asset
      .options()
      .content_type()
      .map_or_else(|| content_type(&path).to_owned(), str::to_owned);

    fs::write(out.join(&file), &bytes)?;
    assets.push(ManifestEntry {
      id: asset.id(),
      file,
      hash,
      content_type,
    });
  }

  Manifest {
    version: MANIFEST_VERSION,
    assets,
  }
  .save(out.join(MANIFEST_NAME))
}

fn file_name(asset: &RawAsset, path: &Path, hash: &str) -> String {
  let stem = asset.options().rename().unwrap_or_else(|| {
    path
      .file_stem()
      .and_then(|stem| stem.to_str())
      .unwrap_or("asset")
  });

  let extension = path
    .extension()
    .and_then(|extension| extension.to_str())
    .map(|extension| format!(".{extension}"))
    .unwrap_or_default();

  format!("{stem}-{}{extension}", &hash[..16])
}

fn content_type(path: &Path) -> &'static str {
  match path.extension().and_then(|extension| extension.to_str()) {
    Some("js" | "mjs") => "text/javascript",
    Some("css") => "text/css",
    Some("svg") => "image/svg+xml",
    Some("png") => "image/png",
    Some("woff2") => "font/woff2",
    _ => "application/octet-stream",
  }
}
