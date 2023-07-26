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
/// `visit_type()` can be used to easily ignore some types.
/// It is used for default implementations and internal shortcuts.
pub trait BinVisitor {
    /// Return true to visit given type
    fn visit_type(&mut self, _btype: BinType) -> bool { true }

    fn visit_entry(&mut self, _value: &BinEntry) -> bool { true }
    fn visit_field(&mut self, value: &BinField) -> bool { self.visit_type(value.vtype) }

    fn visit_none(&mut self, _value: &BinNone) {}
    fn visit_bool(&mut self, _value: &BinBool) {}
    fn visit_s8(&mut self, _value: &BinS8) {}
    fn visit_u8(&mut self, _value: &BinU8) {}
    fn visit_s16(&mut self, _value: &BinS16) {}
    fn visit_u16(&mut self, _value: &BinU16) {}
    fn visit_s32(&mut self, _value: &BinS32) {}
    fn visit_u32(&mut self, _value: &BinU32) {}
    fn visit_s64(&mut self, _value: &BinS64) {}
    fn visit_u64(&mut self, _value: &BinU64) {}
    fn visit_float(&mut self, _value: &BinFloat) {}
    fn visit_vec2(&mut self, _value: &BinVec2) {}
    fn visit_vec3(&mut self, _value: &BinVec3) {}
    fn visit_vec4(&mut self, _value: &BinVec4) {}
    fn visit_matrix(&mut self, _value: &BinMatrix) {}
    fn visit_color(&mut self, _value: &BinColor) {}
    fn visit_string(&mut self, _value: &BinString) {}
    fn visit_hash(&mut self, _value: &BinHash) {}
    fn visit_path(&mut self, _value: &BinPath) {}
    fn visit_list(&mut self, value: &BinList) -> bool { self.visit_type(BinType::List) && self.visit_type(value.vtype) }
    fn visit_struct(&mut self, _value: &BinStruct) -> bool { self.visit_type(BinType::Struct) }
    fn visit_embed(&mut self, _value: &BinEmbed) -> bool { self.visit_type(BinType::Embed) }
    fn visit_link(&mut self, _value: &BinLink) {}
    fn visit_option(&mut self, value: &BinOption) -> bool { self.visit_type(BinType::Option) && self.visit_type(value.vtype) }
    fn visit_map(&mut self, _value: &BinMap) -> bool { self.visit_type(BinType::Map) }
    fn visit_flag(&mut self, _value: &BinFlag) {}
}

/// Interface to traverse nested bin values with a visitor
pub trait BinTraversal {
    fn traverse_bin<BV: BinVisitor>(&self, visitor: &mut BV);
}

macro_rules! impl_traversal {
    ($t:ty, $visit:ident) => {
        impl BinTraversal for $t {
            #[inline]
            fn traverse_bin<BV: BinVisitor>(&self, visitor: &mut BV) {
                visitor.$visit(self);
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


impl BinTraversal for BinEntry {
    fn traverse_bin<BV: BinVisitor>(&self, visitor: &mut BV) {
        if visitor.visit_entry(self) {
            for field in self.fields.iter() {
                field.traverse_bin(visitor);
            }
        }
    }
}

impl BinTraversal for BinField {
    fn traverse_bin<BV: BinVisitor>(&self, visitor: &mut BV) {
        if visitor.visit_field(self) {
            binvalue_map_type!(self.vtype, T, {
                self.downcast::<T>().unwrap().traverse_bin(visitor);
            });
        }
    }
}

impl BinTraversal for BinStruct {
    fn traverse_bin<BV: BinVisitor>(&self, visitor: &mut BV) {
        if visitor.visit_struct(self) {
            for field in self.fields.iter() {
                field.traverse_bin(visitor);
            }
        }
    }
}

impl BinTraversal for BinEmbed {
    fn traverse_bin<BV: BinVisitor>(&self, visitor: &mut BV) {
        if visitor.visit_embed(self) {
            for field in self.fields.iter() {
                field.traverse_bin(visitor);
            }
        }
    }
}

impl BinTraversal for BinOption {
    fn traverse_bin<BV: BinVisitor>(&self, visitor: &mut BV) {
        if visitor.visit_option(self) {
            if self.value.is_some() {
                binvalue_map_type!(self.vtype, V, {
                    self.downcast::<V>().unwrap().traverse_bin(visitor);
                });
            }
        }
    }
}

impl BinTraversal for BinList {
    fn traverse_bin<BV: BinVisitor>(&self, visitor: &mut BV) {
        if visitor.visit_list(self) {
            binvalue_map_type!(self.vtype, V, {
                for v in self.downcast::<V>().unwrap().iter() {
                    v.traverse_bin(visitor);
                }
            });
        }
    }
}

impl BinTraversal for BinMap {
    fn traverse_bin<BV: BinVisitor>(&self, visitor: &mut BV) {
        if visitor.visit_map(self) {
            binvalue_map_keytype!(self.ktype, K, {
                binvalue_map_type!(self.vtype, V, {
                    for (k, v) in self.downcast::<K, V>().unwrap() {
                        k.traverse_bin(visitor);
                        v.traverse_bin(visitor);
                    }
                })
            });
        }
    }
}

