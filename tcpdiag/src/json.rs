use serde::{Deserialize, Serialize};
use std::{
    io::{BufRead, BufReader, StdinLock, Write},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use crate::data::*;

use crate::Output;

pub struct JsonOutput<T: Write> {
    writer: T,
    comma: &'static str,
}

impl<T: Write> JsonOutput<T> {
    pub fn new(writer: T) -> Self {
        Self { writer, comma: "" }
    }
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

#[derive(Serialize, Deserialize)]
struct JsonFormat {
    time: u64,
    samples: Vec<InetDiagMsgExtraOwned>,
    duration: u32,
}

pub fn read_json(mut reader: BufReader<StdinLock>, mut writer: Box<dyn Output>) {
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
