use std::{
    io::{BufReader, Read, StdinLock, Write},
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use zerocopy::IntoBytes;

use crate::data::*;

use crate::Collector;

pub struct BinaryOutput<T: Write> {
    writer: T,
}

crate::impl_output!(BinaryOutput<T>);

impl<T: Write> BinaryOutput<T> {
    pub fn new(writer: T) -> Self {
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

impl<T: Write> Collector for BinaryOutput<T> {
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

pub fn read_binary(mut reader: BufReader<StdinLock>, mut writer: Box<dyn Collector>) {
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
