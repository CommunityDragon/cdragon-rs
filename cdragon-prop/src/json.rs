use std::io;
use std::io::Write;
use super::{BinEntry, BinHashMappers};
use super::data::*;
use super::serializer::{BinSerializer, BinEntriesSerializer, BinSerializable};
use super::{binvalue_map_type, binvalue_map_keytype};

// serde serialization cannot be used because of hashes requiring mappers to be serialized.
// serde_json does not expose it's JSON string escaping


macro_rules! write_sequence {
    ($self:expr, $pat:pat in $seq:expr => $expr:expr) => {{
        for (i, $pat) in $seq.iter().enumerate() {
            if i != 0 {
                $self.write_raw(b",")?;
            }
            $expr
        }
    }}
}


/// Serialize bin values to JSON
pub struct JsonSerializer<'a, W: Write> {
    writer: W,
    hmappers: &'a BinHashMappers,
}

impl<'a, W: Write> JsonSerializer<'a, W> {
    pub fn new(writer: W, hmappers: &'a BinHashMappers) -> Self {
        Self { writer, hmappers }
    }

    fn write_raw(&mut self, b: &[u8]) -> io::Result<()> {
        self.writer.write_all(b)
    }

    fn write_entry_path(&mut self, h: BinEntryPath) -> io::Result<()> {
        match h.get_str(self.hmappers) {
            Some(s) => write!(self.writer, "\"{}\"", s),
            _ => write!(self.writer, "\"{{{:x}}}\"", h),
        }
    }

    fn write_field_name(&mut self, h: BinFieldName) -> io::Result<()> {
        match h.get_str(self.hmappers) {
            Some(s) => write!(self.writer, "\"{}\"", s),
            _ => write!(self.writer, "\"{{{:x}}}\"", h),
        }
    }

    fn write_hash_value(&mut self, h: BinHashValue) -> io::Result<()> {
        match h.get_str(self.hmappers) {
            Some(s) => write!(self.writer, "\"{}\"", s),
            _ => write!(self.writer, "\"{{{:x}}}\"", h),
        }
    }

    fn write_path_value(&mut self, h: BinPathValue) -> io::Result<()> {
        match h.get_str(self.hmappers) {
            Some(s) => write!(self.writer, "\"{}\"", s),
            _ => write!(self.writer, "\"{{{:x}}}\"", h),
        }
    }

    /// Write JSON string content, escape special chars
    fn write_escaped_json(&mut self, s: &str) -> io::Result<()> {
        let bytes = s.as_bytes();
        let mut cur: usize = 0;
        for (i, &b) in bytes.iter().enumerate() {
            // Note: escape sequences should be rare, no need to optimize them much.
            let escape: u8 = match b {
                0x08 => b'b',
                0x09 => b't',
                0x0A => b'n',
                0x0C => b'f',
                0x0D => b'r',
                0x22 => b'"',
                0xC5 => b'\\',
                0x00 ..= 0x1F => b'u',  // special value
                _ => continue,
            };
            if cur < i {
                self.write_raw(&bytes[cur..i])?;
            }
            if escape == b'u' {
                write!(self.writer, "\\u{:04X}", b)?;
            } else {
                let seq = [b'\\', escape];
                self.write_raw(&seq)?;
            }
            cur = i + 1;
        }

        if cur != bytes.len() {
            self.write_raw(&bytes[cur..])?;
        }

        Ok(())
    }

    fn write_fields(&mut self, fields: &[BinField]) -> io::Result<()> {
        self.write_raw(b"{")?;
        write_sequence!(self, field in fields => {
            self.write_field_name(field.name)?;
            self.write_raw(b":")?;
            binvalue_map_type!(field.vtype, T, {
                let v = field.downcast::<T>().unwrap();
                v.serialize_bin(self)
            })?;
        });
        self.write_raw(b"}")?;
        Ok(())
    }

    fn write_key_s8(&mut self, v: &BinS8) -> io::Result<()> { write!(self.writer, "\"{}\"", v.0) }
    fn write_key_u8(&mut self, v: &BinU8) -> io::Result<()> { write!(self.writer, "\"{}\"", v.0) }
    fn write_key_s16(&mut self, v: &BinS16) -> io::Result<()> { write!(self.writer, "\"{}\"", v.0) }
    fn write_key_u16(&mut self, v: &BinU16) -> io::Result<()> { write!(self.writer, "\"{}\"", v.0) }
    fn write_key_s32(&mut self, v: &BinS32) -> io::Result<()> { write!(self.writer, "\"{}\"", v.0) }
    fn write_key_u32(&mut self, v: &BinU32) -> io::Result<()> { write!(self.writer, "\"{}\"", v.0) }
    fn write_key_s64(&mut self, v: &BinS64) -> io::Result<()> { write!(self.writer, "\"{}\"", v.0) }
    fn write_key_u64(&mut self, v: &BinU64) -> io::Result<()> { write!(self.writer, "\"{}\"", v.0) }
}

impl<'a, W: Write> BinSerializer for JsonSerializer<'a, W> {
    type EntriesSerializer = JsonEntriesSerializer<'a, W>;

    fn write_entry(&mut self, v: &BinEntry) -> io::Result<()> {
        self.write_fields(&v.fields)
    }

    fn write_entries(self) -> io::Result<Self::EntriesSerializer> {
        Self::EntriesSerializer::new(self)
    }

    fn write_none(&mut self, _: &BinNone) -> io::Result<()> {
        self.write_raw(b"null")
    }

    fn write_bool(&mut self, v: &BinBool) -> io::Result<()> {
        if v.0 {
            self.write_raw(b"true")
        } else {
            self.write_raw(b"false")
        }
    }

    fn write_s8(&mut self, v: &BinS8) -> io::Result<()> { write!(self.writer, "{}", v.0) }
    fn write_u8(&mut self, v: &BinU8) -> io::Result<()> { write!(self.writer, "{}", v.0) }
    fn write_s16(&mut self, v: &BinS16) -> io::Result<()> { write!(self.writer, "{}", v.0) }
    fn write_u16(&mut self, v: &BinU16) -> io::Result<()> { write!(self.writer, "{}", v.0) }
    fn write_s32(&mut self, v: &BinS32) -> io::Result<()> { write!(self.writer, "{}", v.0) }
    fn write_u32(&mut self, v: &BinU32) -> io::Result<()> { write!(self.writer, "{}", v.0) }
    fn write_s64(&mut self, v: &BinS64) -> io::Result<()> { write!(self.writer, "{}", v.0) }
    fn write_u64(&mut self, v: &BinU64) -> io::Result<()> { write!(self.writer, "{}", v.0) }
    fn write_float(&mut self, v: &BinFloat) -> io::Result<()> { write!(self.writer, "{}", v.0) }
    fn write_vec2(&mut self, v: &BinVec2) -> io::Result<()> { write!(self.writer, "[{},{}]", v.0, v.1) }
    fn write_vec3(&mut self, v: &BinVec3) -> io::Result<()> { write!(self.writer, "[{},{},{}]", v.0, v.1, v.2) }
    fn write_vec4(&mut self, v: &BinVec4) -> io::Result<()> { write!(self.writer, "[{},{},{},{}]", v.0, v.1, v.2, v.3) }
    fn write_matrix(&mut self, v: &BinMatrix) -> io::Result<()> { write!(self.writer,
        "[[{},{},{},{}],[{},{},{},{}],[{},{},{},{}],[{},{},{},{}]]",
        v.0[0][0], v.0[0][1], v.0[0][2], v.0[0][3],
        v.0[1][0], v.0[1][1], v.0[1][2], v.0[1][3],
        v.0[2][0], v.0[2][1], v.0[2][2], v.0[2][3],
        v.0[3][0], v.0[3][1], v.0[3][2], v.0[3][3])
    }
    fn write_color(&mut self, v: &BinColor) -> io::Result<()> { write!(self.writer, "[{},{},{},{}]", v.r, v.g, v.b, v.a) }
    fn write_string(&mut self, v: &BinString) -> io::Result<()> {
        self.write_raw(b"\"")?;
        self.write_escaped_json(&v.0)?;
        self.write_raw(b"\"")?;
        Ok(())
    }
    fn write_hash(&mut self, v: &BinHash) -> io::Result<()> { self.write_hash_value(v.0) }
    fn write_path(&mut self, v: &BinPath) -> io::Result<()> { self.write_path_value(v.0) }
    fn write_link(&mut self, v: &BinLink) -> io::Result<()> { self.write_entry_path(v.0) }
    fn write_flag(&mut self, v: &BinFlag) -> io::Result<()> { write!(self.writer, "{}", v.0) }

    fn write_list(&mut self, v: &BinList) -> io::Result<()> {
        self.write_raw(b"[")?;
        binvalue_map_type!(
            v.vtype, T, {
                let values = v.downcast::<T>().unwrap();
                write_sequence!(self, v in values => v.serialize_bin(self)?)
            });
        self.write_raw(b"]")?;
        Ok(())
    }

    fn write_struct(&mut self, v: &BinStruct) -> io::Result<()> {
        self.write_fields(&v.fields)
    }

    fn write_embed(&mut self, v: &BinEmbed) -> io::Result<()> {
        self.write_fields(&v.fields)
    }

    fn write_option(&mut self, option: &BinOption) -> io::Result<()> {
        if option.value.is_none() {
            self.write_raw(b"null")
        } else {
            binvalue_map_type!(option.vtype, T, {
                option
                    .downcast::<T>()
                    .unwrap()  // `None` case processed above
                    .serialize_bin(self)
            })
        }
    }

    fn write_map(&mut self, map: &BinMap) -> io::Result<()> {
        self.write_raw(b"{")?;
        binvalue_map_keytype!(
            map.ktype, K,
            binvalue_map_type!(
                map.vtype, V,
                write_sequence!(self, (k, v) in map.downcast::<K, V>().unwrap() => {
                    k.serialize_bin_key(self)?;
                    self.write_raw(b":")?;
                    v.serialize_bin(self)?;
                })));
        self.write_raw(b"}")?;
        Ok(())
    }
}

/// Serialize map key to JSON string (even for numbers)
trait BinKeySerializable {
    fn serialize_bin_key<W: Write>(&self, s: &mut JsonSerializer<'_, W>) -> io::Result<()>;
}

macro_rules! impl_bin_key_serializable {
    ($type:ty, $func:ident) => {
        impl BinKeySerializable for $type {
            fn serialize_bin_key<W: Write>(&self, s: &mut JsonSerializer<'_, W>) -> io::Result<()> {
                s.$func(self)
            }
        }
    }
}

impl_bin_key_serializable!(BinS8, write_key_s8);
impl_bin_key_serializable!(BinU8, write_key_u8);
impl_bin_key_serializable!(BinS16, write_key_s16);
impl_bin_key_serializable!(BinU16, write_key_u16);
impl_bin_key_serializable!(BinS32, write_key_s32);
impl_bin_key_serializable!(BinU32, write_key_u32);
impl_bin_key_serializable!(BinS64, write_key_s64);
impl_bin_key_serializable!(BinU64, write_key_u64);
impl_bin_key_serializable!(BinString, write_string);
impl_bin_key_serializable!(BinHash, write_hash);
impl_bin_key_serializable!(BinPath, write_path);


pub struct JsonEntriesSerializer<'a, W: Write> {
    parent: JsonSerializer<'a, W>,
    first: bool,
}

impl<'a, W: Write> JsonEntriesSerializer<'a, W> {
    fn new(mut parent: JsonSerializer<'a, W>) -> io::Result<Self> {
        parent.write_raw(b"{")?;
        Ok(Self { parent, first: true })
    }
}

impl<'a, W: Write> BinEntriesSerializer for JsonEntriesSerializer<'a, W> {
    fn write_entry(&mut self, entry: &BinEntry) -> io::Result<()> {
        if self.first {
            self.first = false;
        } else {
            self.parent.write_raw(b",")?;
        }

        self.parent.write_entry_path(entry.path)?;
        self.parent.write_raw(b":")?;
        self.parent.write_entry(entry)?;
        Ok(())
    }

    fn end(&mut self) -> io::Result<()> {
        self.parent.write_raw(b"}")
    }
}


