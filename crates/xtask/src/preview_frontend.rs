use std::net::{IpAddr, Ipv4Addr, SocketAddr};

use color_eyre::eyre::Result;

pub async fn run(host: IpAddr, port: u16) -> Result<()> {
  #![expect(clippy::print_stdout, reason = "xtask CLI output is intentional")]
  let addr = SocketAddr::new(host, port);
  let listener = tokio::net::TcpListener::bind(addr).await?;
  let local_addr = listener.local_addr()?;

  println!("serving Circus frontend preview at http://{local_addr}/__preview");
  axum::serve(listener, circus_server::routes::dashboard::preview_router())
    .await?;

  Ok(())
}

pub const fn default_host() -> IpAddr {
  IpAddr::V4(Ipv4Addr::LOCALHOST)
}
