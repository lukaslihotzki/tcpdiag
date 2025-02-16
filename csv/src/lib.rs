use std::io;

#[derive(Clone, Copy, Debug)]
pub enum Desc {
    Option(&'static Desc),
    Array(usize, &'static Desc),
    Struct(&'static [(&'static str, &'static Desc)]),
    Atom,
}

impl Desc {
    pub const fn len(&self) -> usize {
        match *self {
            Desc::Option(d) => d.len(),
            Desc::Array(n, d) => n * d.len(),
            Desc::Struct(m) => {
                let mut i = 0;
                let mut sum = 0;
                while i < m.len() {
                    sum += m[i].1.len();
                    i += 1;
                }
                sum
            }
            Desc::Atom => 1,
        }
    }
    pub const fn is_empty(&self) -> bool {
        self.len() == 0
    }
    pub const fn desc_size(&self) -> usize {
        match *self {
            Desc::Option(d) => d.desc_size(),
            Desc::Array(n, d) => n * (2 + d.desc_size() + (n + 1).ilog10() as usize),
            Desc::Struct(m) => {
                let mut o = 0;
                let mut i = 0;
                while i < m.len() {
                    o += (m[i].0.len() + 1) * m[i].1.len() + m[i].1.desc_size();
                    i += 1;
                }
                o
            }
            Desc::Atom => 1,
        }
    }
}

pub trait CsvWrite<T = Self>
where
    T: ?Sized,
{
    type Context;
    const DESC: Desc;
    fn write<W: io::Write>(obj: &T, ctx: &Self::Context, w: &mut W);
}

pub trait Csv<T = Self>: CsvWrite<T>
where
    T: Sized,
{
    fn read<'a, I: Iterator<Item = &'a str>>(r: &mut I) -> T;
}

impl CsvWrite for String {
    type Context = ();

    const DESC: Desc = Desc::Atom;

    fn write<W: io::Write>(obj: &Self, (): &Self::Context, f: &mut W) {
        write!(f, "{obj}").unwrap();
    }
}
impl CsvWrite for str {
    type Context = ();

    const DESC: Desc = Desc::Atom;

    fn write<W: io::Write>(obj: &Self, (): &Self::Context, f: &mut W) {
        write!(f, "{obj}").unwrap();
    }
}
impl Csv for String {
    fn read<'a, I: Iterator<Item = &'a str>>(r: &mut I) -> Self {
        r.next().unwrap().parse().unwrap()
    }
}

macro_rules! iatom {
    ($ty:ty) => {
        impl CsvWrite for $ty {
            type Context = ();

            const DESC: Desc = Desc::Atom;

            fn write<W: io::Write>(obj: &Self, (): &Self::Context, f: &mut W) {
                let mut buf = itoa::Buffer::new();
                let s = buf.format(*obj);
                f.write_all(s.as_bytes()).unwrap();
            }
        }
        impl Csv for $ty {
            fn read<'a, I: Iterator<Item = &'a str>>(r: &mut I) -> Self {
                r.next().unwrap().parse().unwrap()
            }
        }
    };
}

iatom!(u8);
iatom!(u16);
iatom!(u32);
iatom!(u64);
iatom!(i8);
iatom!(i16);
iatom!(i32);
iatom!(i64);

impl<T: CsvWrite> CsvWrite for Option<T> {
    type Context = T::Context;
    const DESC: Desc = Desc::Option(&T::DESC);
    fn write<W: io::Write>(obj: &Self, ctx: &Self::Context, w: &mut W) {
        match obj {
            Some(x) => T::write(x, ctx, w),
            None => {
                write!(w, "_").unwrap();
                for _ in 1..T::DESC.len() {
                    write!(w, " _").unwrap();
                }
            }
        }
    }
}
impl<T: Csv> Csv for Option<T> {
    fn read<'a, I: Iterator<Item = &'a str>>(r: &mut I) -> Self {
        let mut r = r.peekable();
        if *r.peek().unwrap() == "_" {
            r.take(T::DESC.len()).for_each(|_| ());
            None
        } else {
            Some(T::read(&mut r))
        }
    }
}

impl<T: CsvWrite + ?Sized> CsvWrite for &T {
    type Context = T::Context;
    const DESC: Desc = T::DESC;
    fn write<W: io::Write>(obj: &Self, ctx: &Self::Context, w: &mut W) {
        <T as CsvWrite>::write(obj, ctx, w);
    }
}

impl<T: Csv, const N: usize> CsvWrite for [T; N] {
    type Context = T::Context;
    const DESC: Desc = Desc::Array(N, &T::DESC);

    fn write<W: io::Write>(obj: &Self, ctx: &Self::Context, w: &mut W) {
        if let Some(e) = obj.first() {
            T::write(e, ctx, w);
        }
        for e in &obj[1..] {
            write!(w, " ").unwrap();
            T::write(e, ctx, w);
        }
    }
}
impl<T: Csv, const N: usize> Csv for [T; N] {
    fn read<'a, I: Iterator<Item = &'a str>>(r: &mut I) -> Self {
        std::array::from_fn(|_| T::read(r))
    }
}

pub const fn copy(src: &[u8], dst: &mut [u8], shift: usize) {
    let mut i = 0;
    while i < src.len() {
        dst[shift + i] = src[i];
        i += 1;
    }
}

pub const fn fieldnamelen(prefix: &[u8], fields: &[&str]) -> usize {
    let mut i = 0;
    let mut o = 0;
    while i < fields.len() {
        o += prefix.len() + fields[i].len();
        i += 1;
    }
    o
}

pub const fn splitarr<'a, const COUNT: usize>(
    split: &[usize; COUNT],
    mut buf: &'a [u8],
) -> [&'a str; COUNT] {
    let mut a = [""; COUNT];
    let mut i = 0;
    let mut o = 0;
    while i < COUNT {
        let end = split[i];
        let (range, rem) = buf.split_at(end - o);
        a[i] = match std::str::from_utf8(range) {
            Ok(s) => s,
            Err(_) => panic!("invalid utf-8"),
        };
        i += 1;
        o = end;
        buf = rem;
    }
    a
}

pub struct Writer<'a> {
    out: &'a mut [u8],
    pos: usize,
}

impl<'a> Writer<'a> {
    pub const fn new(out: &'a mut [u8]) -> Self {
        Self { out, pos: 0 }
    }

    pub const fn extend(&mut self, string: &str) {
        let mut i = 0;
        while i < string.len() {
            self.out[self.pos] = string.as_bytes()[i];
            i += 1;
            self.pos += 1;
        }
    }

    pub const fn num(&mut self, mut number: usize) {
        let pos1 = self.pos;
        // output digits backwards (easier to compute)
        while number != 0 || pos1 == self.pos {
            self.out[self.pos] = b'0' + (number % 10) as u8;
            self.pos += 1;
            number /= 10;
        }
        // swap digits to forward ordering
        let mut i = 0;
        while i != (self.pos - pos1) / 2 {
            //self.out.swap(pos1 + i, self.pos - 1 - i);
            let a = self.out[pos1 + i];
            let b = self.out[self.pos - 1 - i];
            self.out[pos1 + i] = b;
            self.out[self.pos - 1 - i] = a;
            i += 1;
        }
    }

    pub const fn get_str(&self) -> &str {
        match std::str::from_utf8(self.out.split_at(self.pos).0) {
            Ok(x) => x,
            Err(_) => panic!("invalid utf-8"),
        }
    }
}

pub struct Skip;

impl<T: Default> CsvWrite<T> for Skip {
    type Context = ();

    const DESC: Desc = Desc::Struct(&[]);

    fn write<W: std::io::Write>(_obj: &T, &(): &Self::Context, _w: &mut W) {}
}
impl<T: Default> Csv<T> for Skip {
    fn read<'a, I: Iterator<Item = &'a str>>(_r: &mut I) -> T {
        Default::default()
    }
}

pub const fn cprint<const N: usize>(write: &mut Writer<'_>, prefix: &str, desc: &Desc) {
    match *desc {
        Desc::Option(d) => cprint::<N>(write, prefix, d),
        Desc::Array(n, d) => {
            let mut i = 0;
            while i < n {
                let mut buf = [0; N];
                let mut bwriter = Writer::new(&mut buf);
                bwriter.extend(prefix);
                if !prefix.is_empty() {
                    bwriter.extend(".");
                }
                bwriter.num(i);
                cprint::<N>(write, bwriter.get_str(), d);
                i += 1;
            }
        }
        Desc::Struct(m) => {
            let mut i = 0;
            while i < m.len() {
                let mut buf = [0; N];
                let mut bwriter = Writer::new(&mut buf);
                bwriter.extend(prefix);
                if !prefix.is_empty() && !m[i].0.is_empty() {
                    bwriter.extend(".");
                }
                bwriter.extend(m[i].0);
                cprint::<N>(write, bwriter.get_str(), m[i].1);
                i += 1;
            }
        }
        Desc::Atom => {
            write.extend(prefix);
            write.extend(" ");
        }
    }
}

pub const fn post_process(mut string: &[u8]) -> &str {
    while string[string.len() - 1] == 0 {
        string = string.split_at(string.len() - 1).0;
    }
    match std::str::from_utf8(string) {
        Ok(x) => x,
        Err(_) => panic!(),
    }
}
