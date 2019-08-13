use std::io;
use super::data::*;
use super::{PropFile, BinEntry};

/// Serialize bin data
pub trait BinSerializer {
    type EntriesSerializer: BinEntriesSerializer;

    /// Write a single entry
    fn write_entry(&mut self, v: &BinEntry) -> io::Result<()>;
    /// Return a serializer to write streamed entries
    fn write_entries(self) -> io::Result<Self::EntriesSerializer>;

    /// Write entries from a `PropFile`
    fn write_binfile(self, v: &PropFile) -> io::Result<()> where Self: Sized {
        let mut s = self.write_entries()?;
        for entry in &v.entries {
            s.write_entry(entry)?;
        }
        s.end()?;
        Ok(())
    }

    // Scalar values
    fn write_none(&mut self, v: &BinNone) -> io::Result<()>;
    fn write_bool(&mut self, v: &BinBool) -> io::Result<()>;
    fn write_s8(&mut self, v: &BinS8) -> io::Result<()>;
    fn write_u8(&mut self, v: &BinU8) -> io::Result<()>;
    fn write_s16(&mut self, v: &BinS16) -> io::Result<()>;
    fn write_u16(&mut self, v: &BinU16) -> io::Result<()>;
    fn write_s32(&mut self, v: &BinS32) -> io::Result<()>;
    fn write_u32(&mut self, v: &BinU32) -> io::Result<()>;
    fn write_s64(&mut self, v: &BinS64) -> io::Result<()>;
    fn write_u64(&mut self, v: &BinU64) -> io::Result<()>;
    fn write_float(&mut self, v: &BinFloat) -> io::Result<()>;
    fn write_vec2(&mut self, v: &BinVec2) -> io::Result<()>;
    fn write_vec3(&mut self, v: &BinVec3) -> io::Result<()>;
    fn write_vec4(&mut self, v: &BinVec4) -> io::Result<()>;
    fn write_matrix(&mut self, v: &BinMatrix) -> io::Result<()>;
    fn write_color(&mut self, v: &BinColor) -> io::Result<()>;
    fn write_string(&mut self, v: &BinString) -> io::Result<()>;
    fn write_hash(&mut self, v: &BinHash) -> io::Result<()>;
    fn write_link(&mut self, v: &BinLink) -> io::Result<()>;
    fn write_flag(&mut self, v: &BinFlag) -> io::Result<()>;

    // Nested types
    fn write_list(&mut self, v: &BinList) -> io::Result<()>;
    fn write_struct(&mut self, v: &BinStruct) -> io::Result<()>;
    fn write_embed(&mut self, v: &BinEmbed) -> io::Result<()>;
    fn write_option(&mut self, v: &BinOption) -> io::Result<()>;
    fn write_map(&mut self, v: &BinMap) -> io::Result<()>;
}

/// Serialize streamed bin entries
pub trait BinEntriesSerializer {
    fn write_entry(&mut self, entry: &BinEntry) -> io::Result<()>;
    /// End the serialization
    ///
    /// This method should move out `end(self)` but it does not work on boxed instances.
    fn end(&mut self) -> io::Result<()>;
}


/// Serializable bin data
///
/// This trait is intended to be used by `BinSerializer` implementations.
pub trait BinSerializable {
    fn serialize_bin<S: BinSerializer>(&self, s: &mut S) -> io::Result<()>;
}

macro_rules! impl_serializable {
    ($type:ty, $func:ident) => {
        impl BinSerializable for $type {
            fn serialize_bin<S: BinSerializer>(&self, s: &mut S) -> io::Result<()> {
                s.$func(self)
            }
        }
    }
}

impl_serializable!(BinNone, write_none);
impl_serializable!(BinBool, write_bool);
impl_serializable!(BinS8, write_s8);
impl_serializable!(BinU8, write_u8);
impl_serializable!(BinS16, write_s16);
impl_serializable!(BinU16, write_u16);
impl_serializable!(BinS32, write_s32);
impl_serializable!(BinU32, write_u32);
impl_serializable!(BinS64, write_s64);
impl_serializable!(BinU64, write_u64);
impl_serializable!(BinFloat, write_float);
impl_serializable!(BinVec2, write_vec2);
impl_serializable!(BinVec3, write_vec3);
impl_serializable!(BinVec4, write_vec4);
impl_serializable!(BinMatrix, write_matrix);
impl_serializable!(BinColor, write_color);
impl_serializable!(BinString, write_string);
impl_serializable!(BinHash, write_hash);
impl_serializable!(BinList, write_list);
impl_serializable!(BinStruct, write_struct);
impl_serializable!(BinEmbed, write_embed);
impl_serializable!(BinLink, write_link);
impl_serializable!(BinOption, write_option);
impl_serializable!(BinMap, write_map);
impl_serializable!(BinFlag, write_flag);

