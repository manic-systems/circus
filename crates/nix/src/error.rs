#[derive(thiserror::Error, Debug)]
pub enum Error {
  #[error("Nix evaluation error: {0}")]
  Eval(String),
  #[error("IO error: {0}")]
  Io(#[from] std::io::Error),
  #[error("Build error: {0}")]
  Build(String),
  #[error("Validation error: {0}")]
  Validation(String),
  #[error("Timeout: {0}")]
  Timeout(String),
}

pub type Result<T> = std::result::Result<T, Error>;
