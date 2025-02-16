mod data;
mod integer;
mod serde_context;
mod timespec;

use csv::{Csv, CsvWrite};
use netlink_sys::{protocols::NETLINK_SOCK_DIAG, Socket, SocketAddr};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    io::{BufRead, BufReader, BufWriter, Read, StdinLock, Write},
    num::NonZeroU32,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use timespec::Timespec;
use zerocopy::IntoBytes;

use data::*;
use integer::U16BE;

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

struct BinaryOutput<T: Write> {
    writer: BufWriter<T>,
}

impl<T: Write> BinaryOutput<T> {
    fn new(writer: BufWriter<T>) -> Self {
        Self { writer }
    }

    fn write_ts(&mut self, ty: u16, data: &[u8]) {
        self.push_header(ty, data.len());
        self.writer.write_all(data).unwrap();
    }

    fn push_header(&mut self, ty: u16, len: usize) {
        self.writer
            .write_all(
                nlattr {
                    nla_len: u16::try_from(std::mem::size_of::<nlattr>() + len).unwrap(),
                    nla_type: ty,
                }
                .as_bytes(),
            )
            .unwrap()
    }
}

impl<T: Write> Output for BinaryOutput<T> {
    fn out(&mut self, data: &[u8]) {
        self.push_header(0, data.len());
        self.writer.write_all(data).unwrap();
    }

    fn start(&mut self, time: SystemTime) {
        let ts = time.duration_since(UNIX_EPOCH).unwrap().as_micros() as u64;
        self.write_ts(1, &ts.to_ne_bytes())
    }

    fn end(&mut self, duration: Duration) {
        self.write_ts(2, u32::try_from(duration.as_micros()).unwrap().as_bytes());
        self.writer.flush().unwrap();
    }
}

struct JsonOutput<T: Write> {
    writer: BufWriter<T>,
    comma: &'static str,
}

impl<T: Write> JsonOutput<T> {
    fn new(writer: BufWriter<T>) -> Self {
        Self { writer, comma: "" }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Action {
    Start,
    End,
}

#[derive(Serialize, Deserialize)]
struct Ts {
    action: Action,
    time: u64,
}

impl<T: Write> Output for JsonOutput<T> {
    fn start(&mut self, time: SystemTime) {
        let time = time.duration_since(UNIX_EPOCH).unwrap().as_micros() as u64;
        write!(&mut self.writer, "{{\"time\":{time},\"samples\":[").unwrap();
        self.comma = "";
    }

    fn end(&mut self, duration: Duration) {
        let time = duration.as_micros() as u64;
        writeln!(&mut self.writer, "],\"duration\":{time}}}").unwrap();
        self.writer.flush().unwrap();
    }

    fn out(&mut self, data: &[u8]) {
        let extras = InetDiagMsgExtra::parse(data);
        write!(&mut self.writer, "{}", self.comma).unwrap();
        serde_json::to_writer(&mut self.writer, &extras).unwrap();
        self.comma = ",";
    }
}

struct CsvOutput<T: Write> {
    writer: BufWriter<T>,
    time: SystemTime,
    trailer: &'static str,
}

#[derive(csv_derive::CsvWrite)]
struct CsvLine<'a> {
    time: u64,
    #[csv(flatten())]
    data: Option<InetDiagMsgExtra<'a>>,
}

#[derive(csv_derive::Csv)]
struct CsvLineOwned {
    time: u64,
    #[csv(flatten())]
    data: Option<InetDiagMsgExtraOwned>,
    duration: Option<u64>,
}

const CSV_HEADER: &str = csv::post_process(
    &const {
        const DESC: &csv::Desc = &CsvLineOwned::DESC;
        const SIZE: usize = DESC.desc_size();
        let mut out = [0; SIZE];
        let mut writer = csv::Writer::new(&mut out);
        csv::cprint::<SIZE>(&mut writer, "", DESC);
        out
    },
);

impl<T: Write> CsvOutput<T> {
    fn new(mut writer: BufWriter<T>) -> Self {
        writeln!(&mut writer, "{CSV_HEADER}").unwrap();
        Self {
            writer,
            time: UNIX_EPOCH,
            trailer: "",
        }
    }
}

impl<T: Write> Output for CsvOutput<T> {
    fn start(&mut self, time: SystemTime) {
        self.time = time;
        self.trailer = "";
    }

    fn out(&mut self, data: &[u8]) {
        write!(&mut self.writer, "{}", self.trailer).unwrap();
        let time = self.time.duration_since(UNIX_EPOCH).unwrap().as_micros();
        let line = CsvLine {
            time: time as u64,
            data: Some(InetDiagMsgExtra::parse(data)),
        };
        CsvLine::write(&line, &(), &mut self.writer);
        write!(&mut self.writer, "").unwrap();
        self.trailer = " _\n";
    }

    fn end(&mut self, duration: Duration) {
        if self.trailer.is_empty() {
            let line = CsvLine {
                time: self.time.duration_since(UNIX_EPOCH).unwrap().as_micros() as u64,
                data: None,
            };
            CsvLine::write(&line, &(), &mut self.writer);
            write!(&mut self.writer, "").unwrap();
        }
        writeln!(&mut self.writer, " {}", duration.as_micros()).unwrap();
        self.writer.flush().unwrap();
    }
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

fn read_binary(mut reader: BufReader<StdinLock>, mut writer: Box<dyn Output>) {
    let mut buf = Vec::new();
    loop {
        let mut attr = nlattr::default();
        let s = reader.read(attr.as_mut_bytes()).unwrap();
        if s == 0 {
            break;
        }
        reader.read_exact(&mut attr.as_mut_bytes()[s..]).unwrap();
        buf.resize(usize::from(attr.nla_len) - std::mem::size_of_val(&attr), 0);
        reader.read_exact(&mut buf[..]).unwrap();
        match attr.nla_type {
            0 => writer.out(&buf[..]),
            1 => {
                let time = u64::from_ne_bytes(buf[..].try_into().unwrap());
                writer.start(UNIX_EPOCH + Duration::from_micros(time));
            }
            2 => {
                let duration = u32::from_ne_bytes(buf[..].try_into().unwrap());
                writer.end(Duration::from_micros(duration.into()));
            }
            _ => panic!(),
        }
    }
}

#[derive(Serialize, Deserialize)]
struct JsonFormat {
    time: u64,
    samples: Vec<InetDiagMsgExtraOwned>,
    duration: u32,
}

fn read_json(mut reader: BufReader<StdinLock>, mut writer: Box<dyn Output>) {
    let mut buf = String::new();
    loop {
        buf.clear();
        reader.read_line(&mut buf).unwrap();
        if buf.is_empty() {
            return;
        }
        let Ok(json): Result<JsonFormat, _> = serde_json::from_str(&buf) else {
            continue;
        };
        writer.start(UNIX_EPOCH + Duration::from_micros(json.time));
        for x in json.samples {
            writer.out(&x.to_vec());
        }
        writer.end(Duration::from_micros(json.duration.into()));
    }
}

fn read_csv(mut reader: BufReader<StdinLock>, mut writer: Box<dyn Output>) {
    let mut header = String::new();
    loop {
        reader.read_line(&mut header).unwrap();
        if header.starts_with('#') {
            header = Default::default();
        } else {
            break;
        }
    }
    let header = header.strip_suffix('\n').unwrap();
    let mut reorder = None;
    if !header.starts_with(CSV_HEADER) {
        let header_map: HashMap<_, _> = header.split(" ").zip(0usize..).collect();
        reorder = Some(
            CSV_HEADER
                .split(" ")
                .map(|k| header_map.get(k).copied())
                .collect::<Vec<_>>(),
        );
    }
    let mut buf = String::new();
    let mut time = UNIX_EPOCH;
    loop {
        buf.clear();
        loop {
            reader.read_line(&mut buf).unwrap();
            if buf.is_empty() {
                return;
            }
            if buf.starts_with('#') {
                buf = Default::default();
            } else {
                break;
            }
        }
        let buf = buf.strip_suffix("\n").unwrap();
        let mut iter = buf.split(' ');
        let mut fields = Vec::new();

        let line = if let Some(reorder) = &reorder {
            fields.clear();
            fields.extend(iter);
            let mut iter = reorder
                .iter()
                .map(|i| i.and_then(|i| fields.get(i)).copied().unwrap_or("_"));
            CsvLineOwned::read(&mut iter)
        } else {
            CsvLineOwned::read(&mut iter)
        };
        let time_new = UNIX_EPOCH + Duration::from_micros(line.time);
        if time != time_new {
            time = time_new;
            writer.start(time);
        }
        if let Some(data) = &line.data {
            writer.out(&data.to_vec());
        }
        if let Some(end) = line.duration {
            writer.end(Duration::from_micros(end));
            continue;
        }
    }
}

fn main() {
    let args = Args::parse();

    let stdout = std::io::BufWriter::new(std::io::stdout().lock());
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
