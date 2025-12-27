use std::{
    collections::HashMap,
    io::{BufRead, BufReader, StdinLock, Write},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use crate::data::*;

use crate::Collector;
use csv::{Csv, CsvWrite};

pub struct CsvOutput<T: Write> {
    writer: T,
    time: SystemTime,
    trailer: &'static str,
}

crate::impl_output!(CsvOutput<T>);

#[derive(CsvWrite)]
struct CsvLine<'a> {
    time: u64,
    #[csv(flatten())]
    data: Option<InetDiagMsgExtra<'a>>,
}

#[derive(Csv)]
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
    pub fn new(mut writer: T) -> Self {
        writeln!(&mut writer, "{CSV_HEADER}").unwrap();
        Self {
            writer,
            time: UNIX_EPOCH,
            trailer: "",
        }
    }
}

impl<T: Write> Collector for CsvOutput<T> {
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

pub fn read_csv(mut reader: BufReader<StdinLock>, mut writer: Box<dyn Collector>) {
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
