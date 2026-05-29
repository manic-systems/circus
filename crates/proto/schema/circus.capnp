@0xc1b9c5d9a02e7f01;

# Circus distributed build protocol.
#
# Source of truth for the runner <-> agent wire format. Generated bindings live
# at OUT_DIR/circus_capnp.rs and are re-exported from `circus_proto::schema`.
#
# Versioning: bump `protoVersion` whenever this file changes in a wire-breaking
# way. Both sides exchange the version at register-time and refuse to talk if
# they do not match.

# Top-level capability exposed by the queue-runner on every accepted
# connection. The agent's capnp-rpc bootstrap resolves to this.
interface Runner {
  # Identify, authenticate, and hand the runner a callback `Builder`
  # capability for receiving work. Returns a session capability used for
  # outbound traffic from the agent (heartbeats).
  register @0 (info :AgentInfo, builder :Builder) -> (session :AgentSession);

  # Identify the running runner. Used by the agent to refuse to talk to an
  # incompatible peer before sending sensitive info on `register`.
  version @1 () -> (proto :Text, server :Text);

  # Ask the runner to mint S3 PUT URLs for each `(storePath, narHash)` pair
  # the agent intends to upload. The agent then streams the compressed NAR
  # to each URL itself and finalises via `notifyUploadComplete`. Returns one
  # `PresignedNarResponse` per request, in the same order. Errors are per
  # entry; the request itself succeeds even if individual paths fail.
  requestPresignedUrls @2 (machineId :Text, buildId :Text,
                            request :List(PresignedNarRequest))
                       -> (responses :List(PresignedNarResponse));

  # Tell the runner that the NAR + narinfo for one path have been uploaded
  # successfully. The runner persists the narinfo (signs if a signing key is
  # configured) so subsequent fetches see the path in the cache.
  notifyUploadComplete @3 (machineId :Text, buildId :Text, narInfo :NarInfo)
                       -> ();
}

# Capability the agent passes to the runner during register. The runner calls
# these methods to push work to the agent.
interface Builder {
  # Run one build. `log` and `result` are runner-side capabilities the agent
  # calls back into during execution.
  assign @0 (job :BuildAssignment, log :LogSink, result :ResultSink) -> ();

  # Best-effort abort of a running build. The agent SIGTERMs its child and
  # reports a BuildResult with `success = false`.
  abort @1 (buildId :Text) -> ();

  # Tell the agent to drain and exit. The agent finishes in-flight builds
  # and stops accepting new assignments.
  shutdown @2 (reason :Text) -> ();
}

# Runner-side capability for agent-originated traffic. Held by the agent
# across the session.
interface AgentSession {
  # Periodic load/PSI report. The agent calls this on a timer.
  heartbeat @0 (ping :Heartbeat) -> ();
}

# Runner-side capability for log chunks. The agent calls `write` repeatedly
# with framed log data and ends with `close`.
interface LogSink {
  write @0 (chunk :Data) -> ();
  close @1 () -> ();
}

# Runner-side capability for the final build result. The agent calls this
# exactly once per `assign`.
interface ResultSink {
  report @0 (result :BuildResult) -> ();
}

struct AgentInfo {
  hostname           @0  :Text;
  name               @1  :Text;       # operator-assigned agent name
  machineId          @2  :Text;       # UUIDv4 generated on first start
  systems            @3  :List(Text); # nix systems advertised
  supportedFeatures  @4  :List(Text);
  mandatoryFeatures  @5  :List(Text);
  speedFactor        @6  :Float32;
  cpuCount           @7  :UInt32;
  maxJobs            @8  :UInt32;
  protoVersion       @9  :Text;
  authToken          @10 :Text;       # bearer; runner compares to a hash
}

struct Heartbeat {
  load1        @0 :Float32;
  load5        @1 :Float32;
  load15       @2 :Float32;
  memTotal     @3 :UInt64;
  memUsed      @4 :UInt64;
  storeFree    @5 :UInt64;
  buildDirFree @6 :UInt64;
  currentJobs  @7 :UInt32;
  pressure     @8 :PressureState;
}

# /proc/pressure snapshots; absent fields are zeroed by the agent.
struct PressureState {
  cpuAvg10  @0 :Float32;
  memAvg10  @1 :Float32;
  ioAvg10   @2 :Float32;
  cpuAvg60  @3 :Float32;
  memAvg60  @4 :Float32;
  ioAvg60   @5 :Float32;
}

struct BuildAssignment {
  buildId         @0 :Text;       # UUIDv4 from `builds.id`
  drvPath         @1 :Text;       # /nix/store/...-foo.drv
  maxLogSize      @2 :UInt64;     # bytes; agent truncates and reports failure
  maxSilentTime   @3 :UInt32;     # seconds; 0 = unlimited
  buildTimeout    @4 :UInt32;     # seconds; 0 = unlimited
  extraNixArgs    @5 :List(Text);
  expectedOutputs @6 :List(Text); # output names ("out", "dev", ...)
  inputClosure    @7 :List(Text); # input store paths the agent must have
  # Presigned-upload configuration. Absent (`hasField=false`) keeps the
  # cache-upload path off; present switches the agent to upload directly to
  # the runner's configured S3 bucket via presigned URLs.
  presignedUpload @8 :PresignedUploadOpts;
}

struct PresignedUploadOpts {
  # If set, the agent uploads `debug-info` NARs alongside the main output.
  # Off by default; only useful when the cache backs a debugger.
  uploadDebugInfo @0 :Bool;
  # Compression to apply before upload. Must match what the runner intends
  # to advertise in the narinfo. "zstd" or "xz" are the realistic choices;
  # "none" passes the NAR through unchanged.
  compression     @1 :Text;
  compressionLevel @2 :Int32;     # 0 = library default
}

enum StepStatus {
  preparing        @0;
  sendingInputs    @1;
  building         @2;
  receivingOutputs @3;
  postProcessing   @4;
  done             @5;
}

enum BuildOutcome {
  success            @0;
  buildFailure       @1;
  preparingFailure   @2;
  importFailure      @3;
  uploadFailure      @4;
  postProcessFailure @5;
  aborted            @6;
  timedOut           @7;
}

struct BuildResult {
  outcome       @0 :BuildOutcome;
  exitCode      @1 :Int32;            # -1 if killed by signal
  importTimeMs  @2 :UInt64;
  buildTimeMs   @3 :UInt64;
  uploadTimeMs  @4 :UInt64;
  outputs       @5 :List(OutputInfo);
  errorMessage  @6 :Text;
}

struct OutputInfo {
  name        @0 :Text;
  path        @1 :Text;   # /nix/store/...-foo
  narHash     @2 :Text;   # sha256:... or sri-style
  narSize     @3 :UInt64;
  closureSize @4 :UInt64;
}

struct PresignedNarRequest {
  storePath @0 :Text;     # /nix/store/...
  narHash   @1 :Text;     # sha256:... over the uncompressed NAR
  narSize   @2 :UInt64;   # uncompressed
}

struct PresignedNarResponse {
  storePath   @0 :Text;
  # PUT URL for the (possibly compressed) NAR file itself. The URL embeds
  # the credentials; the agent uploads via `PUT` with the body as-is.
  narUrl      @1 :Text;
  # Path under the bucket where the NAR lands (without query string). The
  # narinfo emitted post-upload references this in its `URL:` field.
  narPath     @2 :Text;
  compression @3 :Text;     # mirrors PresignedUploadOpts.compression
  errorMessage @4 :Text;    # non-empty if presigning failed for this entry
}

# Persisted narinfo for one uploaded path. The runner writes this into its
# cache layer (signing if configured) once the agent confirms upload.
struct NarInfo {
  storePath   @0 :Text;
  narHash     @1 :Text;       # sha256:... over the uncompressed NAR
  narSize     @2 :UInt64;     # uncompressed
  fileHash    @3 :Text;       # sha256:... over the compressed file
  fileSize    @4 :UInt64;     # compressed
  compression @5 :Text;
  url         @6 :Text;       # path within the bucket
  deriver     @7 :Text;
  references  @8 :List(Text); # absolute store paths
  ca          @9 :Text;       # content-addressed reference, may be empty
  sig         @10 :Text;      # signature, may be empty (runner re-signs)
}
