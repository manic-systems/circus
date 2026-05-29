# Distributed Builds in Circus

[manic.systems]: https://github.com/manic.systems

Circus can run builds across a cluster of machines as per our needs in
[manic.systems]. The data plane is a fleet of long-running **agents**, each
running on a build host. Agents connect outbound to the queue-runner over a TCP
socket and stay connected; the runner pushes work down the connection, the agent
streams logs and results back up. This document, in turn, covers the protocol,
the lifecycle, the failure model and how the new agent path coexists with the
legacy SSH dispatch path.

## Why not Hydra's (gRPC) Design?

The new Rust rewrite of Hydra implements this layer with gRPC + tonic +
protobuf. That choice is reasonable, and the schema is the obvious starting
point for any modern fork. Circus picks a different transport: **Cap'n Proto**
with `capnp-rpc`. The reasons are concrete, not aesthetic:

1. **Object-capability RPC.** When an agent connects, it hands the runner a
   `Builder` capability. The runner holds that capability as a typed,
   per-connection handle. There is no `machine_id` lookup in a hash map on every
   method call: the capability _is_ the agent. Capabilities expire when the
   connection drops, so stale agent IDs cannot be addressed.
2. **Promise pipelining.** `assign(...)` returns a promise. The runner can
   immediately use that promise to send `log.write(chunk)` or `result.report`
   without an extra round trip, because the parameters of the next call ride
   along with the first. With gRPC over HTTP/2 this is one ack per call.
3. **No HTTP/2 overhead.** Cap'n Proto runs over plain framed TCP (or a
   tokio-rustls TLS stream). For an internal cluster protocol, HTTP/2's
   stream-prioritisation and headers are not load-bearing, and they cost
   throughput.
4. **Smaller dependency graph.** `capnp`, `capnp-rpc`, `capnp-futures` together
   are leaner than `tonic` + `prost` + `tonic-prost` + `hyper` + `h2` + `tower`.
   Less to keep up to date, less to audit.

Admittedly this comes with a few tradeoff. Namely, the schema is custom and
there is no built-in reflection or "describe service" tooling. We do, however,
compensate with a stable schema, semver, and `protoVersion` strings exchanged at
register time. TLS is also not built-in, but we can easily layer `tokio-rustls`
underneath the framed transport. mTLS works the same way: rustls verifies both
sides before any Cap'n Proto bytes flow.
