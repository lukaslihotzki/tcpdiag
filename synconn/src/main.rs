#![feature(tcp_linger)]

use std::net::{Ipv6Addr, SocketAddrV6, TcpListener, TcpStream};
use std::time::Duration;

fn parse_args() -> (u32, Option<(String, Vec<String>)>) {
    let mut it = std::env::args().skip(1);
    let Some(count) = it.next() else {
        return (500, None);
    };
    let Ok(count) = count.parse() else {
        return (500, Some((count, it.collect())));
    };
    (count, it.next().map(|x| (x, it.collect())))
}

fn main() -> std::io::Result<()> {
    let (count, cmd) = parse_args();
    let bind_addr = SocketAddrV6::new(Ipv6Addr::LOCALHOST, 0, 0, 0);
    let listener = TcpListener::bind(&bind_addr)?;
    let local_addr = listener.local_addr()?;
    eprintln!("listening on {}", local_addr);
    let conns = (0..count)
        .map(|_| {
            let client = TcpStream::connect(local_addr)?;
            client.set_linger(Some(Duration::from_secs(0)))?;
            let (server, _) = listener.accept()?;
            server.set_linger(Some(Duration::from_secs(0)))?;
            Ok([client, server])
        })
        .collect::<std::io::Result<Vec<_>>>()?;
    eprintln!("opened {} connections", conns.len());

    if let Some((cmd, mut args)) = cmd {
        args.push(format!("{}", local_addr.port()));
        let ex = std::process::Command::new(cmd).args(args).spawn()?.wait()?;
        std::process::exit(ex.code().unwrap_or(1))
    } else {
        loop {
            std::thread::park();
        }
    }
}
