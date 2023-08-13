//! Visit a nested bin value

use super::{
    BinEntry,
    data::*,
    binvalue_map_type,
    binvalue_map_keytype,
};

/// Interface to visit values of nested bin values
///
/// Visit methods of nested types can return `false` to not visit nested values.
/// By default, everything is visited.
///
/// [visit_type()](Self::visit_type()) can be used to easily ignore some types.
/// It is used for default implementations and internal shortcuts.
#[allow(missing_docs)]
pub trait BinVisitor {
    type Error;

    /// Called to visit an entry
    ///
    /// This method exists so an implementation can execute code after an entry has been visited.
    fn traverse_entry(&mut self, value: &BinEntry) -> Result<(), Self::Error> {
        value.traverse_bin(self)
    }

    /// Return true to visit given type
    fn visit_type(&mut self, _btype: BinType) -> bool { true }

    fn visit_entry(&mut self, _value: &BinEntry) -> Result<bool, Self::Error> { Ok(true) }
    fn visit_field(&mut self, value: &BinField) -> Result<bool, Self::Error> { Ok(self.visit_type(value.vtype)) }

    fn visit_none(&mut self, _value: &BinNone) -> Result<(), Self::Error> { Ok(()) }
    fn visit_bool(&mut self, _value: &BinBool) -> Result<(), Self::Error> { Ok(()) }
    fn visit_s8(&mut self, _value: &BinS8) -> Result<(), Self::Error> { Ok(()) }
    fn visit_u8(&mut self, _value: &BinU8) -> Result<(), Self::Error> { Ok(()) }
    fn visit_s16(&mut self, _value: &BinS16) -> Result<(), Self::Error> { Ok(()) }
    fn visit_u16(&mut self, _value: &BinU16) -> Result<(), Self::Error> { Ok(()) }
    fn visit_s32(&mut self, _value: &BinS32) -> Result<(), Self::Error> { Ok(()) }
    fn visit_u32(&mut self, _value: &BinU32) -> Result<(), Self::Error> { Ok(()) }
    fn visit_s64(&mut self, _value: &BinS64) -> Result<(), Self::Error> { Ok(()) }
    fn visit_u64(&mut self, _value: &BinU64) -> Result<(), Self::Error> { Ok(()) }
    fn visit_float(&mut self, _value: &BinFloat) -> Result<(), Self::Error> { Ok(()) }
    fn visit_vec2(&mut self, _value: &BinVec2) -> Result<(), Self::Error> { Ok(()) }
    fn visit_vec3(&mut self, _value: &BinVec3) -> Result<(), Self::Error> { Ok(()) }
    fn visit_vec4(&mut self, _value: &BinVec4) -> Result<(), Self::Error> { Ok(()) }
    fn visit_matrix(&mut self, _value: &BinMatrix) -> Result<(), Self::Error> { Ok(()) }
    fn visit_color(&mut self, _value: &BinColor) -> Result<(), Self::Error> { Ok(()) }
    fn visit_string(&mut self, _value: &BinString) -> Result<(), Self::Error> { Ok(()) }
    fn visit_hash(&mut self, _value: &BinHash) -> Result<(), Self::Error> { Ok(()) }
    fn visit_path(&mut self, _value: &BinPath) -> Result<(), Self::Error> { Ok(()) }
    fn visit_list(&mut self, value: &BinList) -> Result<bool, Self::Error> {
        Ok(self.visit_type(BinType::List) && self.visit_type(value.vtype))
    }
    fn visit_struct(&mut self, _value: &BinStruct) -> Result<bool, Self::Error> {
        Ok(self.visit_type(BinType::Struct))
    }
    fn visit_embed(&mut self, _value: &BinEmbed) -> Result<bool, Self::Error> {
        Ok(self.visit_type(BinType::Embed))
    }
    fn visit_link(&mut self, _value: &BinLink) -> Result<(), Self::Error> { Ok(()) }
    fn visit_option(&mut self, value: &BinOption) -> Result<bool, Self::Error> {
        Ok(self.visit_type(BinType::Option) && self.visit_type(value.vtype))
    }
    fn visit_map(&mut self, _value: &BinMap) -> Result<bool, Self::Error> {
        Ok(self.visit_type(BinType::Map))
    }
    fn visit_flag(&mut self, _value: &BinFlag) -> Result<(), Self::Error> { Ok(()) }
}

/// Interface to traverse nested bin values with a visitor
pub trait BinTraversal<BV: BinVisitor + ?Sized> {
    /// Visit the value, recursively
    fn traverse_bin(&self, visitor: &mut BV) -> Result<(), BV::Error>;
}

macro_rules! impl_traversal {
    ($t:ty, $visit:ident) => {
        impl<BV: BinVisitor + ?Sized> BinTraversal<BV> for $t {
            #[inline]
            fn traverse_bin(&self, visitor: &mut BV) -> Result<(), BV::Error> {
                visitor.$visit(self)
            }
        }
    }
}

impl_traversal!(BinNone, visit_none);
impl_traversal!(BinBool, visit_bool);
impl_traversal!(BinS8, visit_s8);
impl_traversal!(BinU8, visit_u8);
impl_traversal!(BinS16, visit_s16);
impl_traversal!(BinU16, visit_u16);
impl_traversal!(BinS32, visit_s32);
impl_traversal!(BinU32, visit_u32);
impl_traversal!(BinS64, visit_s64);
impl_traversal!(BinU64, visit_u64);
impl_traversal!(BinFloat, visit_float);
impl_traversal!(BinVec2, visit_vec2);
impl_traversal!(BinVec3, visit_vec3);
impl_traversal!(BinVec4, visit_vec4);
impl_traversal!(BinMatrix, visit_matrix);
impl_traversal!(BinColor, visit_color);
impl_traversal!(BinString, visit_string);
impl_traversal!(BinHash, visit_hash);
impl_traversal!(BinPath, visit_path);
impl_traversal!(BinLink, visit_link);
impl_traversal!(BinFlag, visit_flag);


impl<BV: BinVisitor + ?Sized> BinTraversal<BV> for BinEntry {
    fn traverse_bin(&self, visitor: &mut BV) -> Result<(), BV::Error> {
        if visitor.visit_entry(self)? {
            for field in self.fields.iter() {
                field.traverse_bin(visitor)?;
            }
        }
        Ok(())
    }
}

impl<BV: BinVisitor + ?Sized> BinTraversal<BV> for BinField {
    fn traverse_bin(&self, visitor: &mut BV) -> Result<(), BV::Error> {
        if visitor.visit_field(self)? {
            binvalue_map_type!(self.vtype, T, {
                self.downcast::<T>().unwrap().traverse_bin(visitor)?;
            });
        }
        Ok(())
    }
}

impl<BV: BinVisitor + ?Sized> BinTraversal<BV> for BinStruct {
    fn traverse_bin(&self, visitor: &mut BV) -> Result<(), BV::Error> {
        if visitor.visit_struct(self)? {
            for field in self.fields.iter() {
                field.traverse_bin(visitor)?;
            }
        }
        Ok(())
    }
}

impl<BV: BinVisitor + ?Sized> BinTraversal<BV> for BinEmbed {
    fn traverse_bin(&self, visitor: &mut BV) -> Result<(), BV::Error> {
        if visitor.visit_embed(self)? {
            for field in self.fields.iter() {
                field.traverse_bin(visitor)?;
            }
        }
        Ok(())
    }
}

impl<BV: BinVisitor + ?Sized> BinTraversal<BV> for BinOption {
    fn traverse_bin(&self, visitor: &mut BV) -> Result<(), BV::Error> {
        if visitor.visit_option(self)? {
            if self.value.is_some() {
                binvalue_map_type!(self.vtype, V, {
                    self.downcast::<V>().unwrap().traverse_bin(visitor)?;
                });
            }
        }
        Ok(())
    }
}

impl<BV: BinVisitor + ?Sized> BinTraversal<BV> for BinList {
    fn traverse_bin(&self, visitor: &mut BV) -> Result<(), BV::Error> {
        if visitor.visit_list(self)? {
            binvalue_map_type!(self.vtype, V, {
                for v in self.downcast::<V>().unwrap().iter() {
                    v.traverse_bin(visitor)?;
                }
            });
        }
        Ok(())
    }
}

impl<BV: BinVisitor + ?Sized> BinTraversal<BV> for BinMap {
    fn traverse_bin(&self, visitor: &mut BV) -> Result<(), BV::Error> {
        if visitor.visit_map(self)? {
            binvalue_map_keytype!(self.ktype, K, {
                binvalue_map_type!(self.vtype, V, {
                    for (k, v) in self.downcast::<K, V>().unwrap() {
                        k.traverse_bin(visitor)?;
                        v.traverse_bin(visitor)?;
                    }
                })
            });
        }
        Ok(())
    }
}

