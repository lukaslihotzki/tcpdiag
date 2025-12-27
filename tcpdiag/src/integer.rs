use serde::{Deserialize, Serialize};
use zerocopy::{FromBytes, Immutable, IntoBytes, KnownLayout};

macro_rules! wrapper_traits {
    ($name: ident, $raw: ty) => {
        impl Serialize for $name {
            fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                self.get().serialize(serializer)
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                Ok(Self::new(Deserialize::deserialize(deserializer)?))
            }
        }

        impl std::fmt::Debug for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                self.get().fmt(f)
            }
        }
    };
}

impl csv::CsvWrite for NlU64 {
    type Context = ();
    const DESC: csv::Desc = csv::Desc::Atom;
    fn write<W: std::io::Write>(obj: &Self, ctx: &Self::Context, w: &mut W) {
        u64::write(&obj.get(), ctx, w);
    }
}
impl csv::Csv for NlU64 {
    fn read<'a, I: Iterator<Item = &'a str>>(r: &mut I) -> Self {
        Self::new(u64::read(r))
    }
}

pub(crate) use wrapper_traits;

#[derive(Copy, Clone, KnownLayout, Immutable, FromBytes, IntoBytes, Default)]
pub struct NlU64([u32; 2]);

impl NlU64 {
    pub fn get(self) -> u64 {
        let Self([lsb, msb]) = self;
        u64::from(lsb) | u64::from(msb) << 32
    }
    pub fn new(val: u64) -> Self {
        Self([val as u32, (val >> 32) as u32])
    }
}

wrapper_traits!(NlU64, [u32; 2]);

macro_rules! wrapper {
    ($name: ident, $mem: ty, $raw: ty, $from: expr, $to: expr) => {
        #[derive(Copy, Clone, Default, KnownLayout, Immutable, FromBytes, IntoBytes)]
        pub struct $name($mem);

        impl $name {
            pub fn get(self) -> $raw {
                $to(self.0)
            }
            pub fn new(val: $raw) -> Self {
                Self($from(val))
            }
        }

        impl csv::CsvWrite for $name {
            type Context = ();
            const DESC: csv::Desc = csv::Desc::Atom;
            fn write<W: std::io::Write>(obj: &Self, ctx: &Self::Context, w: &mut W) {
                <$raw>::write(&obj.get(), ctx, w);
            }
        }
        impl csv::Csv for $name {
            fn read<'a, I: Iterator<Item = &'a str>>(r: &mut I) -> Self {
                Self::new(<$raw>::read(r))
            }
        }

        wrapper_traits!($name, $raw);
    };
}

wrapper!(U16BE, [u8; 2], u16, u16::to_be_bytes, u16::from_be_bytes);
wrapper!(U64NE, [u8; 8], u64, u64::to_ne_bytes, u64::from_ne_bytes);
