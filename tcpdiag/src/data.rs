use std::str::FromStr;

use serde::{Deserialize, Serialize};
use zerocopy::{FromBytes, Immutable, IntoBytes, KnownLayout};

use crate::integer::{wrapper_traits, NlU64, U16BE, U64NE};
use serde_context::SerializeWithContext;

use csv::{Csv, CsvWrite};

/* Modifiers to GET request */
pub const NLM_F_ROOT: u16 = 0x100; /* specify tree root */
pub const NLM_F_MATCH: u16 = 0x200; /* return all matching */
pub const NLM_F_DUMP: u16 = NLM_F_ROOT | NLM_F_MATCH;

pub const NLM_F_REQUEST: u16 = 1; /* It is request message. */

pub const NLMSG_ERROR: u16 = 0x2;
pub const NLMSG_DONE: u16 = 0x3;

pub const SOCK_DIAG_BY_FAMILY: u16 = 20;
pub const INET_DIAG_INFO: u16 = 2;
pub const INET_DIAG_VEGASINFO: u16 = 3;
pub const INET_DIAG_CONG: u16 = 4;
pub const INET_DIAG_BBRINFO: u16 = 16;

pub const TCP_ESTABLISHED: u8 = 1;
pub const TCPF_ESTABLISHED: u32 = 1 << TCP_ESTABLISHED;

pub const fn request_as(extension: u16) -> u8 {
    match extension {
        1..=8 => 1u8 << (extension - 1),
        INET_DIAG_BBRINFO => request_as(INET_DIAG_VEGASINFO),
        _ => unimplemented!(),
    }
}

#[derive(
    KnownLayout, Immutable, FromBytes, IntoBytes, Default, Debug, Clone, Copy, PartialEq, Eq, Hash,
)]
pub struct IpAddrUnspec([u8; 16]);

impl SerializeWithContext for IpAddrUnspec {
    type Context = u8;
    fn serialize<S: serde::Serializer>(
        &self,
        context: &u8,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        match context {
            2 => {
                let [a, b, c, d, ..] = self.0;
                std::net::Ipv4Addr::new(a, b, c, d).serialize(serializer)
            }
            10 => std::net::Ipv6Addr::from(self.0).serialize(serializer),
            _ => panic!(),
        }
    }
}

impl<'de> Deserialize<'de> for IpAddrUnspec {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        match std::net::IpAddr::deserialize(deserializer)? {
            std::net::IpAddr::V6(addr) => Ok(addr.into()),
            std::net::IpAddr::V4(addr) => Ok(addr.into()),
        }
    }
}

impl From<std::net::Ipv6Addr> for IpAddrUnspec {
    fn from(value: std::net::Ipv6Addr) -> Self {
        Self(value.octets())
    }
}

impl From<std::net::Ipv4Addr> for IpAddrUnspec {
    fn from(value: std::net::Ipv4Addr) -> Self {
        Self({
            let mut octets = [0; 16];
            octets[..4].copy_from_slice(&value.octets());
            octets
        })
    }
}

impl From<std::net::IpAddr> for IpAddrUnspec {
    fn from(value: std::net::IpAddr) -> Self {
        match value {
            std::net::IpAddr::V4(v4) => v4.into(),
            std::net::IpAddr::V6(v6) => v6.into(),
        }
    }
}

impl csv::CsvWrite for IpAddrUnspec {
    type Context = u8;
    const DESC: csv::Desc = csv::Desc::Atom;
    fn write<W: std::io::Write>(obj: &Self, ctx: &Self::Context, w: &mut W) {
        match ctx {
            2 => {
                let [a, b, c, d, ..] = obj.0;
                write!(w, "{}", std::net::Ipv4Addr::new(a, b, c, d)).unwrap()
            }
            10 => write!(w, "{}", std::net::Ipv6Addr::from(obj.0)).unwrap(),
            _ => panic!(),
        }
    }
}
impl csv::Csv for IpAddrUnspec {
    fn read<'a, I: Iterator<Item = &'a str>>(r: &mut I) -> Self {
        match std::net::IpAddr::from_str(r.next().unwrap()).unwrap() {
            std::net::IpAddr::V6(addr) => Self(addr.octets()),
            std::net::IpAddr::V4(addr) => Self({
                let mut octets = [0; 16];
                octets[..4].copy_from_slice(&addr.octets());
                octets
            }),
        }
    }
}

#[derive(
    KnownLayout,
    Immutable,
    FromBytes,
    IntoBytes,
    Default,
    Debug,
    SerializeWithContext,
    Deserialize,
    Csv,
)]
#[repr(C)]
#[context(family: u8)]
pub struct InetDiagSockid {
    pub sport: U16BE,
    pub dport: U16BE,

    #[pass(family)]
    pub src: IpAddrUnspec,
    #[pass(family)]
    pub dst: IpAddrUnspec,

    pub ifindex: u32,
    pub cookie: NlU64,
}

#[derive(KnownLayout, Immutable, FromBytes, IntoBytes, Default, Debug)]
#[repr(C)]
pub struct nlmsghdr {
    pub nlmsg_len: u32,
    pub nlmsg_type: u16,
    pub nlmsg_flags: u16,
    pub nlmsg_seq: u32,
    pub nlmsg_pid: u32,
}

#[derive(KnownLayout, Immutable, FromBytes)]
#[repr(C)]
pub struct nlmsg {
    pub hdr: nlmsghdr,
    pub data: [u8],
}

pub struct NlmsgIter<'a>(&'a [u8]);

impl<'a> NlmsgIter<'a> {
    pub fn new(bytes: &'a [u8]) -> Self {
        Self(bytes)
    }
}

impl<'a> Iterator for NlmsgIter<'a> {
    type Item = &'a nlmsg;

    fn next(&mut self) -> Option<Self::Item> {
        if self.0.is_empty() {
            return None;
        }
        let (hdr, _) = nlmsghdr::ref_from_prefix(self.0).unwrap();
        let (a, b) = self.0.split_at(usize::try_from(hdr.nlmsg_len).unwrap());
        self.0 = b;
        Some(nlmsg::ref_from_bytes(a).unwrap())
    }
}

pub struct NlattrIter<'a>(&'a [u8]);

impl<'a> NlattrIter<'a> {
    pub fn new(bytes: &'a [u8]) -> Self {
        Self(bytes)
    }
}

impl<'a> Iterator for NlattrIter<'a> {
    type Item = &'a NlAttribute;

    fn next(&mut self) -> Option<Self::Item> {
        if self.0.is_empty() {
            return None;
        }
        let (hdr, _) = nlattr::ref_from_prefix(self.0).unwrap();
        let len = usize::from(hdr.nla_len);
        let payload_len = len - std::mem::size_of_val(hdr);
        let (current, remaining) = self.0.split_at((len + 3) & !3);
        self.0 = remaining;
        Some(
            NlAttribute::ref_from_prefix_with_elems(current, payload_len)
                .unwrap()
                .0,
        )
    }
}

#[derive(KnownLayout, Immutable, FromBytes, IntoBytes, Default, Debug)]
#[repr(C)]
pub struct nlattr {
    pub nla_len: u16,
    pub nla_type: u16,
}

#[derive(KnownLayout, Immutable, FromBytes, Debug)]
#[repr(C)]
pub struct NlAttribute {
    pub hdr: nlattr,
    pub data: [u8],
}

#[derive(KnownLayout, Immutable, FromBytes, IntoBytes, Default, Csv, SerializeWithContext)]
#[repr(C)]
pub struct InetDiagReqV2 {
    pub family: u8,
    pub protocol: u8,
    pub ext: u8,
    pub pad: u8,
    pub states: u32,

    #[pass(family)]
    pub id: InetDiagSockid,
}

#[derive(KnownLayout, Immutable, FromBytes, IntoBytes)]
#[repr(C)]
pub struct Encap {
    pub hdr: nlmsghdr,
    pub data: InetDiagReqV2,
}

#[derive(Debug, Serialize, Deserialize, Csv)]
pub struct WscaleExp {
    pub snd: u8,
    pub rcv: u8,
}

#[derive(Clone, Copy, KnownLayout, Immutable, FromBytes, IntoBytes)]
#[repr(transparent)]
pub struct Wscale(u8);

impl Wscale {
    pub fn get(self) -> WscaleExp {
        WscaleExp {
            snd: self.0 & 0xf,
            rcv: self.0 >> 4,
        }
    }
    pub fn new(val: WscaleExp) -> Self {
        Self(val.snd & 0xf | val.rcv << 4)
    }
}

wrapper_traits!(Wscale, WscaleExp);

impl csv::CsvWrite for Wscale {
    type Context = ();
    const DESC: csv::Desc = WscaleExp::DESC;
    fn write<W: std::io::Write>(obj: &Self, ctx: &Self::Context, w: &mut W) {
        WscaleExp::write(&obj.get(), ctx, w);
    }
}
impl csv::Csv for Wscale {
    fn read<'a, I: Iterator<Item = &'a str>>(r: &mut I) -> Self {
        Self::new(WscaleExp::read(r))
    }
}

#[derive(
    KnownLayout,
    Immutable,
    FromBytes,
    IntoBytes,
    Debug,
    Default,
    Deserialize,
    Csv,
    SerializeWithContext,
)]
#[repr(C)]
pub struct InetDiagMsg {
    pub family: u8,
    pub state: u8,
    pub timer: u8,
    pub retrans: u8,

    #[pass(family)]
    pub id: InetDiagSockid,

    pub expires: u32,
    pub rqueue: u32,
    pub wqueue: u32,
    pub uid: u32,
    pub inode: u32,
}

#[derive(KnownLayout, Immutable, FromBytes, IntoBytes, Debug, Serialize, Deserialize, Csv)]
#[repr(C)]
pub struct TcpInfo {
    pub state: u8,
    pub ca_state: u8,
    pub retransmits: u8,
    pub probes: u8,
    pub backoff: u8,
    pub options: u8,
    pub wscale: Wscale,
    pub flags: u8,
    pub rto: u32,
    pub ato: u32,
    pub snd_mss: u32,
    pub rcv_mss: u32,
    pub unacked: u32,
    pub sacked: u32,
    pub lost: u32,
    pub retrans: u32,
    pub fackets: u32,
    pub last_data_sent: u32,
    pub last_ack_sent: u32,
    pub last_data_recv: u32,
    pub last_ack_recv: u32,
    pub pmtu: u32,
    pub rcv_ssthresh: u32,
    pub rtt: u32,
    pub rttvar: u32,
    pub snd_ssthresh: u32,
    pub snd_cwnd: u32,
    pub advmss: u32,
    pub reordering: u32,
    pub rcv_rtt: u32,
    pub rcv_space: u32,
    pub total_retrans: u32,
    pub pacing_rate: U64NE,
    pub max_pacing_rate: U64NE,
    pub bytes_acked: U64NE,
    pub bytes_received: U64NE,
    pub segs_out: u32,
    pub segs_in: u32,
    pub notsent_bytes: u32,
    pub min_rtt: u32,
    pub data_segs_in: u32,
    pub data_segs_out: u32,
    pub delivery_rate: U64NE,
    pub busy_time: U64NE,
    pub rwnd_limited: U64NE,
    pub sndbuf_limited: U64NE,
    pub delivered: u32,
    pub delivered_ce: u32,
    pub bytes_sent: U64NE,
    pub bytes_retrans: U64NE,
    pub dsack_dups: u32,
    pub reord_seen: u32,
    pub rcv_ooopack: u32,
    pub snd_wnd: u32,
}

#[derive(KnownLayout, Immutable, FromBytes, IntoBytes, Debug, Serialize, Deserialize, Csv)]
#[repr(C)]
pub struct BbrInfo {
    pub bw: NlU64,
    pub min_rtt: u32,
    pub pacing_gain: u32,
    pub cwnd_gain: u32,
}

#[derive(KnownLayout, Immutable, FromBytes, IntoBytes, Debug, Serialize, Deserialize, Csv)]
#[repr(C)]
pub struct Bbr3Info {
    pub bw_hi: NlU64, /* bw_hi */
    pub bw_lo: NlU64, /* bw_lo */
    pub mode: u8,     /* current bbr_mode in state machine */
    pub phase: u8,    /* current state machine phase */

    #[serde(skip)]
    #[csv(type(csv::Skip))]
    unused1: u8, /* alignment padding; not used yet */

    pub version: u8,      /* BBR algorithm version */
    pub inflight_lo: u32, /* lower short-term data volume bound */
    pub inflight_hi: u32, /* higher long-term data volume bound */
    pub extra_acked: u32, /* max excess packets ACKed in epoch */
}

#[derive(Debug, Serialize, CsvWrite)]
#[non_exhaustive]
pub struct InetDiagMsgExtra<'a> {
    pub base: &'a InetDiagMsg,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cong: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tcp_info: Option<&'a TcpInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bbr: Option<&'a BbrInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bbr3: Option<&'a Bbr3Info>,
}

#[derive(Debug, Serialize, Deserialize, Csv)]
#[non_exhaustive]
pub struct InetDiagMsgExtraOwned {
    pub base: InetDiagMsg,
    pub cong: Option<String>,
    pub tcp_info: Option<TcpInfo>,
    pub bbr: Option<BbrInfo>,
    pub bbr3: Option<Bbr3Info>,
}

impl InetDiagMsgExtraOwned {
    fn push_header(buf: &mut Vec<u8>, ty: u16, len: usize) {
        buf.extend(
            nlattr {
                nla_len: u16::try_from(std::mem::size_of::<nlattr>() + len).unwrap(),
                nla_type: ty,
            }
            .as_bytes(),
        )
    }
    pub fn to_vec(&self) -> Vec<u8> {
        let mut buf: Vec<u8> = vec![];
        buf.extend(self.base.as_bytes());
        if let Some(cong) = &self.cong {
            Self::push_header(&mut buf, INET_DIAG_CONG, cong.len() + 1);
            buf.extend(cong.as_bytes());
            buf.push(0);
            while buf.len() & 3 != 0 {
                buf.push(0);
            }
        }
        if let Some(tcp_info) = &self.tcp_info {
            Self::push_header(&mut buf, INET_DIAG_INFO, std::mem::size_of_val(tcp_info));
            buf.extend(tcp_info.as_bytes());
        }
        if let Some(bbr) = &self.bbr {
            let parts = [
                bbr.as_bytes(),
                self.bbr3.as_ref().map(|x| x.as_bytes()).unwrap_or(&[]),
            ];
            Self::push_header(
                &mut buf,
                INET_DIAG_BBRINFO,
                parts.iter().map(|p| p.len()).sum(),
            );
            for part in parts {
                buf.extend(part);
            }
        }
        buf
    }
}

impl<'a> InetDiagMsgExtra<'a> {
    pub fn new(base: &'a InetDiagMsg) -> Self {
        Self {
            base,
            cong: None,
            tcp_info: None,
            bbr: None,
            bbr3: None,
        }
    }

    pub fn parse(data: &'a [u8]) -> Self {
        let (diag, extra) = InetDiagMsg::ref_from_prefix(data).unwrap();
        let mut extras = InetDiagMsgExtra::new(diag);

        for attribute in NlattrIter::new(extra) {
            use crate::data;
            match attribute.hdr.nla_type {
                data::INET_DIAG_INFO => {
                    extras.tcp_info = Some(TcpInfo::ref_from_prefix(&attribute.data).unwrap().0)
                }
                data::INET_DIAG_CONG => {
                    extras.cong = Some(
                        std::str::from_utf8(&attribute.data)
                            .unwrap()
                            .strip_suffix('\0')
                            .unwrap(),
                    )
                }
                data::INET_DIAG_BBRINFO => {
                    if let Ok((bbr, tail)) = BbrInfo::ref_from_prefix(&attribute.data) {
                        extras.bbr = Some(bbr);
                        extras.bbr3 = Bbr3Info::ref_from_prefix(tail).ok().map(|(bbr3, _)| bbr3);
                    }
                }
                _ => (),
            }
        }

        extras
    }
}
