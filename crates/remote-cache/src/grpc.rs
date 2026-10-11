//! gRPC framing over hyper's HTTP/2 server: length-prefixed messages in,
//! status in the trailers out, uncompressed only.

use std::{convert::Infallible, fmt::Display};

use bytes::{Buf as _, Bytes, BytesMut};
use http::{HeaderMap, HeaderValue, Response};
use http_body_util::{BodyExt as _, StreamBody, combinators::UnsyncBoxBody};
use hyper::body::{Frame, Incoming};
use percent_encoding::{AsciiSet, CONTROLS, utf8_percent_encode};

/// Requests may carry messages up to this size, above tonic's 4 MiB default.
pub const MAX_MESSAGE: usize = 16 << 20;

pub type Body = UnsyncBoxBody<Bytes, Infallible>;

/// The gRPC spec percent-encodes `grpc-message` outside printable ASCII.
const GRPC_MESSAGE: &AsciiSet = &CONTROLS.add(b'%');

/// The `google.rpc.Code` values the cache returns.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum Code {
  Ok                 = 0,
  InvalidArgument    = 3,
  NotFound           = 5,
  PermissionDenied   = 7,
  ResourceExhausted  = 8,
  FailedPrecondition = 9,
  Unimplemented      = 12,
  Internal           = 13,
}

#[derive(Debug)]
pub struct Status {
  code:    Code,
  message: String,
}

impl Status {
  fn new(code: Code, message: impl Display) -> Self {
    Self {
      code,
      message: message.to_string(),
    }
  }

  pub fn invalid_argument(message: impl Display) -> Self {
    Self::new(Code::InvalidArgument, message)
  }

  pub fn not_found(message: impl Display) -> Self {
    Self::new(Code::NotFound, message)
  }

  pub fn permission_denied(message: impl Display) -> Self {
    Self::new(Code::PermissionDenied, message)
  }

  pub fn resource_exhausted(message: impl Display) -> Self {
    Self::new(Code::ResourceExhausted, message)
  }

  pub fn failed_precondition(message: impl Display) -> Self {
    Self::new(Code::FailedPrecondition, message)
  }

  pub fn unimplemented(message: impl Display) -> Self {
    Self::new(Code::Unimplemented, message)
  }

  pub fn internal(message: impl Display) -> Self {
    Self::new(Code::Internal, message)
  }

  pub const fn code(&self) -> Code {
    self.code
  }

  pub fn message(&self) -> &str {
    &self.message
  }

  fn trailers(&self) -> HeaderMap {
    let mut trailers = HeaderMap::new();
    trailers.insert("grpc-status", HeaderValue::from(self.code as i32));

    let message = utf8_percent_encode(&self.message, GRPC_MESSAGE).to_string();
    if let Ok(message) = HeaderValue::from_str(&message) {
      trailers.insert("grpc-message", message);
    }

    trailers
  }

  /// A trailers-only response carrying this status in its headers.
  pub fn into_response(self) -> Response<Body> {
    let mut response = Response::new(empty());
    let headers = response.headers_mut();
    headers
      .insert("content-type", HeaderValue::from_static("application/grpc"));
    headers.extend(self.trailers());
    response
  }
}

impl From<std::io::Error> for Status {
  fn from(error: std::io::Error) -> Self {
    Self::internal(error)
  }
}

fn empty() -> Body {
  http_body_util::Empty::new().boxed_unsync()
}

/// `message` with its gRPC length prefix.
pub fn frame(message: &[u8]) -> Bytes {
  let mut framed = BytesMut::with_capacity(message.len() + 5);
  framed.extend_from_slice(&[0]);
  framed.extend_from_slice(
    &u32::try_from(message.len())
      .unwrap_or(u32::MAX)
      .to_be_bytes(),
  );
  framed.extend_from_slice(message);
  framed.freeze()
}

/// A streaming response: `data` frames, then `grpc-status` in the trailers.
pub fn streaming<S>(frames: S) -> Response<Body>
where
  S: futures::Stream<Item = Result<Frame<Bytes>, Infallible>> + Send + 'static,
{
  let mut response = Response::new(StreamBody::new(frames).boxed_unsync());
  response
    .headers_mut()
    .insert("content-type", HeaderValue::from_static("application/grpc"));
  response
}

/// The trailers frame that ends a stream with `status`, OK when `None`.
pub fn end(status: Option<&Status>) -> Frame<Bytes> {
  Frame::trailers(
    status
      .map_or_else(|| Status::new(Code::Ok, "").trailers(), Status::trailers),
  )
}

/// A unary reply carrying `message`.
pub fn reply(message: &[u8]) -> Response<Body> {
  let frames = [Ok(Frame::data(frame(message))), Ok(end(None))];
  streaming(futures::stream::iter(frames))
}

/// Reads the length-prefixed messages of a request body one at a time.
pub struct Messages {
  body:     Incoming,
  buffered: BytesMut,
  done:     bool,
}

impl Messages {
  pub fn new(body: Incoming) -> Self {
    Self {
      body,
      buffered: BytesMut::new(),
      done: false,
    }
  }

  /// The next message, or `None` once the body ends cleanly.
  pub async fn next(&mut self) -> Result<Option<Bytes>, Status> {
    loop {
      if self.buffered.len() >= 5 {
        if self.buffered[0] != 0 {
          return Err(Status::unimplemented("compressed gRPC messages"));
        }
        let length = u32::from_be_bytes([
          self.buffered[1],
          self.buffered[2],
          self.buffered[3],
          self.buffered[4],
        ]) as usize;

        if length > MAX_MESSAGE {
          return Err(Status::resource_exhausted(format!(
            "gRPC message of {length} bytes exceeds {MAX_MESSAGE}"
          )));
        }

        if self.buffered.len() >= 5 + length {
          self.buffered.advance(5);
          return Ok(Some(self.buffered.split_to(length).freeze()));
        }
      }

      if self.done {
        return if self.buffered.is_empty() {
          Ok(None)
        } else {
          Err(Status::invalid_argument("truncated gRPC message"))
        };
      }

      match self.body.frame().await {
        Some(Ok(frame)) => {
          if let Ok(data) = frame.into_data() {
            self.buffered.extend_from_slice(&data);
          }
        },
        Some(Err(error)) => {
          return Err(Status::invalid_argument(format!(
            "request body failed: {error}"
          )));
        },
        None => self.done = true,
      }
    }
  }

  /// Exactly one message, as a unary call carries.
  pub async fn single(&mut self) -> Result<Bytes, Status> {
    let message = self
      .next()
      .await?
      .ok_or_else(|| Status::invalid_argument("missing request message"))?;

    self.drain().await;
    Ok(message)
  }

  /// Consumes the rest of the body so the client sees the reply rather than
  /// a reset stream.
  pub async fn drain(&mut self) {
    while !self.done {
      match self.body.frame().await {
        Some(Ok(_)) => {},
        Some(Err(_)) | None => self.done = true,
      }
    }

    self.buffered.clear();
  }
}
