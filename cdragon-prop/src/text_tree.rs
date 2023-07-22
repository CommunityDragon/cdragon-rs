use std::io;
use std::io::Write;
use super::{
    BinEntry,
    BinHashMappers,
    data::*,
    serializer::{BinSerializer, BinEntriesSerializer, BinSerializable},
    binvalue_map_keytype,
    binvalue_map_type,
};


macro_rules! indented {
    ($s:expr, $b:block) => {{
        $s.indent += 2;
        let result = $b;
        $s.indent -= 2;
        result
    }}
}

macro_rules! serialize {
    ($s:expr, $($arg:tt)*) => (
        write!($s.writer, $($arg)*)
    );
}

macro_rules! serializeln {
    ($s:expr) => (
        write!($s.writer, "\n{i_:iw_$}", i_="", iw_=$s.indent)
    );
    ($s:expr, $fmt:expr) => (
        serialize!($s, concat!("\n{i_:iw_$}", $fmt), i_="", iw_=$s.indent)
    );
    ($s:expr, $fmt:expr, $($arg:tt)*) => (
        serialize!($s, concat!("\n{i_:iw_$}", $fmt), $($arg)*, i_="", iw_=$s.indent)
    );
}


/// Serialize bin values to a human readable text tree
pub struct TextTreeSerializer<'a, W: Write> {
    writer: W,
    hmappers: &'a BinHashMappers,
    indent: usize,
}

impl<'a, W: Write> TextTreeSerializer<'a, W> {
    pub fn new(writer: W, hmappers: &'a BinHashMappers) -> Self {
        Self { writer, hmappers, indent: 0 }
    }

    fn format_entry_path(&self, h: BinEntryPath) -> String {
        match h.get_str(self.hmappers) {
            Some(s) => format!("'{}'", s),
            _ => format!("{{{:x}}}", h),
        }
    }

    fn format_type_name(&self, h: BinClassName) -> String {
        match h.get_str(self.hmappers) {
            Some(s) => s.to_string(),
            _ => format!("{{{:x}}}", h),
        }
    }

    fn format_field_name(&self, h: BinFieldName) -> String {
        match h.get_str(self.hmappers) {
            Some(s) => s.to_string(),
            _ => format!("{{{:x}}}", h),
        }
    }

    fn format_hash_value(&self, h: BinHashValue) -> String {
        match h.get_str(self.hmappers) {
            Some(s) => format!("'{}'", s),
            _ => format!("{{{:x}}}", h),
        }
    }

    fn format_path_value(&self, h: BinPathValue) -> String {
        match h.get_str(self.hmappers) {
            Some(s) => format!("'{}'", s),
            _ => format!("{{{:x}}}", h),
        }
    }

    fn write_fields(&mut self, fields: &[BinField]) -> io::Result<()> {
        if fields.is_empty() {
            serialize!(self, "[]")?;
        } else {
            serialize!(self, "[")?;
            indented!(self, {
                fields.iter().try_for_each(|field| -> io::Result<()> {
                    serializeln!(self, "<{} ", self.format_field_name(field.name))?;
                    self.write_field_content(field)?;
                    serialize!(self, ">")?;
                    Ok(())
                })?;
            });
            serializeln!(self, "]")?;
        }
        Ok(())
    }

    fn write_field_content(&mut self, field: &BinField) -> io::Result<()> {
        macro_rules! serialize_field {
            // Default, for basic bin types
            ($t:ty) => {{
                let v = field.downcast::<$t>().unwrap();
                serialize!(self, "{} ", basic_bintype_name(field.vtype))?;
                v.serialize_bin(self)?;
            }};
            // Nested bin types with fields
            ($t:ty: {$v:ident} => $($fmt:tt)*) => {{
                let $v = field.downcast::<$t>().unwrap();
                serialize!(self, $($fmt)*)?;
                self.write_fields(&$v.fields)?;
            }};
            // Other nested bin types
            ($t:ty: [$v:ident] => $($fmt:tt)*) => {{
                let $v = field.downcast::<$t>().unwrap();
                serialize!(self, $($fmt)*)?;
                $v.serialize_bin(self)?;
            }};
        }

        match field.vtype {
            BinType::None => serialize_field!(BinNone),
            BinType::Bool => serialize_field!(BinBool),
            BinType::S8 => serialize_field!(BinS8),
            BinType::U8 => serialize_field!(BinU8),
            BinType::S16 => serialize_field!(BinS16),
            BinType::U16 => serialize_field!(BinU16),
            BinType::S32 => serialize_field!(BinS32),
            BinType::U32 => serialize_field!(BinU32),
            BinType::S64 => serialize_field!(BinS64),
            BinType::U64 => serialize_field!(BinU64),
            BinType::Float => serialize_field!(BinFloat),
            BinType::Vec2 => serialize_field!(BinVec2),
            BinType::Vec3 => serialize_field!(BinVec3),
            BinType::Vec4 => serialize_field!(BinVec4),
            BinType::Matrix => serialize_field!(BinMatrix),
            BinType::Color => serialize_field!(BinColor),
            BinType::String => serialize_field!(BinString),
            BinType::Hash => serialize_field!(BinHash),
            BinType::Path => serialize_field!(BinPath),
            BinType::List | BinType::List2 => serialize_field!(BinList: [v] => "LIST({}) ", basic_bintype_name(v.vtype)),
            BinType::Struct => serialize_field!(BinStruct: {v} => "STRUCT {} ", self.format_type_name(v.ctype)),
            BinType::Embed => serialize_field!(BinEmbed: {v} => "EMBED {} ", self.format_type_name(v.ctype)),
            BinType::Link => serialize_field!(BinLink),
            BinType::Option => serialize_field!(BinOption: [v] => "OPTION({}) ", basic_bintype_name(v.vtype)),
            BinType::Map => serialize_field!(BinMap: [v] => "MAP({},{}) ", basic_bintype_name(v.ktype), basic_bintype_name(v.vtype)),
            BinType::Flag => serialize_field!(BinFlag),
        }
        Ok(())
    }
}

impl<'a, W: Write> BinSerializer for TextTreeSerializer<'a, W> {
    type EntriesSerializer = TextTreeEntriesSerializer<'a, W>;

    fn write_entry(&mut self, v: &BinEntry) -> io::Result<()> {
        serialize!(self, "<BinEntry {} {} ", self.format_entry_path(v.path), self.format_type_name(v.ctype))?;
        self.write_fields(&v.fields)?;
        serialize!(self, ">")?;
        serializeln!(self)
    }

    fn write_entries(self) -> io::Result<Self::EntriesSerializer> {
        Ok(Self::EntriesSerializer { parent: self })
    }

    fn write_none(&mut self, _: &BinNone) -> io::Result<()> { serialize!(self, "-") }
    fn write_bool(&mut self, v: &BinBool) -> io::Result<()> { serialize!(self, "{}", v.0) }
    fn write_s8(&mut self, v: &BinS8) -> io::Result<()> { serialize!(self, "{}", v.0) }
    fn write_u8(&mut self, v: &BinU8) -> io::Result<()> { serialize!(self, "{}", v.0) }
    fn write_s16(&mut self, v: &BinS16) -> io::Result<()> { serialize!(self, "{}", v.0) }
    fn write_u16(&mut self, v: &BinU16) -> io::Result<()> { serialize!(self, "{}", v.0) }
    fn write_s32(&mut self, v: &BinS32) -> io::Result<()> { serialize!(self, "{}", v.0) }
    fn write_u32(&mut self, v: &BinU32) -> io::Result<()> { serialize!(self, "{}", v.0) }
    fn write_s64(&mut self, v: &BinS64) -> io::Result<()> { serialize!(self, "{}", v.0) }
    fn write_u64(&mut self, v: &BinU64) -> io::Result<()> { serialize!(self, "{}", v.0) }
    fn write_float(&mut self, v: &BinFloat) -> io::Result<()> { serialize!(self, "{}", v.0) }
    fn write_vec2(&mut self, v: &BinVec2) -> io::Result<()> { serialize!(self, "({}, {})", v.0, v.1) }
    fn write_vec3(&mut self, v: &BinVec3) -> io::Result<()> { serialize!(self, "({}, {}, {})", v.0, v.1, v.2) }
    fn write_vec4(&mut self, v: &BinVec4) -> io::Result<()> { serialize!(self, "({}, {}, {}, {})", v.0, v.1, v.2, v.3) }
    fn write_matrix(&mut self, v: &BinMatrix) -> io::Result<()> { serialize!(self,
        "(({}, {}, {}, {}), ({}, {}, {}, {}), ({}, {}, {}, {}), ({}, {}, {}, {}))",
        v.0[0][0], v.0[0][1], v.0[0][2], v.0[0][3],
        v.0[1][0], v.0[1][1], v.0[1][2], v.0[1][3],
        v.0[2][0], v.0[2][1], v.0[2][2], v.0[2][3],
        v.0[3][0], v.0[3][1], v.0[3][2], v.0[3][3]) }
    fn write_color(&mut self, v: &BinColor) -> io::Result<()> { serialize!(self, "({}, {}, {}, {})", v.r, v.g, v.b, v.a) }
    fn write_string(&mut self, v: &BinString) -> io::Result<()> { serialize!(self, "'{}'", v.0) }
    fn write_hash(&mut self, v: &BinHash) -> io::Result<()> { serialize!(self, "{}", self.format_hash_value(v.0)) }
    fn write_path(&mut self, v: &BinPath) -> io::Result<()> { serialize!(self, "{}", self.format_path_value(v.0)) }
    fn write_link(&mut self, v: &BinLink) -> io::Result<()> { serialize!(self, "{}", self.format_entry_path(v.0)) }
    fn write_flag(&mut self, v: &BinFlag) -> io::Result<()> { serialize!(self, "{}", v.0) }

    fn write_list(&mut self, v: &BinList) -> io::Result<()> {
        serialize!(self, "[")?;
        indented!(self, {
            binvalue_map_type!(
                v.vtype, T,
                v.downcast::<T>().unwrap().iter().try_for_each(|x| {
                    serializeln!(self)?;
                    x.serialize_bin(self)
                }))?;
        });
        serializeln!(self, "]")?;
        Ok(())
    }

    fn write_struct(&mut self, v: &BinStruct) -> io::Result<()> {
        serialize!(self, "<STRUCT {} ", self.format_type_name(v.ctype))?;
        self.write_fields(&v.fields)?;
        serialize!(self, ">")?;
        Ok(())
    }

    fn write_embed(&mut self, v: &BinEmbed) -> io::Result<()> {
        serialize!(self, "<EMBED {} ", self.format_type_name(v.ctype))?;
        self.write_fields(&v.fields)?;
        serialize!(self, ">")?;
        Ok(())
    }

    fn write_option(&mut self, option: &BinOption) -> io::Result<()> {
        if option.value.is_none() {
            serialize!(self, "-")?;
        } else {
            serialize!(self, "[")?;
            indented!(self, {
                serializeln!(self)?;
                binvalue_map_type!(option.vtype, T, {
                    option
                        .downcast::<T>()
                        .unwrap()  // `None` case processed above
                        .serialize_bin(self)
                })?
            });
            serializeln!(self, "]")?;
        }
        Ok(())
    }

    fn write_map(&mut self, map: &BinMap) -> io::Result<()> {
        serialize!(self, "{{")?;
        indented!(self, {
            binvalue_map_keytype!(
                map.ktype, K,
                binvalue_map_type!(
                    map.vtype, V,
                    map.downcast::<K, V>().unwrap().iter().try_for_each(|(k, v)| -> io::Result<()> {
                        serializeln!(self)?;
                        k.serialize_bin(self)?;
                        serialize!(self, " => ")?;
                        v.serialize_bin(self)?;
                        Ok(())
                    })))?;
        });
        serializeln!(self, "}}")?;
        Ok(())
    }
}

fn basic_bintype_name(vtype: BinType) -> &'static str {
    match vtype {
        BinType::None => "NONE",
        BinType::Bool => "BOOL",
        BinType::S8 => "S8",
        BinType::U8 => "U8",
        BinType::S16 => "S16",
        BinType::U16 => "U16",
        BinType::S32 => "S32",
        BinType::U32 => "U32",
        BinType::S64 => "S64",
        BinType::U64 => "U64",
        BinType::Float => "FLOAT",
        BinType::Vec2 => "VEC2",
        BinType::Vec3 => "VEC3",
        BinType::Vec4 => "VEC4",
        BinType::Matrix => "MATRIX",
        BinType::Color => "COLOR",
        BinType::String => "STRING",
        BinType::Hash => "HASH",
        BinType::Path => "PATH",
        BinType::Struct => "STRUCT",
        BinType::Embed => "EMBED",
        BinType::Link => "LINK",
        BinType::Flag => "FLAG",
        _ => panic!("basic BinType name should not be needed for non-nestable types"),
    }
}


pub struct TextTreeEntriesSerializer<'a, W: Write> {
    parent: TextTreeSerializer<'a, W>,
}

impl<'a, W: Write> BinEntriesSerializer for TextTreeEntriesSerializer<'a, W> {
    fn write_entry(&mut self, entry: &BinEntry) -> io::Result<()> {
        self.parent.write_entry(entry)
    }

    fn end(&mut self) -> io::Result<()> {
        Ok(())
    }
}

