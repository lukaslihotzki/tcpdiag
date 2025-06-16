use clap::Parser;

pub mod binary;
pub mod csv;
pub mod data;
pub mod integer;
pub mod json;
pub mod timespec;

use netlink_sys::{protocols::NETLINK_SOCK_DIAG, Socket, SocketAddr};
use std::{
    num::NonZeroU32,
    time::{Duration, Instant, SystemTime},
};
use timespec::Timespec;
use zerocopy::IntoBytes;

use data::*;
use integer::U16BE;

pub trait Output {
    fn out(&mut self, data: &[u8]);
    fn start(&mut self, time: SystemTime);
    fn end(&mut self, duration: Duration);
}

#[derive(Parser, Debug)]
#[group(id = "netlink")]
pub struct NetlinkArgs {
    #[arg(conflicts_with = "inet6", short = '4')]
    pub inet4: bool,
    #[arg(conflicts_with = "inet4", short = '6')]
    pub inet6: bool,
    #[arg(short = 's', long, default_value_t = 0)]
    pub sport: u16,
    #[arg(short = 'd', long, default_value_t = 0)]
    pub dport: u16,
    #[arg(short = 'a', long)]
    pub all_states: bool,
    #[arg(short = 'x', long)]
    pub all_extensions: bool,
    #[arg(short = 'p')]
    pub period: Option<f64>,
    #[arg(requires = "period", short = 'c')]
    pub count: Option<std::num::NonZeroU32>,
}

fn send_request(sock: &Socket, args: &NetlinkArgs, family: u8) {
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

pub fn read_netlink(args: &NetlinkArgs, mut writer: Box<dyn Output>) {
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
