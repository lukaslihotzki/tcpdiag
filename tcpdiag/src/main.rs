use clap::Parser;

use std::io::{BufRead, BufReader, BufWriter};
use tcpdiag::binary::{read_binary, BinaryOutput};
use tcpdiag::csv::{read_csv, CsvOutput};
use tcpdiag::json::{read_json, JsonOutput};
use tcpdiag::Output;
use tcpdiag::{read_netlink, NetlinkArgs};

#[derive(Clone, Copy, Debug, clap::ValueEnum)]
pub enum Format {
    Binary,
    Json,
    Csv,
}

#[derive(Parser, Debug)]
struct Args {
    #[command(flatten)]
    netlink: NetlinkArgs,
    #[arg(short = 'o', default_value = "json")]
    output: Format,
    #[arg(conflicts_with = "netlink", short = 'C', long)]
    convert: bool,
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
        read_netlink(&args.netlink, writer);
    }
}
