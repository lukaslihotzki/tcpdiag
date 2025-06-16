mod binary;
mod csv;
mod data;
mod integer;
mod json;
mod timespec;

use netlink_sys::{protocols::NETLINK_SOCK_DIAG, Socket, SocketAddr};
use std::{
    io::{BufRead, BufReader, BufWriter},
    num::NonZeroU32,
    time::{Duration, Instant, SystemTime},
};
use timespec::Timespec;
use zerocopy::IntoBytes;

use binary::{read_binary, BinaryOutput};
use csv::{read_csv, CsvOutput};
use data::*;
use integer::U16BE;
use json::{read_json, JsonOutput};

fn send_request(sock: &Socket, args: &Args, family: u8) {
    let msg = Encap {
        hdr: nlmsghdr {
            nlmsg_len: std::mem::size_of::<Encap>().try_into().unwrap(),
            nlmsg_flags: NLM_F_DUMP | NLM_F_REQUEST,
            nlmsg_type: SOCK_DIAG_BY_FAMILY,
            ..Default::default()
        },
        data: InetDiagReqV2 {
            family,
            protocol: libc::IPPROTO_TCP.try_into().unwrap(),
            ext: if args.all_extensions {
                u8::MAX
            } else {
                const {
                    data::request_as(data::INET_DIAG_INFO)
                        | data::request_as(data::INET_DIAG_CONG)
                        | data::request_as(data::INET_DIAG_BBRINFO)
                }
            },
            pad: 0,
            states: if args.all_states {
                u32::MAX
            } else {
                data::TCPF_ESTABLISHED
            },
            id: InetDiagSockid {
                sport: U16BE::new(args.sport),
                dport: U16BE::new(args.dport),
                ..Default::default() // kernel ignores src, dst, and ifindex
            },
        },
    };
    sock.send_to(msg.as_bytes(), &SocketAddr::new(0, 0), 0)
        .unwrap();
}

use clap::Parser;

#[derive(Clone, Copy, Debug, clap::ValueEnum)]
enum Format {
    Binary,
    Json,
    Csv,
}

#[derive(Parser, Debug)]
struct Args {
    #[arg(conflicts_with = "convert", conflicts_with = "inet6", short = '4')]
    inet4: bool,
    #[arg(conflicts_with = "convert", conflicts_with = "inet4", short = '6')]
    inet6: bool,
    #[arg(conflicts_with = "convert", short = 's', long, default_value_t = 0)]
    sport: u16,
    #[arg(conflicts_with = "convert", short = 'd', long, default_value_t = 0)]
    dport: u16,
    #[arg(conflicts_with = "convert", short = 'a', long)]
    all_states: bool,
    #[arg(conflicts_with = "convert", short = 'x', long)]
    all_extensions: bool,
    #[arg(conflicts_with = "convert", short = 'p')]
    period: Option<f64>,
    #[arg(conflicts_with = "convert", requires = "period", short = 'c')]
    count: Option<std::num::NonZeroU32>,
    #[arg(short = 'o', default_value = "json")]
    output: Format,
    #[arg(short = 'C', long)]
    convert: bool,
}

trait Output {
    fn out(&mut self, data: &[u8]);
    fn start(&mut self, time: SystemTime);
    fn end(&mut self, duration: Duration);
}

fn read_netlink(args: &Args, mut writer: Box<dyn Output>) {
    let s = Socket::new(NETLINK_SOCK_DIAG).unwrap();

    let mut buf = Vec::with_capacity(1 << 18);
    let mut count = args.count.map(NonZeroU32::get).unwrap_or(0);

    let mut period_start = Timespec::now();
    loop {
        let start = Instant::now();
        let time = SystemTime::now();
        writer.start(time);
        let address_families: &[u8] = match () {
            _ if args.inet4 => &[libc::AF_INET.try_into().unwrap()],
            _ if args.inet6 => &[libc::AF_INET6.try_into().unwrap()],
            _ => &[
                libc::AF_INET.try_into().unwrap(),
                libc::AF_INET6.try_into().unwrap(),
            ],
        };
        for &address_family in address_families {
            send_request(&s, args, address_family);
            'a: loop {
                buf.clear();
                s.recv_from(&mut buf, 0).unwrap();
                for nlmsg in NlmsgIter::new(&buf[..]) {
                    if nlmsg.hdr.nlmsg_type == NLMSG_DONE || nlmsg.hdr.nlmsg_type == NLMSG_ERROR {
                        break 'a;
                    }
                    if nlmsg.hdr.nlmsg_type == SOCK_DIAG_BY_FAMILY {
                        writer.out(&nlmsg.data);
                    }
                }
            }
        }
        writer.end(start.elapsed());
        if count != 0 {
            count -= 1;
            if count == 0 {
                break;
            }
        }

        (if let Some(p) = args.period {
            period_start += Duration::from_secs_f64(p);
            period_start
        } else {
            break;
        })
        .sleep_until();
    }
}

fn main() {
    let args = Args::parse();

    let stdout = BufWriter::new(std::io::stdout().lock());
    let writer: Box<dyn Output> = match args.output {
        Format::Json => Box::new(JsonOutput::new(stdout)),
        Format::Binary => Box::new(BinaryOutput::new(stdout)),
        Format::Csv => Box::new(CsvOutput::new(stdout)),
    };

    if args.convert {
        let mut reader = BufReader::new(std::io::stdin().lock());
        let peek = reader.fill_buf().unwrap();
        const A: u8 = 1u16.to_ne_bytes()[0];
        const B: u8 = 1u16.to_ne_bytes()[1];
        match *peek {
            [_, _, A, B, ..] => read_binary(reader, writer),
            [_, _, B, A, ..] => unimplemented!("foreign endianness"),
            [b'{', b'"', ..] => read_json(reader, writer),
            [b'#' | b'a'..=b'z', ..] => read_csv(reader, writer),
            [] => (),
            _ => panic!("unrecognized format"),
        }
    } else {
        read_netlink(&args, writer);
    }
}
