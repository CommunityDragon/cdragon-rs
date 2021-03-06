use super::{
    BinHashSets,
    BinEntry,
    data::*,
};

macro_rules! binvalue_map_with_hashes {
    ($b:expr, $t:ident, $e:expr) => (match $b {
        BinType::Hash => { type $t = BinHash; $e },
        BinType::List => { type $t = BinList; $e },
        BinType::Struct => { type $t = BinStruct; $e },
        BinType::Embed => { type $t = BinEmbed; $e },
        BinType::Link => { type $t = BinLink; $e },
        BinType::Option => { type $t = BinOption; $e },
        BinType::Map => { type $t = BinMap; $e },
        _ => {}
    })
}

/// Interface to gather hashes from nested bin values
pub(crate) trait GatherHashes {
    fn gather_hashes(&self, hashes: &mut BinHashSets);
}

impl GatherHashes for BinHash {
    #[inline]
    fn gather_hashes(&self, hashes: &mut BinHashSets) {
        hashes.hash_value.insert(self.0.hash);
    }
}

impl GatherHashes for BinLink {
    #[inline]
    fn gather_hashes(&self, hashes: &mut BinHashSets) {
        hashes.entry_path.insert(self.0.hash);
    }
}

impl GatherHashes for BinEntry {
    fn gather_hashes(&self, hashes: &mut BinHashSets) {
        hashes.entry_path.insert(self.path.hash);
        hashes.class_name.insert(self.ctype.hash);
        for field in self.fields.iter() {
            field.gather_hashes(hashes);
        }
    }
}

impl GatherHashes for BinField {
    fn gather_hashes(&self, hashes: &mut BinHashSets) {
        hashes.field_name.insert(self.name.hash);
        binvalue_map_with_hashes!(self.vtype, T, {
            self.downcast_value::<T>().unwrap().gather_hashes(hashes);
        });
    }
}

impl GatherHashes for BinStruct {
    fn gather_hashes(&self, hashes: &mut BinHashSets) {
        hashes.class_name.insert(self.ctype.hash);
        for field in self.fields.iter() {
            field.gather_hashes(hashes);
        }
    }
}

impl GatherHashes for BinEmbed {
    fn gather_hashes(&self, hashes: &mut BinHashSets) {
        hashes.class_name.insert(self.ctype.hash);
        for field in self.fields.iter() {
            field.gather_hashes(hashes);
        }
    }
}

impl GatherHashes for BinOption {
    fn gather_hashes(&self, hashes: &mut BinHashSets) {
        if self.value.is_some() {
            match self.vtype {
                BinType::Hash => self.downcast_value::<BinHash>().unwrap().gather_hashes(hashes),
                BinType::Link => self.downcast_value::<BinLink>().unwrap().gather_hashes(hashes),
                _ => {}
            }
        }
    }
}

impl GatherHashes for BinList {
    fn gather_hashes(&self, hashes: &mut BinHashSets) {
        match self.vtype {
            BinType::Struct => {
                for v in self.downcast_values::<BinStruct>().unwrap() {
                    v.gather_hashes(hashes);
                }
            }
            BinType::Hash => {
                for v in self.downcast_values::<BinHash>().unwrap() {
                    v.gather_hashes(hashes);
                }
            }
            BinType::Link => {
                for v in self.downcast_values::<BinLink>().unwrap() {
                    v.gather_hashes(hashes);
                }
            }
            _ => {}
        }
    }
}

impl GatherHashes for BinMap {
    fn gather_hashes(&self, hashes: &mut BinHashSets) {
        // process keys, then value, for better code factorization
        match self.ktype {
            BinType::Hash => binvalue_map_type!(self.vtype, V, {
                for (k, _) in self.downcast_values::<BinHash, V>().unwrap() {
                    k.gather_hashes(hashes);
                }
            }),
            _ => {}
        }
        binvalue_map_keytype!(self.ktype, K, {
            binvalue_map_with_hashes!(self.vtype, V, {
                for (_, v) in self.downcast_values::<K, V>().unwrap() {
                    v.gather_hashes(hashes);
                }
            })
        });
    }
}

