use std::path::Path;
use std::collections::{HashMap, HashSet};
use cdragon_prop::{
    data::*,
    BinEntry,
    BinHashKind,
    BinHashMappers,
    BinTraversal,
    BinVisitor,
    PropFile,
    binget,
};
use cdragon_hashes::{
    binh,
    bin::compute_binhash,
    HashOrStr,
};
use super::BinHashSets;
use crate::utils::bin_files_from_dir;


/// Base object to check bin hashes
pub struct BinHashFinder {
    /// Unknown hashes to find
    pub hashes: BinHashSets,
    /// Hash mappers where found hashes are added
    pub hmappers: BinHashMappers,
    /// Callback called when a new hash is found
    on_found: fn(u32, &str),
}

impl BinHashFinder {
    pub fn new(hashes: BinHashSets, hmappers: BinHashMappers) -> Self {
        Self { hashes, hmappers, on_found: |_, _| {} }
    }

    pub fn on_found(mut self, f: fn(u32, &str)) -> Self {
        self.on_found = f;
        self
    }

    /// Return true if the given hash is unknown
    pub fn is_unknown(&self, kind: BinHashKind, hash: u32) -> bool {
        self.hashes.get(kind).contains(&hash)
    }

    /// Get a hash string for given hash
    pub fn get_str(&self, kind: BinHashKind, hash: u32) -> Option<&str> {
        self.hmappers.get(kind).get(hash)
    }

    /// Try to get a string for the given hash
    pub fn seek_str(&self, kind: BinHashKind, hash: u32) -> HashOrStr<u32, &str> {
        match self.get_str(kind, hash) {
            Some(s) => HashOrStr::Str(s),
            None => HashOrStr::Hash(hash),
        }
    }

    /// Check a single string to match any unknown hash of a kind
    pub fn check_any<S: Into<String> + AsRef<str>>(&mut self, kind: BinHashKind, value: S) {
        let hash = compute_binhash(value.as_ref());
        if self.hashes.get_mut(kind).remove(&hash) {
            (self.on_found)(hash, value.as_ref());
            self.hmappers.get_mut(kind).insert(hash, value.into());
        }
    }

    /// Check an iterable of strings to match any unknown hash of a kind
    pub fn check_any_from_iter<S: Into<String> + AsRef<str>>(&mut self, kind: BinHashKind, values: impl Iterator<Item=S>) {
        let hashes = self.hashes.get_mut(kind);
        let hmapper = self.hmappers.get_mut(kind);
        for value in values {
            let hash = compute_binhash(value.as_ref());
            if hashes.remove(&hash) {
                (self.on_found)(hash, value.as_ref());
                hmapper.insert(hash, value.into());
            }
        }
    }

    /// Check an iterable of strings to match a subset of unknown hash of a kind
    pub fn check_selected_from_iter<S: Into<String> + AsRef<str>>(&mut self, kind: BinHashKind, selected: &HashSet<u32>, values: impl Iterator<Item=S>) {
        let hashes = self.hashes.get_mut(kind);
        let hmapper = self.hmappers.get_mut(kind);
        for value in values {
            let hash = compute_binhash(value.as_ref());
            if selected.contains(&hash) {
                if hashes.remove(&hash) {
                    (self.on_found)(hash, value.as_ref());
                    hmapper.insert(hash, value.into());
                }
            }
        }
    }

    /// Check a single string to match a given hash
    /// Return false if still unknown
    pub fn check_one<S: Into<String> + AsRef<str>>(&mut self, kind: BinHashKind, hash: u32, value: S) -> bool {
        let hashes = self.hashes.get_mut(kind);
        if !hashes.contains(&hash) {
            return true;
        }
        if hash == compute_binhash(value.as_ref()) {
            hashes.remove(&hash);
            (self.on_found)(hash, value.as_ref());
            let hmapper = self.hmappers.get_mut(kind);
            hmapper.insert(hash, value.into());
            return true;
        }
        false
    }

    /// Check an iterable of strings to match a given hash
    /// Return None if still unknown
    pub fn check_one_from_iter<S: Into<String> + AsRef<str>>(&mut self, kind: BinHashKind, hash: u32, values: impl Iterator<Item=S>) -> bool {
        let hashes = self.hashes.get_mut(kind);
        if !hashes.contains(&hash) {
            return true;
        }
        for value in values {
            if hash == compute_binhash(value.as_ref()) {
                hashes.remove(&hash);
                (self.on_found)(hash, value.as_ref());
                let hmapper = self.hmappers.get_mut(kind);
                hmapper.insert(hash, value.into());
                return true;
            }
        }
        false
    }
}


type GuessingFunc = fn(&BinEntry, &mut BinHashFinder);

/// IDs of the maps that have a `Maps/Shipping/Map{id}` directory
const MAP_IDS: [u32; 8] = [11, 12, 21, 22, 30, 33, 35, 453];

pub trait GuessingHook {
    /// Return entry types to watch
    fn entry_types(&self) -> &[BinClassName];
    /// Guess from an entry
    fn on_entry(&mut self, entry: &BinEntry, finder: &mut BinHashFinder);
    /// Called at the end of guessing, to possibly correlate things at the end
    fn on_end(&mut self, _finder: &mut BinHashFinder, _entries_by_type: &HashMap<BinClassName, Vec<BinEntryPath>>) {}
}


/// Guess bin hashes from bin files and hashes
pub struct BinHashGuesser {
    /// Hooks added to the guesser
    hooks: Vec<Box<dyn GuessingHook>>,
    /// Indexes of hooks registered for each entry type
    registry: HashMap<BinClassName, Vec<usize>>,
    /// Finder used to guess hashes
    finder: BinHashFinder,
    /// Collected entries paths, grouped by type
    entries_by_type: HashMap<BinClassName, Vec<BinEntryPath>>,
}

impl BinHashGuesser {
    pub fn new(finder: BinHashFinder) -> Self {
        Self {
            hooks: Vec::default(),
            registry: HashMap::default(),
            finder,
            entries_by_type: HashMap::default(),
        }
    }

    pub fn with_hook(mut self, hook: Box<dyn GuessingHook>) -> Self {
        let i = self.hooks.len();
        for t in hook.entry_types().iter() {
            self.registry.entry(*t).or_default().push(i);
        }
        self.hooks.push(hook);
        self
    }

    pub fn with_single_hook(self, typ: BinClassName, on_entry: GuessingFunc) -> Self {
        self.with_hook(Box::new(SingleHook::new(typ, on_entry)))
    }

    pub fn with_multi_hook(self, types: &'static [BinClassName], on_entry: GuessingFunc) -> Self {
        self.with_hook(Box::new(MultiHook::new(types, on_entry)))
    }

    // Guessable, but don't bother
    // - TrophyData: Loadouts/SummonerTrophies/Trophies/{cup}/Trophy_{n}
    //   Where {cup} is from {4458ef52} (type {1ebb9d12}) and {n} is 4, 8, 16

    //TODO
    // GameModeMapData parsing
    // - Format is `Maps/Shipping/{map}/Modes/{mModeName}`
    // - ... but the map has to be iterated
    // Pedestal
    // - pattern: Loadouts/SummonerTrophies/Pedestals/%pedestal%
    // Lots of hashes that are actually entries
    // - List the matches to detect fields
    // - Use full hash for all `*ViewController` types, and UI elements
    // - ContextualConditionCharacterName

    /// Add all known hooks
    #[allow(dead_code)]
    pub fn with_all_hooks(self) -> Self {
        self
            .with_entry_from_attr_hooks()
            .with_simple_hooks()
            .with_character_hooks()
            .with_collecting_hooks()
    }

    /// Add a hook to get some statistics on entries
    #[allow(dead_code)]
    pub fn with_entry_stats(self) -> Self {
        self.with_hook(Box::new(EntryTypesStatsHook))
    }

    /// Add hooks to guess entry's path from an attribute
    pub fn with_entry_from_attr_hooks(self) -> Self {
        // Guess the entry path using a field value directly
        macro_rules! EntryPathAttrHook {
            ($typ:ident.$attr:ident) => { EntryPathAttrHook!(binh!(stringify!($typ)), $attr) };
            ($typ:literal.$attr:ident) => { EntryPathAttrHook!($typ.into(), $attr) };
            ($typ:expr, $attr:ident) => {
                Box::new(SingleHook::new($typ, |entry, finder| {
                    if finder.is_unknown(BinHashKind::EntryPath, entry.path.hash) {
                        let arg = binget!(entry => $attr(BinString)).unwrap();
                        finder.check_one(BinHashKind::EntryPath, entry.path.hash, &arg.0);
                    }
                }))
            };
            ([$typ:expr].$attr:ident) => {
                Box::new(MultiHook::new($typ, |entry, finder| {
                    if finder.is_unknown(BinHashKind::EntryPath, entry.path.hash) {
                        let arg = binget!(entry => $attr(BinString)).unwrap();
                        finder.check_one(BinHashKind::EntryPath, entry.path.hash, &arg.0);
                    }
                }))
            };
        }

        // Guess the entry path using a pattern and a field value
        macro_rules! EntryPathPatternHook {
            ($typ:ident.$attr:ident($ty:ty): $arg:ident => $fmt:literal, $val:expr) => {
                Box::new(SingleHook::new(binh!(stringify!($typ)), |entry, finder| {
                    if finder.is_unknown(BinHashKind::EntryPath, entry.path.hash) {
                        let $arg = &binget!(entry => $attr($ty)).unwrap().0;
                        finder.check_one(BinHashKind::EntryPath, entry.path.hash, format!($fmt, $val));
                    }
                }))
            };
            ($typ:ident.$attr:ident($ty:ty) => $fmt:literal) => {
                EntryPathPatternHook!($typ.$attr($ty): arg => $fmt, arg)
            };
            ($typ:ident.$attr:ident: $arg:ident => $fmt:literal, $val:expr) => {
                EntryPathPatternHook!($typ.$attr(BinString): $arg => $fmt, $val)
            };
            ($typ:ident.$attr:ident => $fmt:literal) => {
                EntryPathPatternHook!($typ.$attr(BinString) => $fmt)
            };
        }

        // Many types have their path in the `name` field
        // We could also check `name` in all cases but that would require to parse ALL entries.
        const NAMED_TYPES: [BinClassName; 35] = [
            binh!(BinClassName, "StaticMaterialDef"),
            binh!(BinClassName, "UISceneData"),
            binh!(BinClassName, "UiElementEffectAmmoData"),
            binh!(BinClassName, "UiElementEffectAnimatedRotatingIconData"),
            binh!(BinClassName, "UiElementEffectAnimationData"),
            binh!(BinClassName, "UiElementEffectArcFillData"),
            binh!(BinClassName, "UiElementEffectCircleMaskCooldownData"),
            binh!(BinClassName, "UiElementEffectCircleMaskDesaturateData"),
            binh!(BinClassName, "UiElementEffectCooldownData"),
            binh!(BinClassName, "UiElementEffectCooldownRadialData"),
            binh!(BinClassName, "UiElementEffectCustomMaterialData"),
            binh!(BinClassName, "UiElementEffectData"),
            binh!(BinClassName, "UiElementEffectDesaturateData"),
            binh!(BinClassName, "UiElementEffectFillPercentageData"),
            binh!(BinClassName, "UiElementEffectGlowConstantData"),
            binh!(BinClassName, "UiElementEffectGlowData"),
            binh!(BinClassName, "UiElementEffectGlowingRotatingIconData"),
            binh!(BinClassName, "UiElementEffectInstancedData"),
            binh!(BinClassName, "UiElementEffectLineData"),
            binh!(BinClassName, "UiElementEffectRotatingIconData"),
            binh!(BinClassName, "UiElementGroupButtonData"),
            binh!(BinClassName, "UiElementGroupData"),
            binh!(BinClassName, "UiElementGroupFramedData"),
            binh!(BinClassName, "UiElementGroupManagedLayoutData"),
            binh!(BinClassName, "UiElementGroupMeterData"),
            binh!(BinClassName, "UiElementGroupSliderData"),
            binh!(BinClassName, "UiElementIconData"),
            binh!(BinClassName, "UiElementParticleSystemData"),
            binh!(BinClassName, "UiElementRegionData"),
            binh!(BinClassName, "UiElementScissorRegionData"),
            binh!(BinClassName, "UiElementSpineAnimationData"),
            binh!(BinClassName, "UiElementTextData"),
            binh!(BinClassName, "UiSceneViewPaneData"),
            binh!(BinClassName, "UiComponent"),
            BinClassName { hash:0x857c08ad },
        ];

        self
            .with_hook(EntryPathAttrHook!([&NAMED_TYPES].name))
            .with_hook(EntryPathAttrHook!(ContextualActionData.mObjectPath))
            .with_hook(EntryPathAttrHook!(CustomShaderDef.objectPath))
            .with_hook(EntryPathAttrHook!(MapContainer.mapPath))
            .with_hook(EntryPathAttrHook!(RewardGroup.internalName))
            .with_hook(EntryPathAttrHook!(VfxSystemDefinitionData.particlePath))
            .with_hook(EntryPathAttrHook!(Sequence.path))
            .with_hook(EntryPathPatternHook!(CharacterRecord.mCharacterName => "Characters/{}/CharacterRecords/Root"))
            .with_hook(EntryPathPatternHook!(GameFontDescription.name => "UX/Fonts/Descriptions/{}"))
            .with_hook(EntryPathPatternHook!(ItemData.itemID(BinU32) => "Items/{}"))
            .with_hook(EntryPathPatternHook!(SummonerEmote.summonerEmoteId(BinU32) => "Loadouts/SummonerEmotes/{}"))
            .with_hook(EntryPathPatternHook!(TFTCharacterRecord.mCharacterName => "Characters/{}/CharacterRecords/Root"))
            .with_hook(EntryPathPatternHook!(TFTRoundData.mName => "Maps/Shipping/Map22/Rounds/{}"))
            .with_hook(EntryPathPatternHook!(TftItemData.mName => "Maps/Shipping/Map22/Items/{}"))
            .with_hook(EntryPathPatternHook!(TftMapSkin.mapContainer: s => "Loadouts/TFTMapSkins/{}", s.rsplit_once('/').unwrap().1))
            .with_hook(EntryPathPatternHook!(TftSetData.name => "Maps/Shipping/Map22/Sets/{}"))
            .with_hook(EntryPathPatternHook!(TooltipFormat.mObjectName => "UX/Tooltips/{}"))
            .with_hook(EntryPathPatternHook!(X3DSharedConstantBufferDef.name => "Shaders/SharedData/{}"))
            .with_hook(EntryPathPatternHook!(Character.name => "Characters/{}"))
            .with_hook(EntryPathPatternHook!(CheatSet.mName => "Cheats/CheatSets/{}"))
            .with_hook(EntryPathPatternHook!(TftPassAsset.internalName => "Passes/TFT/Assets/{}"))
            .with_hook(EntryPathPatternHook!(GuestOfHonor.name => "Maps/Shipping/Map30/GuestOfHonor/{}"))
            .with_hook(EntryPathPatternHook!(TftZoomSkin.name => "Loadouts/TFTZoomSkins/{}"))
    }

    /// Add relatively simple (but not trivial) hooks
    pub fn with_simple_hooks(self) -> Self {
        const RESOURCE_RESOLVER_TYPES: [BinClassName; 2] = [
            binh!(BinClassName, "ResourceResolver"),
            binh!(BinClassName, "GlobalResourceResolver"),
        ];

        const OBJECT_PATH_TYPES: [BinClassName; 5] = [
            binh!(BinClassName, "VfxSystemDefinitionData"),
            binh!(BinClassName, "SpellObject"),
            binh!(BinClassName, "SkinCharacterDataProperties"),
            binh!(BinClassName, "TftSkinCharacterDataProperties"),
            binh!(BinClassName, "AnimationGraphData"),
        ];

        // Types that store entry paths in hash values
        const ENTRY_PATH_HASH_TYPES: [BinClassName; 12] = [
            binh!(BinClassName, "EsportsBannerConfiguration"),
            binh!(BinClassName, "GameModeChampionList"),
            binh!(BinClassName, "KillCalloutsViewController"),
            binh!(BinClassName, "OffScreenPOIViewController"),
            binh!(BinClassName, "PingRadialViewController"),
            binh!(BinClassName, "PlayerReportViewController"),
            binh!(BinClassName, "PracticeToolViewController"),
            binh!(BinClassName, "RewardGroup"),
            binh!(BinClassName, "TFTModeData"),
            binh!(BinClassName, "TftPlaybook"),
            binh!(BinClassName, "UnitFloatingInfoBarData"),
            BinClassName { hash: 0x409a5657 },
        ];

        /// Guess a hash key from a link value, check full path or basename
        fn guess_map_key_from_link_value(map: &BinMap, finder: &mut BinHashFinder) {
            if let Some(map) = &binget!(map => (BinHash, BinLink)) {
                for (k, v) in map.iter() {
                    if finder.is_unknown(BinHashKind::HashValue, k.0.hash) {
                        if let Some(target) = finder.get_str(BinHashKind::EntryPath, v.0.hash) {
                            let target = target.to_owned();
                            if finder.check_one(BinHashKind::HashValue, k.0.hash, &target) {
                                // found
                            } else if let Some((_, base)) = target.rsplit_once('/') {
                                if !finder.check_one(BinHashKind::HashValue, k.0.hash, base)
                                && !finder.check_one(BinHashKind::HashValue, k.0.hash, format!("{}_BV2", base)) {
                                    finder.check_one_from_iter(BinHashKind::HashValue, k.0.hash, resource_key_candidates(base).into_iter());
                                }
                            }
                        }
                    }
                }
            }
        }

        self
            // Guess ResourceResolve.resourceMap keys from values
            .with_multi_hook(&RESOURCE_RESOLVER_TYPES, |entry, finder| {
                // Notes
                // - 'Particles' paths are already guessed
                // - Some entries don't exist at all
                if let Some(map) = &binget!(entry => resourceMap(BinMap)) {
                    guess_map_key_from_link_value(map, finder);
                }
            })

            // Guess from ViewControllerList
            .with_single_hook(binh!("ViewControllerList"), |entry, finder| {
                // Assume all strings are entry paths (true in practice)
                // No maps, visit only lists and structs
                struct CheckStrings<'a> {
                    finder: &'a mut BinHashFinder,
                }

                impl<'a> BinVisitor for CheckStrings<'a> {
                    type Error = ();

                    fn visit_type(&mut self, btype: BinType) -> bool {
                        matches!(btype,
                            BinType::String |
                            BinType::List |
                            BinType::List2 |
                            BinType::Struct |
                            BinType::Embed)
                    }

                    fn visit_string(&mut self, value: &BinString) -> Result<(), ()> {
                        self.finder.check_any(BinHashKind::EntryPath, &value.0);
                        Ok(())
                    }
                }

                let mut visitor = CheckStrings { finder };
                entry.traverse_bin(&mut visitor).unwrap()
            })

            // Guess from ViewControllerSet
            .with_single_hook(binh!("ViewControllerSet"), |entry, finder| {
                if let Some(list) = binget!(entry => SpecifiedGameModes(BinList)(BinString)) {
                    let it = list.iter().map(|v| &v.0);
                    finder.check_any_from_iter(BinHashKind::EntryPath, it);
                }
            })

            // Guess TftMapGroupData links, from TftMapSkin.GroupLink
            .with_single_hook(binh!("TftMapSkin"), |entry, finder| {
                if let Some(BinString(s)) = binget!(entry => GroupLink(BinString)) {
                    // Link to TftMapGroupData entry
                    finder.check_any(BinHashKind::EntryPath, s);
                }
            })

            // Guess SpellObject path from mScriptName
            // This does more than `EntryPathPatternHook!(SpellObject.mScriptName => "Items/Spells/{}"))`
            .with_single_hook(binh!("SpellObject"), |entry, finder| {
                if finder.is_unknown(BinHashKind::EntryPath, entry.path.hash) {
                    let name = &binget!(entry => mScriptName(BinString)).unwrap().0;
                    if finder.check_one(BinHashKind::EntryPath, entry.path.hash, format!("Items/Spells/{}", name))
                    || finder.check_one(BinHashKind::EntryPath, entry.path.hash, format!("Shared/Spells/{}", name)) {
                        return;
                    }
                    let it = MAP_IDS.iter().map(|i| format!("Maps/Shipping/Map{}/Spells/{}", i, name));
                    if finder.check_one_from_iter(BinHashKind::EntryPath, entry.path.hash, it) {
                        return;
                    }
                    if let Some((id, _)) = name.split_once(|c: char| !c.is_ascii_digit())
                    && finder.check_one(BinHashKind::EntryPath, entry.path.hash, format!("Items/{}/Spells/{}", id, name)) {
                        return;
                    }
                    // A child spell of a map spell is under its parent: `Maps/Shipping/Map{id}/Spells/{parent}/{name}`
                    // The parent name is a prefix of the child name
                    let it = MAP_IDS.iter().flat_map(|i| {
                        name.char_indices().skip(1).map(move |(n, _)| format!("Maps/Shipping/Map{}/Spells/{}/{}", i, &name[..n], name))
                    });
                    finder.check_one_from_iter(BinHashKind::EntryPath, entry.path.hash, it);
                }

                // guess mSpellCalculations mDataValue hashes from SpellDataValue.name in mSpell.DataValues
                if let Some(spell) = binget!(entry => mSpell(BinStruct)) {
                    if let Some(data_values) = binget!(spell => DataValues(BinList)) {
                        for data_value in data_values.downcast::<BinEmbed>().unwrap() {
                            finder.check_any(BinHashKind::HashValue, &binget!(data_value => name(BinString)).unwrap().0);
                        }
                    }
                }
                //.with_hook(EntryPathPatternHook!(SpellObject.mScriptName => "Items/Spells/{}"))
            })

            // Guess Cheats path from ScriptCheat.mName
            .with_single_hook(binh!("ScriptCheat"), |entry, finder| {
                if finder.is_unknown(BinHashKind::EntryPath, entry.path.hash) {
                    let name = &binget!(entry => mName(BinString)).unwrap().0;
                    finder.check_one(BinHashKind::EntryPath, entry.path.hash, format!("Cheats/GameModes/TFT/{}", name));
                    finder.check_one(BinHashKind::EntryPath, entry.path.hash, format!("Cheats/GameModes/Cherry/{}", name));
                    finder.check_one(BinHashKind::EntryPath, entry.path.hash, format!("Cheats/GameModes/Slime/{}", name));
                    finder.check_one(BinHashKind::EntryPath, entry.path.hash, format!("Cheats/GameModes/Strawberry/{}", name));
                    finder.check_one(BinHashKind::EntryPath, entry.path.hash, format!("Cheats/GameModes/Ultbook/{}", name));
                }
            })

            // Guess TftTraitData path from TftTraitData.mName
            .with_single_hook(binh!("TftTraitData"), |entry, finder| {
                if finder.is_unknown(BinHashKind::EntryPath, entry.path.hash) {
                    let name = &binget!(entry => mName(BinString)).unwrap().0;
                    let it = (1..30).map(|i| format!("Maps/Shipping/Map22/Sets/TFTSet{}/Traits/{}", i, name));
                    finder.check_one_from_iter(BinHashKind::EntryPath, entry.path.hash, it);
                }
            })

            // Guess TftItemData path from TftItemData.mName
            .with_single_hook(binh!("TftItemData"), |entry, finder| {
                if finder.is_unknown(BinHashKind::EntryPath, entry.path.hash) {
                    let name = &binget!(entry => mName(BinString)).unwrap().0;
                    let it = (1..30).map(|i| format!("Maps/Shipping/Map22/Sets/TFTSet{}/Augments/{}", i, name));
                    finder.check_one_from_iter(BinHashKind::EntryPath, entry.path.hash, it);
                    let it = (1..30).map(|i| format!("Maps/Shipping/Map22/Sets/TFTSet{}/Items/{}", i, name));
                    finder.check_one_from_iter(BinHashKind::EntryPath, entry.path.hash, it);
                    let it = (1..30).map(|i| format!("Maps/Shipping/Map22/Augments/Set{}/{}", i, name));
                    finder.check_one_from_iter(BinHashKind::EntryPath, entry.path.hash, it);
                    finder.check_one(BinHashKind::EntryPath, entry.path.hash, format!("Maps/Shipping/Map22/Items/{}", name));
                    finder.check_one(BinHashKind::EntryPath, entry.path.hash, format!("Maps/Shipping/Map22/Augments/{}", name));
                    finder.check_one(BinHashKind::EntryPath, entry.path.hash, format!("Maps/Shipping/Map22/Augments/Shared/{}", name));
                }
            })

            // Guess entry path from TftShopData.mName
            .with_single_hook(binh!("TftShopData"), |entry, finder| {
                if finder.is_unknown(BinHashKind::EntryPath, entry.path.hash) {
                    let name = &binget!(entry => mName(BinString)).unwrap().0;
                    let it = (1..30).map(|i| format!("Maps/Shipping/Map22/Sets/TFTSet{}/Shop/{}", i, name));
                    finder.check_one_from_iter(BinHashKind::EntryPath, entry.path.hash, it);
                    finder.check_one(BinHashKind::EntryPath, entry.path.hash, format!("Maps/Shipping/Map22/Shop/{}", name));
                }
            })

            // Guess entry path from TftItemList.name
            .with_single_hook(binh!("TftItemList"), |entry, finder| {
                if finder.is_unknown(BinHashKind::EntryPath, entry.path.hash) {
                    let name = &binget!(entry => name(BinString)).unwrap().0;
                    let it = (1..30).map(|i| format!("Maps/Shipping/Map22/Sets/TFTSet{}/{}", i, name));
                    finder.check_one_from_iter(BinHashKind::EntryPath, entry.path.hash, it);
                }
            })

            // Guess entry path and VfxResourceResolver hash from TFTDamageSkin.mName
            .with_single_hook(binh!("TFTDamageSkin"), |entry, finder| {
                if finder.is_unknown(BinHashKind::EntryPath, entry.path.hash) {
                    if let Some(tiered_name) = binget!(entry => mName(BinString)) {
                        if let Some((name, _)) = tiered_name.0.rsplit_once("_") {
                            finder.check_one(BinHashKind::EntryPath, entry.path.hash, format!("Loadouts/TFTDamageSkins/{}/{}", name, tiered_name.0));
                        }
                    }
                }

                if let Some(entry_path) = finder.get_str(BinHashKind::EntryPath, entry.path.hash) {
                    let entry_path = entry_path.to_owned();
                    if let Some(hash) = binget!(entry => VfxResourceResolver(BinHash)) {
                        finder.check_one(BinHashKind::EntryPath, hash.0.hash, format!("{}/ResourceBin/Resources", entry_path));
                        finder.check_one(BinHashKind::HashValue, hash.0.hash, format!("{}/ResourceBin/Resources", entry_path));
                    }
                }
            })

            // Guess entry path and VfxResourceResolver hash from TftPlaybook.name
            .with_single_hook(binh!("TftPlaybook"), |entry, finder| {
                if finder.is_unknown(BinHashKind::EntryPath, entry.path.hash) {
                    if let Some(name) = binget!(entry => name(BinString)) {
                        finder.check_one(BinHashKind::EntryPath, entry.path.hash, format!("Loadouts/TFTPlaybooks/{}", name.0.replace(' ', "")));
                    }
                }

                if let Some(entry_path) = finder.get_str(BinHashKind::EntryPath, entry.path.hash) {
                    let entry_path = entry_path.to_owned();
                    if let Some(hash) = binget!(entry => VfxResourceResolver(BinHash)) {
                        finder.check_one(BinHashKind::EntryPath, hash.0.hash, format!("{}/Resources", entry_path));
                        finder.check_one(BinHashKind::HashValue, hash.0.hash, format!("{}/Resources", entry_path));
                    }
                }
            })

            // Guess VfxResourceResolver hash from entry path
            .with_single_hook(binh!("TftZoomSkin"), |entry, finder| {
                if let Some(entry_path) = finder.get_str(BinHashKind::EntryPath, entry.path.hash) {
                    let entry_path = entry_path.to_owned();
                    if let Some(hash) = binget!(entry => VfxResourceResolver(BinHash)) {
                        finder.check_one(BinHashKind::EntryPath, hash.0.hash, format!("{}/ResourceBin/Resources", entry_path));
                        finder.check_one(BinHashKind::HashValue, hash.0.hash, format!("{}/ResourceBin/Resources", entry_path));
                    }
                }
            })

            // Guess ItemGroups path from ItemGroup.mItemGroupID (a hash)
            .with_single_hook(binh!("ItemGroup"), |entry, finder| {
                if finder.is_unknown(BinHashKind::EntryPath, entry.path.hash) {
                    if let Some(hash) = binget!(entry => mItemGroupID(BinHash)) {
                        if let Some(id) = finder.get_str(BinHashKind::HashValue, hash.0.hash) {
                            finder.check_one(BinHashKind::EntryPath, entry.path.hash, format!("Items/ItemGroup/{}", id));
                        }
                    }
                }
            })

            // Guess MapSkin path from MapSkin.name for each known map id
            .with_single_hook(binh!("MapSkin"), |entry, finder| {
                if finder.is_unknown(BinHashKind::EntryPath, entry.path.hash) {
                    let name = &binget!(entry => name(BinString)).unwrap().0;
                    let it = MAP_IDS.iter().map(|i| format!("Maps/Shipping/Map{}/MapSkins/{}", i, name));
                    finder.check_one_from_iter(BinHashKind::EntryPath, entry.path.hash, it);
                }
            })

            // Guess MapAudioDataProperties path for each known map id
            .with_single_hook(binh!("MapAudioDataProperties"), |entry, finder| {
                if finder.is_unknown(BinHashKind::EntryPath, entry.path.hash) {
                    let it = MAP_IDS.iter().map(|i| format!("Maps/Shipping/Map{}/Audio", i));
                    finder.check_one_from_iter(BinHashKind::EntryPath, entry.path.hash, it);
                }
            })

            // Guess TftUnitPropertyDefinition path from TftUnitPropertyDefinition.name
            .with_single_hook(binh!("TftUnitPropertyDefinition"), |entry, finder| {
                if finder.is_unknown(BinHashKind::EntryPath, entry.path.hash) {
                    if let Some(name) = binget!(entry => name(BinString)) {
                        finder.check_one(BinHashKind::EntryPath, entry.path.hash, format!("Maps/Shipping/Map22/UnitProperties/{}", &name.0));
                    }
                }
            })

            // Guess AnvilData path from AnvilData.AugmentNameId for each known map id
            .with_single_hook(binh!("AnvilData"), |entry, finder| {
                if finder.is_unknown(BinHashKind::EntryPath, entry.path.hash) {
                    if let Some(name) = binget!(entry => AugmentNameId(BinString)) {
                        let it = MAP_IDS.iter().map(|i| format!("Maps/Shipping/Map{}/Anvils/{}", i, &name.0));
                        finder.check_one_from_iter(BinHashKind::EntryPath, entry.path.hash, it);
                    }
                }
            })

            // Guess hash values that are entry paths
            .with_multi_hook(&ENTRY_PATH_HASH_TYPES, |entry, finder| {
                struct CheckHashes<'a> {
                    finder: &'a mut BinHashFinder,
                }

                impl<'a> BinVisitor for CheckHashes<'a> {
                    type Error = ();

                    fn visit_hash(&mut self, value: &BinHash) -> Result<(), ()> {
                        let hash = value.0.hash;
                        if self.finder.is_unknown(BinHashKind::HashValue, hash)
                        && let Some(path) = self.finder.get_str(BinHashKind::EntryPath, hash) {
                            let path = path.to_owned();
                            self.finder.check_one(BinHashKind::HashValue, hash, path);
                        }
                        Ok(())
                    }
                }

                let mut visitor = CheckHashes { finder };
                entry.traverse_bin(&mut visitor).unwrap()
            })

            // Guess paths from AugmentData.AugmentNameId
            .with_single_hook(binh!("AugmentData"), |entry, finder| {
                if let Some(augment_name) = binget!(entry => AugmentNameId(BinString)) {
                    if finder.is_unknown(BinHashKind::EntryPath, entry.path.hash) {
                        finder.check_one(BinHashKind::EntryPath, entry.path.hash, format!("Maps/ModeSpecificData/Augments/{}", &augment_name.0));
                        let it = MAP_IDS.iter().map(|i| format!("Maps/Shipping/Map{}/AugmentTags/{}", i, &augment_name.0));
                        finder.check_one_from_iter(BinHashKind::EntryPath, entry.path.hash, it);
                    }
                    if let Some(root_spell) = binget!(entry => RootSpell(BinLink)) && finder.is_unknown(BinHashKind::EntryPath, root_spell.0.hash) {
                        finder.check_one(BinHashKind::EntryPath, root_spell.0.hash, format!("Maps/ModeSpecificData/Augments/{}/Augment_{}", &augment_name.0, &augment_name.0));
                    }
                }
            })

            // Guess entry path from ModesQuestData.QuestName
            .with_single_hook(binh!("ModesQuestData"), |entry, finder| {
                if finder.is_unknown(BinHashKind::EntryPath, entry.path.hash) {
                    if let Some(quest_name) = binget!(entry => QuestName(BinString)) {
                        finder.check_one(BinHashKind::EntryPath, entry.path.hash, format!("Maps/ModeSpecificData/ModesQuests/{}", &quest_name.0));
                    }
                }
            })

            // Guess CompanionSpeciesData path from CompanionData.speciesLink
            .with_single_hook(binh!("CompanionData"), |entry, finder| {
                if let Some(s) = binget!(entry => speciesLink(BinString)) {
                    finder.check_any(BinHashKind::EntryPath, &s.0);
                }
            })

            // Guess ChallengeConfigData path from ChallengeConfigData.ID
            .with_single_hook(binh!("ChallengeConfigData"), |entry, finder| {
                if finder.is_unknown(BinHashKind::EntryPath, entry.path.hash) {
                    let id = match binget!(entry => ID(BinU64)) {
                        Some(id) => id.0,
                        None => 0,
                    };
                    finder.check_one(BinHashKind::EntryPath, entry.path.hash, format!("LCU/Challenges/Config/{}/Config", id));
                }
            })

            // Guess various from MapPlaceableContainer.items
            // Values of type GdsMapObject have a {ad304db5} path
            .with_single_hook(binh!("MapPlaceableContainer"), |entry, finder| {
                if let Some(map) = binget!(entry => items(BinMap)(BinHash, BinStruct)) {
                    let it = map.iter().filter_map(|(_, data)| {
                        if data.ctype == binh!("GdsMapObject") {
                            binget!(data => 0xad304db5(BinString)).map(|s| &s.0)
                        } else {
                            None
                        }
                    });
                    finder.check_any_from_iter(BinHashKind::EntryPath, it);
                }
            })

            // Guess MapContainer.chunks keys from values
            .with_single_hook(binh!("MapContainer"), |entry, finder| {
                if let Some(map) = &binget!(entry => chunks(BinMap)) {
                    guess_map_key_from_link_value(map, finder);
                }
            })

            // Guess ItemData.mVFXResourceResolver.resourceMap keys from values
            .with_single_hook(binh!("ItemData"), |entry, finder| {
                if let Some(resolver) = binget!(entry => mVFXResourceResolver(BinStruct)) {
                    if let Some(map) = &binget!(resolver => resourceMap(BinMap)) {
                        guess_map_key_from_link_value(map, finder);
                    }
                }
            })

            // Guess the objectPath hash from its own entry path
            .with_multi_hook(&OBJECT_PATH_TYPES, |entry, finder| {
                if let Some(object_path) = binget!(entry => objectPath(BinHash)) {
                    if finder.is_unknown(BinHashKind::HashValue, object_path.0.hash) {
                        if let Some(s) = finder.get_str(BinHashKind::EntryPath, entry.path.hash) {
                            finder.check_one(BinHashKind::HashValue, object_path.0.hash, s.to_owned());
                        }
                    }
                }
            })
    }

    /// Add guessing from character data
    pub fn with_character_hooks(self) -> Self {
        const CHARACTER_RECORDS: [BinClassName; 2] = [
            binh!(BinClassName, "CharacterRecord"),
            binh!(BinClassName, "TFTCharacterRecord"),
        ];

        const SKIN_CHARACTER_DATA_PROPERTIES: [BinClassName; 2] = [
            binh!(BinClassName, "SkinCharacterDataProperties"),
            binh!(BinClassName, "TftSkinCharacterDataProperties"),
        ];

        self
            .with_multi_hook(&CHARACTER_RECORDS, on_character_record_entry)
            .with_multi_hook(&SKIN_CHARACTER_DATA_PROPERTIES, on_skin_character_data_entry)
            // Guess `AnimationGraphData.mClipDataMap` from `mAnimationResourceData.mAnimationFilePath`
            .with_single_hook(binh!("AnimationGraphData"), |entry, finder| {
                fn check_clip_data(hash: u32, data: &BinStruct, finder: &mut BinHashFinder) -> Option<()> {
                    if finder.is_unknown(BinHashKind::HashValue, hash) {
                        let path = &binget!(data => mAnimationResourceData(BinEmbed).mAnimationFilePath(BinString))?.0;
                        let path = path.strip_suffix(".anm")?;
                        let (_, path) = path.split_once('/')?;
                        let path: String = path.chars().scan(false, |upper, c| {
                            if c == '_' {
                                *upper = true;
                                Some('_')
                            } else if *upper {
                                *upper = false;
                                Some(c.to_ascii_uppercase())
                            } else {
                                Some(c)
                            }
                        }).collect();
                        let it = path.rmatch_indices('_').map(|(i, _)| &path[i+1..]);
                        finder.check_one_from_iter(BinHashKind::HashValue, hash, it);
                    }
                    None
                }

                if let Some(map) = binget!(entry => mClipDataMap(BinMap)(BinHash, BinStruct)) {
                    for (hash, clip_data) in map {
                        check_clip_data(hash.0.hash, clip_data, finder);
                    }
                }
            })

    }

    /// Add hooks that use collected entry types
    pub fn with_collecting_hooks(self) -> Self {
        self
            .with_hook(Box::<ItemHashListsHook>::default())
            .with_hook(Box::<ScriptPathsHook>::default())
            .with_hook(Box::<CharacterEntriesHook>::default())
            .with_hook(Box::<GameModeLinksHook>::default())
    }

    /// End guessing, return the updated finder
    pub fn result(mut self) -> BinHashFinder {
        for mut hook in self.hooks {
            hook.on_end(&mut self.finder, &self.entries_by_type)
        }
        self.finder
    }

    /// Run the guesser
    pub fn guess_dir<P: AsRef<Path>>(&mut self, root: P) {
        for path in bin_files_from_dir(root) {
            if let Ok(scanner) = PropFile::scan_entries_from_path(path) {
                let mut scanner = scanner.scan();
                while let Some(Ok(item)) = scanner.next() {
                    self.entries_by_type.entry(item.ctype).or_default().push(item.path);
                    if let Some(indexes) = self.registry.get(&item.ctype) {
                        if let Ok(entry) = item.read() {
                            for i in indexes {
                                self.hooks[*i].on_entry(&entry, &mut self.finder);
                            }
                        }
                    }
                }
            }
        }
    }

    /*TODO
    pub fn guess_from_summoner_trophies(&mut self) -> Result<(), PropError> {
        // Formats given in `{89e3706b}.mGDSObjectPathTemplates`
        for entry in PropFile::from_path(self.root.join("global/loadouts/summonertrophies.bin"))?.entries {
            if entry.ctype == binh!("TrophyData") {
                let skeleton = binget!(entry => skinMeshProperties(BinEmbed).skeleton(BinString)).expect("TrophyData skeleton not found");
                // Extract the cup name
                let cup = skeleton.0.split('/').nth(4).expect("TrophyData cup name not found");
                self.finder.check_from_iter(BinHashKind::EntryPath, [4, 8, 16].iter().map(|gem| {
                    format!("Loadouts/SummonerTrophies/Trophies/{}/Trophy_{}", cup, gem)
                }));
            }
        }
        Ok(())
    }

    // Guess `Emblems/{N}` from `data/emblems.bin`
    // Get spells, etc. from non-character .bin (if any)
    */
}


pub struct SingleHook {
    types: [BinClassName; 1],
    on_entry: GuessingFunc,
}

impl SingleHook {
    pub fn new(typ: BinClassName, on_entry: GuessingFunc) -> Self {
        Self { types: [typ], on_entry }
    }
}

impl GuessingHook for SingleHook {
    fn entry_types(&self) -> &[BinClassName] {
        &self.types
    }

    fn on_entry(&mut self, entry: &BinEntry, finder: &mut BinHashFinder) {
        (self.on_entry)(entry, finder)
    }
}

pub struct MultiHook {
    types: &'static [BinClassName],
    on_entry: GuessingFunc,
}

impl MultiHook {
    pub fn new(types: &'static [BinClassName], on_entry: GuessingFunc) -> Self {
        Self { types, on_entry }
    }
}

impl GuessingHook for MultiHook {
    fn entry_types(&self) -> &[BinClassName] {
        self.types
    }

    fn on_entry(&mut self, entry: &BinEntry, finder: &mut BinHashFinder) {
        (self.on_entry)(entry, finder)
    }
}


/// Guess hashes from character data: derived pattern, spells
fn on_character_record_entry(entry: &BinEntry, finder: &mut BinHashFinder) {
    let cname = match &binget!(entry => mCharacterName(BinString)) {
        Some(s) => &s.0,
        None => return,
    };
    let prefix = format!("Characters/{}", cname);

    // Common entries.
    // Note: possible entries actually depend on the character subtype, but it does not cost
    // much to check them all.
    finder.check_any_from_iter(BinHashKind::EntryPath, vec![
        format!("{}", prefix),
        format!("{}/CharacterRecords/Root", prefix),
        format!("{}/CharacterRecords/SLIME", prefix),
        format!("{}/CharacterRecords/URF", prefix),
        format!("{}/Skins/Meta", prefix),
        format!("{}/Skins/Root", prefix),
    ].into_iter());

    // Spells can be found in different "directories", for instance:
    // - common spells `Shared/Spells` (not checked)
    // - children spells of "abilities"
    // - cross-character spells (not checked)
    //
    // SpellObject of abilities are under their AbilityObject
    // - spell: Characters/{char}/Spells/{ability}Ability/{spell}
    // - ability: Characters/{char}/Spells/{ability}Ability
    // Ability spells are under `AbilityObject.mChildSpells`
    // Their `{ability}Ability/{spell}` suffix is also under `CharacterRecord.spellNames`
    //
    // `AbilityObject.mName` and `SpellObject.mScriptName` are unique, but separately.

    let spell_path = |s: &BinString| {
        format!("{}/Spells/{}", prefix, s.0)
    };

    fn attack_slot_name(attack_slot: &BinEmbed) -> Option<&BinString> {
        binget!(attack_slot => mAttackName(BinOption)(BinString))
    }

    // AttackSlotData fields
    if let Some(name) = binget!(entry => basicAttack(BinEmbed)) {
        if let Some(name) = attack_slot_name(name).map(spell_path) {
            finder.check_any(BinHashKind::EntryPath, name);
        }
    }
    if let Some(names) = binget!(entry => extraAttacks(BinList)(BinEmbed)) {
        let it = names.iter().filter_map(attack_slot_name).map(spell_path);
        finder.check_any_from_iter(BinHashKind::EntryPath, it);
    }
    if let Some(names) = binget!(entry => critAttacks(BinList)(BinEmbed)) {
        let it = names.iter().filter_map(attack_slot_name).map(spell_path);
        finder.check_any_from_iter(BinHashKind::EntryPath, it);
    }

    // spellNames, includes `{ability}Ability/{spell}` names
    if let Some(names) = binget!(entry => spellNames(BinList)(BinString)) {
        let it = names.iter().map(spell_path);
        finder.check_any_from_iter(BinHashKind::EntryPath, it);
        // Also check for abilities by removing the basename
        //XXX Do it for ALL spells instead?
        let it = names.iter().filter_map(|name| {
            let parent = name.0.split_once('/')?.0;
            if parent.is_empty() {
                None
            } else {
                Some(format!("{}/Spells/{}", prefix, parent))
            }
        });
        finder.check_any_from_iter(BinHashKind::EntryPath, it);
    }

    // extraSpells, other spells, ignore the false `BaseSpell`
    if let Some(names) = binget!(entry => extraSpells(BinList)(BinString)) {
        let it = names.iter().filter(|v| v.0 != "BaseSpell").map(spell_path);
        finder.check_any_from_iter(BinHashKind::EntryPath, it);
    }
}

/// Guess hashes from skin data
fn on_skin_character_data_entry(entry: &BinEntry, finder: &mut BinHashFinder) {
    let path = finder.get_str(BinHashKind::EntryPath, entry.path.hash)
        .map(|s| s.to_owned())
        .or_else(|| {
            // 1. Assume `championSkinName` format `{character}Skin{N}` format (and strip leading `0`)
            // 2. Get champion name from `iconSquare`, try incrementing skin numbers
            // Note: the skin path could be easily guessed from file name, but we don't have it.
            // Note: `championSkinName` is sometimes equal to the character name
            let s = &binget!(entry => championSkinName(BinString))?.0;
            if let Some((champ, skin_number)) = s.split_once("Skin") {
                let path = format!("Characters/{}/Skins/Skin{}", champ, skin_number.trim_start_matches('0'));
                if finder.check_one(BinHashKind::EntryPath, entry.path.hash, &path) {
                    return Some(path)
                }
            } else {
                let s = &binget!(entry => iconSquare(BinOption)(BinString))?.0;
                let mut split = s.splitn(4, '/');
                if let (Some("ASSETS"), Some("Characters"), Some(character)) = (split.next(), split.next(), split.next()) {
                    let it = (0..200).map(|i| format!("Characters/{}/Skins/Skin{}", character, i));
                    if finder.check_one_from_iter(BinHashKind::EntryPath, entry.path.hash, it) {
                        // `check_one_from_iter()` does not return the found value, get it back
                        return Some(finder.get_str(BinHashKind::EntryPath, entry.path.hash)?.to_owned());
                    }
                }
            }
            None
        });
    let path = match path {
        Some(p) => p,
        None => return,
    };

    if let Some(resolver) = binget!(entry => mResourceResolver(BinLink)) {
        finder.check_one(BinHashKind::EntryPath, resolver.0.hash, format!("{}/Resources", path));
    }

    if let Some(animation) = binget!(entry => skinAnimationProperties(BinEmbed).animationGraphData(BinLink)) {
        let mut split = path.splitn(4, '/');
        if let (Some("Characters"), Some(character), Some("Skins"), Some(skin)) = (split.next(), split.next(), split.next(), split.next()) {
            finder.check_one(BinHashKind::EntryPath, animation.0.hash, format!("Characters/{}/Animations/{}", character, skin));
        }
    }
}

/// Returns the length of the `Skin{N}_` token at the start of `s`, where `{N}` is one or more digits.
/// The comparison ignores ASCII case.
/// Returns `None` if `s` does not start with such a token.
fn skin_token_len(s: &[u8]) -> Option<usize> {
    if s.len() < 4 || !s[..4].eq_ignore_ascii_case(b"skin") {
        return None;
    }
    let digits = s[4..].iter().take_while(|b| b.is_ascii_digit()).count();
    (digits > 0 && s.get(4 + digits) == Some(&b'_')).then_some(4 + digits + 1)
}

/// Returns the length of the `Base_` token at the start of `s`.
/// The comparison ignores ASCII case.
/// Returns `None` if `s` does not start with such a token.
fn base_token_len(s: &[u8]) -> Option<usize> {
    (s.len() >= 5 && s[..5].eq_ignore_ascii_case(b"base_")).then_some(5)
}

/// Returns `name` with each token that `token_len` matches replaced by `replacement`.
/// Returns `None` if `name` contains no such token.
fn replace_tokens(name: &str, token_len: fn(&[u8]) -> Option<usize>, replacement: &str) -> Option<String> {
    let bytes = name.as_bytes();
    let mut replaced = String::with_capacity(name.len());
    let mut found = false;
    // Start of the text that is not copied to `replaced` yet
    let mut start = 0;
    let mut i = 0;
    while i < bytes.len() {
        if let Some(len) = token_len(&bytes[i..]) {
            replaced.push_str(&name[start..i]);
            replaced.push_str(replacement);
            i += len;
            start = i;
            found = true;
        } else {
            i += 1;
        }
    }
    replaced.push_str(&name[start..]);
    found.then_some(replaced)
}

/// Returns `name` without its trailing `_{N}` or `_v{N}` token, where `{N}` is one or more digits.
/// Returns `None` if `name` does not end with such a token.
fn strip_version_suffix(name: &str) -> Option<&str> {
    let (head, token) = name.rsplit_once('_')?;
    let digits = token.strip_prefix(['v', 'V']).unwrap_or(token);
    (!digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit())).then_some(head)
}

/// Returns the map keys to try for a link to an entry whose path ends with `base`.
///
/// The candidates are:
/// - `base` without its `Skin{N}_` tokens, or without its `Base_` tokens
/// - the same names without a trailing version token
/// - `base` with each `Skin{N}_` token replaced by `Base_`
/// - each suffix of `base` that starts after a `_`
/// - `base` without one of its `_`-separated tokens
/// - `base` without two adjacent `_`-separated tokens
fn resource_key_candidates(base: &str) -> Vec<String> {
    const TOKENS: [fn(&[u8]) -> Option<usize>; 2] = [skin_token_len, base_token_len];

    let mut candidates = Vec::new();
    for token_len in TOKENS {
        if let Some(stripped) = replace_tokens(base, token_len, "") {
            if let Some(unversioned) = strip_version_suffix(&stripped) {
                candidates.push(unversioned.to_owned());
            }
            candidates.push(stripped);
        }
    }
    if let Some(replaced) = replace_tokens(base, skin_token_len, "Base_") {
        candidates.push(replaced);
    }

    let tokens: Vec<&str> = base.split('_').collect();
    for i in 1..tokens.len() {
        candidates.push(tokens[i..].join("_"));
    }
    if tokens.len() > 1 {
        for i in 0..tokens.len() {
            candidates.push([&tokens[..i], &tokens[i + 1..]].concat().join("_"));
        }
    }
    if tokens.len() > 2 {
        for i in 0..tokens.len() - 1 {
            candidates.push([&tokens[..i], &tokens[i + 2..]].concat().join("_"));
        }
    }
    candidates
}

/// Returns the prefixes of `name` that end at a word boundary, and `name` itself.
/// A word boundary is before an uppercase letter that follows a lowercase letter or a digit.
fn word_prefixes(name: &str) -> impl Iterator<Item=&str> {
    let bytes = name.as_bytes();
    (1..bytes.len())
        .filter(move |&i| bytes[i].is_ascii_uppercase() && (bytes[i - 1].is_ascii_lowercase() || bytes[i - 1].is_ascii_digit()))
        .map(move |i| &name[..i])
        .chain(std::iter::once(name))
}

/// Hook that guesses the entry paths of script entries from `ScriptName`
///
/// Checked formats:
/// - `Characters/{character}/Scripts/{name}` for `CharScript`
/// - `Maps/Shipping/{map}/Scripts/{name}` for the other script types
#[derive(Default)]
pub struct ScriptPathsHook {
    /// Path hash and `ScriptName` of each `CharScript` entry whose path is still unknown
    char_scripts: Vec<(u32, String)>,
    /// Character names, from `mCharacterName` of the character records
    characters: Vec<String>,
}

impl GuessingHook for ScriptPathsHook {
    fn entry_types(&self) -> &[BinClassName] {
        const TYPES: [BinClassName; 6] = [
            binh!(BinClassName, "CharScript"),
            binh!(BinClassName, "BuffScript"),
            binh!(BinClassName, "LolSpellScript"),
            binh!(BinClassName, "LevelControlScript"),
            binh!(BinClassName, "CharacterRecord"),
            binh!(BinClassName, "TFTCharacterRecord"),
        ];
        &TYPES
    }

    fn on_entry(&mut self, entry: &BinEntry, finder: &mut BinHashFinder) {
        if let Some(name) = binget!(entry => mCharacterName(BinString)) {
            self.characters.push(name.0.clone());
            return;
        }

        let hash = entry.path.hash;
        let name = match binget!(entry => ScriptName(BinString)) {
            Some(s) => &s.0,
            None => return,
        };

        if finder.is_unknown(BinHashKind::EntryPath, hash) {
            if entry.ctype == binh!("CharScript") {
                // Format is `Characters/{character}/Scripts/{name}`
                // Most names are `charscript{character}`
                let found = match name.get(..10).zip(name.get(10..)) {
                    Some((prefix, character)) if prefix.eq_ignore_ascii_case("charscript") => {
                        finder.check_one(BinHashKind::EntryPath, hash, format!("Characters/{}/Scripts/{}", character, name))
                    }
                    _ => false,
                };
                if !found {
                    self.char_scripts.push((hash, name.clone()));
                }
            } else {
                let it = MAP_IDS.iter().map(|i| format!("Maps/Shipping/Map{}/Scripts/{}", i, name));
                if !finder.check_one_from_iter(BinHashKind::EntryPath, hash, it) {
                    finder.check_one(BinHashKind::EntryPath, hash, format!("Maps/Shipping/Common/Scripts/{}", name));
                }
            }
        }

        // The `path` field is the hash of the entry path
        if let Some(path) = finder.get_str(BinHashKind::EntryPath, hash) {
            let path = path.to_owned();
            finder.check_one(BinHashKind::HashValue, hash, path);
        }
    }

    fn on_end(&mut self, finder: &mut BinHashFinder, _entries_by_type: &HashMap<BinClassName, Vec<BinEntryPath>>) {
        // Try each character for the scripts that are not named after their character
        for (hash, name) in &self.char_scripts {
            let it = self.characters.iter().map(|character| format!("Characters/{}/Scripts/{}", character, name));
            if finder.check_one_from_iter(BinHashKind::EntryPath, *hash, it)
            && let Some(path) = finder.get_str(BinHashKind::EntryPath, *hash) {
                let path = path.to_owned();
                finder.check_one(BinHashKind::HashValue, *hash, path);
            }
        }
    }
}

/// Hook that guesses the entry paths of `SpellObject` and `ScriptDataObject` entries
///
/// Checked formats for `ScriptDataObject`:
/// - `Maps/Shipping/Map{id}/ScriptData/{name}`
/// - `Maps/Shipping/Map22/Sets/TFTSet{N}/ScriptData/{name}`
/// - `Characters/{character}/ScriptData/{name}`
///
/// Checked formats for `SpellObject`:
/// - `Characters/{character}/Spells/{name}`
/// - `Characters/{character}/Spells/Attacks/{name}`
/// - `Characters/{character}/Spells/{ability}Ability/{name}`, where `{ability}` is a prefix of `{name}`
///
/// A character is checked only if its name is a prefix of `{name}`.
#[derive(Default)]
pub struct CharacterEntriesHook {
    /// Path hash and `mScriptName` of each `SpellObject` entry whose path is still unknown
    spells: Vec<(u32, String)>,
    /// Path hash and `mName` of each `ScriptDataObject` entry whose path is still unknown
    script_data: Vec<(u32, String)>,
    /// Character names, from `mCharacterName` of the character records
    characters: Vec<String>,
}

impl CharacterEntriesHook {
    /// Returns the characters whose name is a prefix of `name`. The comparison ignores ASCII case.
    fn characters_of<'a>(&'a self, name: &'a str) -> impl Iterator<Item=&'a String> {
        self.characters.iter().filter(move |character| {
            name.get(..character.len()).is_some_and(|prefix| prefix.eq_ignore_ascii_case(character))
        })
    }
}

impl GuessingHook for CharacterEntriesHook {
    fn entry_types(&self) -> &[BinClassName] {
        const TYPES: [BinClassName; 4] = [
            binh!(BinClassName, "SpellObject"),
            binh!(BinClassName, "ScriptDataObject"),
            binh!(BinClassName, "CharacterRecord"),
            binh!(BinClassName, "TFTCharacterRecord"),
        ];
        &TYPES
    }

    fn on_entry(&mut self, entry: &BinEntry, finder: &mut BinHashFinder) {
        if let Some(name) = binget!(entry => mCharacterName(BinString)) {
            self.characters.push(name.0.clone());
            return;
        }

        let hash = entry.path.hash;
        if !finder.is_unknown(BinHashKind::EntryPath, hash) {
            return;
        }
        if entry.ctype == binh!("SpellObject") {
            if let Some(name) = binget!(entry => mScriptName(BinString)) {
                self.spells.push((hash, name.0.clone()));
            }
        } else if let Some(name) = binget!(entry => mName(BinString)) {
            let name = &name.0;
            let it = MAP_IDS.iter().map(|i| format!("Maps/Shipping/Map{}/ScriptData/{}", i, name));
            if finder.check_one_from_iter(BinHashKind::EntryPath, hash, it) {
                return;
            }
            let it = (1..30).map(|i| format!("Maps/Shipping/Map22/Sets/TFTSet{}/ScriptData/{}", i, name));
            if !finder.check_one_from_iter(BinHashKind::EntryPath, hash, it) {
                self.script_data.push((hash, name.clone()));
            }
        }
    }

    fn on_end(&mut self, finder: &mut BinHashFinder, _entries_by_type: &HashMap<BinClassName, Vec<BinEntryPath>>) {
        for (hash, name) in &self.spells {
            let it = self.characters_of(name).flat_map(|character| {
                let dir = format!("Characters/{}/Spells", character);
                [format!("{}/{}", dir, name), format!("{}/Attacks/{}", dir, name)].into_iter()
                    .chain(name.char_indices().skip(1).map(move |(n, _)| format!("{}/{}Ability/{}", dir, &name[..n], name)))
            });
            finder.check_one_from_iter(BinHashKind::EntryPath, *hash, it);
        }
        for (hash, name) in &self.script_data {
            let it = self.characters_of(name).map(|character| format!("Characters/{}/ScriptData/{}", character, name));
            finder.check_one_from_iter(BinHashKind::EntryPath, *hash, it);
        }
    }
}

/// Link of a `GameModeMapData` entry to an entry whose path is unknown
struct GameModeLink {
    /// Path hash of the linked entry
    target: u32,
    /// `Maps/Shipping/{map}` directory of the linking entry
    map_dir: String,
    /// Name of the linking field, without its `m` prefix. `None` for a link in a list.
    field: Option<String>,
}

/// Hook that guesses the entry paths of the entries linked by `GameModeMapData`
///
/// The candidate names of a linked entry are the name of the linking field, the class name of
/// the linked entry and the prefixes of this class name. Checked formats, for each mode:
/// - `Maps/Shipping/{map}/GameModeConfigs/{name}_{mode}`
/// - `Maps/Shipping/{map}/GameModeConfigs/{name}`
/// - `Maps/Shipping/{map}/Configs/{name}`
/// - `Maps/Shipping/{map}/{name}`
/// - `Maps/Shipping/Common/{name}`
/// - `UX/HUD/Globals/{name}`
#[derive(Default)]
pub struct GameModeLinksHook {
    links: Vec<GameModeLink>,
    /// Mode names, from the paths of the `GameModeMapData` entries
    modes: HashSet<String>,
}

impl GuessingHook for GameModeLinksHook {
    fn entry_types(&self) -> &[BinClassName] {
        const TYPES: [BinClassName; 1] = [binh!(BinClassName, "GameModeMapData")];
        &TYPES
    }

    fn on_entry(&mut self, entry: &BinEntry, finder: &mut BinHashFinder) {
        // Path format is `Maps/Shipping/{map}/Modes/{mode}`
        let (map_dir, mode) = match finder.get_str(BinHashKind::EntryPath, entry.path.hash).and_then(|s| s.split_once("/Modes/")) {
            Some((map_dir, mode)) => (map_dir.to_owned(), mode.to_owned()),
            None => return,
        };
        self.modes.insert(mode);

        for field in &entry.fields {
            if let Some(link) = field.downcast::<BinLink>() {
                if finder.is_unknown(BinHashKind::EntryPath, link.0.hash) {
                    let name = finder.get_str(BinHashKind::FieldName, field.name.hash).map(|s| {
                        match s.strip_prefix('m') {
                            Some(rest) if rest.starts_with(|c: char| c.is_ascii_uppercase()) => rest,
                            _ => s,
                        }.to_owned()
                    });
                    self.links.push(GameModeLink { target: link.0.hash, map_dir: map_dir.clone(), field: name });
                }
            } else if let Some(links) = field.downcast::<BinList>().and_then(|list| list.downcast::<BinLink>()) {
                for link in links {
                    if finder.is_unknown(BinHashKind::EntryPath, link.0.hash) {
                        self.links.push(GameModeLink { target: link.0.hash, map_dir: map_dir.clone(), field: None });
                    }
                }
            }
        }
    }

    fn on_end(&mut self, finder: &mut BinHashFinder, entries_by_type: &HashMap<BinClassName, Vec<BinEntryPath>>) {
        if self.links.is_empty() {
            return;
        }

        // Get the class name of each linked entry
        let targets: HashSet<u32> = self.links.iter().map(|link| link.target).collect();
        let mut class_names: HashMap<u32, String> = HashMap::new();
        for (ctype, paths) in entries_by_type {
            if let Some(name) = finder.get_str(BinHashKind::ClassName, ctype.hash) {
                for path in paths.iter().filter(|path| targets.contains(&path.hash)) {
                    class_names.insert(path.hash, name.to_owned());
                }
            }
        }

        for link in &self.links {
            let class_name = class_names.get(&link.target);
            let names = link.field.as_deref().into_iter()
                .chain(class_name.into_iter().flat_map(|name| word_prefixes(name)));
            let it = names.flat_map(|name| {
                self.modes.iter()
                    .map(move |mode| format!("{}/GameModeConfigs/{}_{}", link.map_dir, name, mode))
                    .chain([
                        format!("{}/GameModeConfigs/{}", link.map_dir, name),
                        format!("{}/Configs/{}", link.map_dir, name),
                        format!("{}/{}", link.map_dir, name),
                        format!("Maps/Shipping/Common/{}", name),
                        format!("UX/HUD/Globals/{}", name),
                    ])
            });
            finder.check_one_from_iter(BinHashKind::EntryPath, link.target, it);
        }
    }
}

/// Guess lists of item hashes
#[derive(Default)]
pub struct ItemHashListsHook {
    hashes: HashSet<u32>,
}

impl ItemHashListsHook {
    fn extend_with_list(&mut self, field: Option<&BinList>) {
        if let Some(field) = field {
            if let Some(list) = binget!(field => (BinHash)) {
                self.hashes.extend(list.iter().map(|v| v.0.hash));
            }
        }
    }
}

impl GuessingHook for ItemHashListsHook {
    fn entry_types(&self) -> &[BinClassName] {
        const TYPES: [BinClassName; 2] = [
            binh!(BinClassName, "ItemShopGameModeData"),
            binh!(BinClassName, "GameModeItemList"),
        ];
        &TYPES
    }

    fn on_entry(&mut self, entry: &BinEntry, _finder: &mut BinHashFinder) {
        if entry.ctype == binh!("ItemShopGameModeData") {
            self.extend_with_list(binget!(entry => RecOverrideSmiteStartingItems(BinList)));
            self.extend_with_list(binget!(entry => PurchasedItemExclusionItems(BinList)));
            self.extend_with_list(binget!(entry => CompletedItems(BinList)));
            self.extend_with_list(binget!(entry => ConsumablesQuickBuyData(BinStruct).items(BinList)));
        } else if entry.ctype == binh!("GameModeItemList") {
            self.extend_with_list(binget!(entry => mItems(BinList)));
        }
    }

    fn on_end(&mut self, finder: &mut BinHashFinder, entries_by_type: &HashMap<BinClassName, Vec<BinEntryPath>>) {
        // Filter out known hashes
        self.hashes.retain(|h| finder.is_unknown(BinHashKind::HashValue, *h));
        if !self.hashes.is_empty() {
            if let Some(candidates) = entries_by_type.get(&binh!("ItemData")) {
                let candidates: Vec<String> = candidates.iter()
                    .map(|h| h.hash)
                    .filter(|h| self.hashes.contains(h))
                    .filter_map(|h| finder.get_str(BinHashKind::EntryPath, h))
                    .map(|s| s.to_owned())
                    .collect();
                finder.check_selected_from_iter(BinHashKind::HashValue, &self.hashes, candidates.iter());
            }
        }
    }
}


/// Hook to dump some information about entry types
#[derive(Default)]
pub struct EntryTypesStatsHook;

impl GuessingHook for EntryTypesStatsHook {
    fn entry_types(&self) -> &[BinClassName] {
        &[]
    }

    fn on_entry(&mut self, _entry: &BinEntry, _finder: &mut BinHashFinder) {}

    fn on_end(&mut self, finder: &mut BinHashFinder, entries_by_type: &HashMap<BinClassName, Vec<BinEntryPath>>) {
        // Filter out known hashes
        for (ctype, paths) in entries_by_type.iter() {
            let hstr = finder.seek_str(BinHashKind::ClassName, ctype.hash);
            let nall = paths.len();
            let nunknown = paths.iter().filter(|h| finder.is_unknown(BinHashKind::EntryPath, h.hash)).count();
            println!("?: {:5} / {:5}  |  {}", nunknown, nall, hstr);
        }
    }
}
