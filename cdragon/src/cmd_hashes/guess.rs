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
    /// Return false if still unknown
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
        const NAMED_TYPES: [BinClassName; 30] = [
            binh!(BinClassName, "StaticMaterialDef"),
            binh!(BinClassName, "UISceneData"),
            binh!(BinClassName, "UiElementGroupButtonData"),
            binh!(BinClassName, "UiElementGroupData"),
            binh!(BinClassName, "UiElementGroupFramedData"),
            binh!(BinClassName, "UiElementGroupMeterData"),
            binh!(BinClassName, "UiElementGroupSliderData"),
            BinClassName { hash: 0x376d5bc9 },
            BinClassName { hash: 0x39ce5bdd },
            BinClassName { hash: 0x4218b45a },
            BinClassName { hash: 0x5e447d8d },
            BinClassName { hash: 0x89a3465f },
            BinClassName { hash: 0x8c5a7cbe },
            BinClassName { hash: 0x97caa9c2 },
            BinClassName { hash: 0x9b4cc4fd },
            BinClassName { hash: 0xa742684a },
            BinClassName { hash: 0xa7ec17a8 },
            BinClassName { hash: 0xb1e9be66 },
            BinClassName { hash: 0xb6d4a0f9 },
            BinClassName { hash: 0xc209ee16 },
            BinClassName { hash: 0xc3e489da },
            BinClassName { hash: 0xc8336f7d },
            BinClassName { hash: 0xc9e1a631 },
            BinClassName { hash: 0xde9a30e2 },
            BinClassName { hash: 0xe4e463fe },
            BinClassName { hash: 0xe9f32215 },
            BinClassName { hash: 0xf726b035 },
            BinClassName { hash: 0x0a5d0595 },
            BinClassName { hash: 0x1a65e950 },
            BinClassName { hash: 0xd71d9476 },
        ];

        self
            .with_hook(EntryPathAttrHook!([&NAMED_TYPES].name))
            .with_hook(EntryPathAttrHook!(ContextualActionData.mObjectPath))
            .with_hook(EntryPathAttrHook!(CustomShaderDef.objectPath))
            .with_hook(EntryPathAttrHook!(MapContainer.mapPath))
            .with_hook(EntryPathAttrHook!(RewardGroup.internalName))
            .with_hook(EntryPathAttrHook!(VfxSystemDefinitionData.particlePath))
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
            .with_hook(EntryPathPatternHook!(MapSkin.name => "Maps/Shipping/Map11/MapSkins/{}"))
    }

    /// Add relatively simple (but not trivial) hooks
    pub fn with_simple_hooks(self) -> Self {
        const RESOURCE_RESOLVER_TYPES: [BinClassName; 2] = [
            binh!(BinClassName, "ResourceResolver"),
            binh!(BinClassName, "GlobalResourceResolver"),
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
                                finder.check_one(BinHashKind::HashValue, k.0.hash, base);
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
                    if finder.check_one(BinHashKind::EntryPath, entry.path.hash, format!("Items/Spells/{}", name)) {
                        return;
                    }
                    if let Some((id, _)) = name.split_once(|c: char| !c.is_ascii_digit()) {
                        finder.check_one(BinHashKind::EntryPath, entry.path.hash, format!("Items/{}/Spells/{}", id, name));
                    }
                }
                //.with_hook(EntryPathPatternHook!(SpellObject.mScriptName => "Items/Spells/{}"))
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

            // Guess CompanionSpeciesData path from CompanionData.speciesLink
            .with_single_hook(binh!("CompanionData"), |entry, finder| {
                if finder.is_unknown(BinHashKind::EntryPath, entry.path.hash) {
                    if let Some(s) = binget!(entry => speciesLink(BinString)) {
                        finder.check_any(BinHashKind::EntryPath, &s.0);
                    }
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

    }

    /// Add guessing from character data
    pub fn with_character_hooks(self) -> Self {
        const CHARACTER_RECORDS: [BinClassName; 2] = [
            binh!(BinClassName, "CharacterRecord"),
            binh!(BinClassName, "TFTCharacterRecord"),
        ];

        self
            .with_multi_hook(&CHARACTER_RECORDS, on_character_record_entry)
            .with_single_hook(binh!("SkinCharacterDataProperties"), on_skin_character_data_entry)
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
            // Try to guess the skin path
            // Assume `{character}Skin{N}` format, but it does not work for TFT and others
            //XXX Previous guessing used the .bin path. Iterate on all numbers?
            let s = &binget!(entry => championSkinName(BinString))?.0;
            let (champ, skin) = s.rfind("Skin").map(|i| s.split_at(i))?;
            let path = format!("Characters/{}/Skins/{}", champ, skin);
            if finder.check_one(BinHashKind::EntryPath, entry.path.hash, &path) {
                Some(path)
            } else {
                None
            }
        });
    let path = match path {
        Some(p) => p,
        None => return,
    };

    if let Some(resolver) = binget!(entry => mResourceResolver(BinLink)) {
        finder.check_one(BinHashKind::EntryPath, resolver.0.hash, format!("{}/Resources", path));
    }

    if let Some(animation) = binget!(entry => skinAnimation(BinStruct).animationGraphData(BinLink)) {
        let mut split = path.rsplitn(3, '/');
        if let (Some(character), Some("Skins"), Some(skin)) = (split.next(), split.next(), split.next()) {
            finder.check_one(BinHashKind::EntryPath, animation.0.hash, format!("{}/Animation/{}", character, skin));
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
            self.extend_with_list(binget!(entry => 0xc561f8e9(BinList)));
            self.extend_with_list(binget!(entry => 0x37792a41(BinList)));
            self.extend_with_list(binget!(entry => CompletedItems(BinList)));
            self.extend_with_list(binget!(entry => 0x891a5676(BinStruct).items(BinList)));
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

