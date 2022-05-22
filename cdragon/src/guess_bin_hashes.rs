use std::path::{Path, PathBuf};
use std::collections::HashMap;
use walkdir::{WalkDir, DirEntry};
use cdragon_utils::Result;
use cdragon_prop::{
    NON_PROP_BASENAMES,
    PropFile,
    BinHashKind,
    BinHashSets,
    BinHashMappers,
    compute_binhash,
    compute_binhash_const,
    binh,
    binget,
};
use cdragon_prop::data::*;


/// Base object to check bin hashes
pub struct BinHashFinder<'a> {
    hashes: &'a mut BinHashSets,
    hmappers: &'a mut BinHashMappers,
    /// Callback called when a new hash is found
    pub on_found: fn(u32, &str),
}

impl<'a> BinHashFinder<'a> {
    pub fn new(hashes: &'a mut BinHashSets, hmappers: &'a mut BinHashMappers) -> Self {
        Self { hashes, hmappers, on_found: |_, _| {} }
    }

    /// Check a single hash
    pub fn check<S: Into<String> + AsRef<str>>(&mut self, kind: BinHashKind, value: S) {
        let hash = compute_binhash(value.as_ref());
        if self.hashes.get_mut(kind).remove(&hash) {
            (self.on_found)(hash, value.as_ref());
            self.hmappers.get_mut(kind).insert(hash, value.into());
        }
    }

    /// Check hashes from an iterable
    pub fn check_from_iter<S: Into<String> + AsRef<str>>(&mut self, kind: BinHashKind, values: impl Iterator<Item=S>) {
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
}


/// Guess bin hashes from bin files and hashes
pub struct BinHashGuesser<'a> {
    root: PathBuf,
    finder: BinHashFinder<'a>,
}

impl<'a> BinHashGuesser<'a> {
    pub fn new<P: AsRef<Path>>(root: P, finder: BinHashFinder<'a>) -> Self {
        let root = root.as_ref();
        let root = if root.ends_with("game") {
            root.to_path_buf()
        } else {
            root.join("game")
        };
        Self { root, finder }
    }

    /// Find character names, based on `data/characters/` subdirectories
    pub fn collect_character_names(&self) -> Vec<String> {
        WalkDir::new(self.root.join("data/characters")).max_depth(1)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter_map(|e| {
                let path = e.path();
                let name = path.file_name().unwrap().to_str().unwrap();
                if path.join(format!("{}.bin", name)).is_file() {
                    Some(name.to_owned())
                } else {
                    None
                }
            }).collect()
    }

    /// Guess everything possible
    pub fn guess_all(&mut self) -> Result<()> {
        self.guess_common_entry_types_paths()?;
        self.guess_from_characters()?;
        self.guess_from_items()?;
        self.guess_from_companions()?;
        self.guess_from_summoner_emotes()?;
        self.guess_from_summoner_trophies()?;
        self.guess_from_tft_map_skins()?;
        self.guess_from_shaders_shareddata()?;
        self.guess_from_fonts()?;
        self.guess_from_tooltips()?;
        Ok(())
    }

    /// Guess hashes from all characters
    pub fn guess_from_characters(&mut self) -> Result<()> {
        for name in self.collect_character_names() {
            self.guess_from_character(&name)?;
        }
        Ok(())
    }

    /// Guess hashes from character name (case insensitive)
    pub fn guess_from_character(&mut self, name: &str) -> Result<()> {
        let name = name.to_ascii_lowercase();
        let path = self.root.join("data/characters").join(&name);

        let mut cname: Option<String> = None;
        let mut prefix: Option<String> = None;
        // Collect spell and ability names in a shared map
        let mut spell_names = HashMap::<BinEntryPath, String>::new();
        let mut abilities: Option<Vec<BinEntryPath>> = None;
        let mut ability_spells = HashMap::<BinEntryPath, Vec<BinEntryPath>>::new();

        const FILTERED_TYPES: [u32; 4] = [
            compute_binhash_const("CharacterRecord"),
            compute_binhash_const("TFTCharacterRecord"),
            compute_binhash_const("SpellObject"),
            compute_binhash_const("AbilityObject"),
        ];

        // Open character's bin file
        //TODO Add a method to scan and parse after checking the entry type
        let scanner = PropFile::scan_entries_from_path(path.join(format!("{}.bin", name)))?;
        for entry in scanner.filter_parse(|_, htype| FILTERED_TYPES.contains(&htype.hash)) {
            let entry = entry?;
            if entry.ctype == binh!("CharacterRecord") || entry.ctype == binh!("TFTCharacterRecord") {
                cname = {
                    if let Some(s) = binget!(entry => mCharacterName(BinString)) {
                        Some(s.0.clone())
                    } else {
                        break;  // no character name, no need to continue
                    }
                };
                let cname = cname.as_ref().unwrap();
                prefix = Some(format!("Characters/{}", cname));
                let prefix = prefix.as_ref().unwrap();

                // Now that we have the capitalized prefix, guess common entries.
                // Note: possible entries actually depend on the character subtype, but it does not cost
                // much to check them all.
                self.finder.check_from_iter(BinHashKind::EntryPath, vec![
                    format!("{}", prefix),
                    format!("{}/CharacterRecords/Root", prefix),
                    format!("{}/CharacterRecords/SLIME", prefix),
                    format!("{}/CharacterRecords/URF", prefix),
                    format!("{}/Skins/Meta", prefix),
                    format!("{}/Skins/Root", prefix),
                    format!("{}/CAC/{}_Base", prefix, cname),
                ].into_iter());

                // Spells can be found in different "directories", for instance:
                // - common spells `Shared/Spells` (not checked)
                // - children spells of "abilities"
                // - cross-character spells (not checked)
                if let Some(spell_names) = binget!(entry => spellNames(BinList)(BinString)) {
                    let it = spell_names.iter()
                        .map(|v| format!("{}/Spells/{}", prefix, v.0));
                    self.finder.check_from_iter(BinHashKind::EntryPath, it);
                }
                if let Some(spell_names) = binget!(entry => extraSpells(BinList)(BinString)) {
                    let it = spell_names.iter()
                        .filter(|v| v.0 != "BaseSpell")
                        .map(|v| format!("{}/Spells/{}", prefix, v.0));
                    self.finder.check_from_iter(BinHashKind::EntryPath, it);
                }
                abilities = binget!(entry => mAbilities(BinList)(BinLink)).and_then(|v| Some(v.iter().map(|v| v.0).collect()));

            } else if entry.ctype == binh!("SpellObject") {
                if let Some(name) = binget!(entry => mScriptName(BinString)) {
                    spell_names.insert(entry.path, name.0.to_owned());
                }
            } else if entry.ctype == binh!("AbilityObject") {
                let name = binget!(entry => mName(BinString)).expect("AbilityObject without a name").0.to_owned();
                spell_names.insert(entry.path, name);
                let root_spell = binget!(entry => mRootSpell(BinLink)).expect("AbilityObject without a root spell").0;
                let mut spells: Vec<BinEntryPath> = vec![root_spell];
                if let Some(child_spells) = binget!(entry => mChildSpells(BinList)(BinLink)) {
                    spells.extend(child_spells.iter().map(|v| v.0));
                }
                ability_spells.insert(entry.path, spells);
            }
        }

        // Some characters are (almost) empty, ignore them
        // Note that they usually have a `/Skins/Meta` entry which will not be guessed.
        if cname.is_none() {
            return Ok(());
        }
        let cname = cname.unwrap();
        let prefix = prefix.unwrap();

        // Check all collected spell names, just in case
        self.finder.check_from_iter(BinHashKind::EntryPath, spell_names.values().map(|s| format!("{}/Spells/{}", prefix, s)));
        // Check spells from abilities
        if let Some(abilities) = abilities {
            for hability in abilities {
                // Some related characters use cross-character abilities (ex. Elemental Lux)
                if let Some(ability_name) = spell_names.get(&hability) {
                    self.finder.check_from_iter(BinHashKind::EntryPath, ability_spells[&hability].iter().map(|h| format!("{}/Spells/{}/{}", prefix, ability_name, spell_names[h])));
                }
            }
        }

        // Check skins
        for direntry in Self::walk_bins(WalkDir::new(path.join("skins")).max_depth(1)) {
            let mut skin_name = direntry.path().file_stem().unwrap().to_str().unwrap().to_owned();
            if let Some(s) = skin_name.get_mut(0..1) {
                s.make_ascii_uppercase();
            }
            let sprefix = format!("{}/Skins/{}", prefix, skin_name);

            self.finder.check_from_iter(BinHashKind::EntryPath, vec![
                format!("{}", sprefix),
                format!("{}/Resources", sprefix),
                // Try both `Skins0X` and `SkinsX`
                format!("{}/CAC/{}_{}", prefix, cname, skin_name),
                format!("{}/CAC/{}_{}", prefix, cname, skin_name.replace("Skins", "Skins0")),
            ].into_iter());

            // Parse all entries, don't bother filtering
            for entry in PropFile::from_path(direntry.path())?.entries {
                if entry.ctype == binh!("SkinCharacterDataProperties") {
                    //TODO mContextualActionData is usually included in another file
                    binget!(entry => name(BinString)).map(|v| self.finder.check(BinHashKind::EntryPath, &v.0));
                }
            }
        }

        // Check animations
        for direntry in Self::walk_bins(WalkDir::new(path.join("animations")).max_depth(1)) {
            let mut skin_name = direntry.path().file_stem().unwrap().to_str().unwrap().to_owned();
            if let Some(s) = skin_name.get_mut(0..1) {
                s.make_ascii_uppercase();
            }
            let prefix = format!("{}/Animations/{}", prefix, skin_name);

            self.finder.check(BinHashKind::EntryPath, prefix);

            //TODO Guess `mClipDataMap` keys from `.anm` paths (`{character}_{key}`)
        }

        Ok(())
    }

    /// Guess hashes from items
    pub fn guess_from_items(&mut self) -> Result<()> {
        for entry in PropFile::from_path(self.root.join("global/items/items.bin"))?.entries {
            if entry.ctype == binh!("SpellObject") {
                entry.getv::<BinString>(binh!("mScriptName")).map(|v| self.finder.check(BinHashKind::EntryPath, format!("Items/Spells/{}", v.0)));
            } else if entry.ctype == binh!("ItemData") {
                entry.getv::<BinS32>(binh!("itemID")).map(|v| self.finder.check(BinHashKind::EntryPath, format!("Items/{}", v.0)));
            }
        }

        //XXX For `ItemGroups`, entry path and `mItemGroupID` are linked

        Ok(())
    }

    pub fn guess_from_companions(&mut self) -> Result<()> {
        for entry in PropFile::from_path(self.root.join("global/loadouts/companions.bin"))?.entries {
            if entry.ctype == binh!("CompanionData") {
                entry.getv::<BinString>(binh!("speciesLink")).map(|v| self.finder.check(BinHashKind::EntryPath, &v.0));
            }
        }
        Ok(())
    }

    pub fn guess_from_summoner_emotes(&mut self) -> Result<()> {
        for entry in PropFile::from_path(self.root.join("global/loadouts/summoneremotes.bin"))?.entries {
            if entry.ctype == binh!("CompanionData") {
                entry.getv::<BinString>(binh!("speciesLink")).map(|v| self.finder.check(BinHashKind::EntryPath, &v.0));
            } else if entry.ctype == binh!("SummonerEmote") {
                entry.getv::<BinU32>(binh!("summonerEmoteId")).map(|v| self.finder.check(BinHashKind::EntryPath, format!("Loadouts/SummonerEmotes/{}", v.0)));
            }
        }
        Ok(())
    }

    pub fn guess_from_summoner_trophies(&mut self) -> Result<()> {
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

    pub fn guess_from_tft_map_skins(&mut self) -> Result<()> {
        for entry in PropFile::from_path(self.root.join("global/loadouts/tftmapskins.bin"))?.entries {
            if entry.ctype == binh!("TftMapSkin") {
                entry.getv::<BinString>(binh!("mapContainer")).map(|v| {
                    let name = v.0.rsplit('/').next().unwrap();
                    self.finder.check(BinHashKind::EntryPath, format!("Loadouts/TFTMapSkins/{}", name))
                });
                entry.getv::<BinString>(0xfb59da5c.into()).map(|v| self.finder.check(BinHashKind::EntryPath, &v.0));
            }
        }
        Ok(())
    }

    pub fn guess_from_shaders_shareddata(&mut self) -> Result<()> {
        for entry in PropFile::from_path(self.root.join("assets/shaders/shareddata.bin"))?.entries {
            if entry.ctype == binh!("X3DSharedConstantBufferDef") {
                let name = &binget!(entry => name(BinString)).expect("missing name").0;
                self.finder.check(BinHashKind::EntryPath, format!("Shaders/SharedData/{}", name));
            }
        }
        Ok(())
    }

    pub fn guess_from_fonts(&mut self) -> Result<()> {
        for entry in PropFile::from_path(self.root.join("ux/fonts.bin"))?.entries {
            if entry.ctype == binh!("GameFontDescription") {
                let name = &binget!(entry => name(BinString)).expect("missing name").0;
                self.finder.check(BinHashKind::EntryPath, format!("UX/Fonts/Descriptions/{}", name));
            }
        }
        Ok(())
    }

    pub fn guess_from_tooltips(&mut self) -> Result<()> {
        for entry in PropFile::from_path(self.root.join("ux/tooltips.bin"))?.entries {
            if entry.ctype == binh!("TooltipFormat") {
                let name = &binget!(entry => mObjectName(BinString)).expect("missing name").0;
                self.finder.check(BinHashKind::EntryPath, format!("UX/Tooltips/{}", name));
            }
        }
        Ok(())
    }

    /// Guess entry paths for common types
    pub fn guess_common_entry_types_paths(&mut self) -> Result<()> {
        let mut htype_to_field = HashMap::<BinClassName, BinFieldName>::new();
        htype_to_field.insert(binh!("ContextualActionData"), binh!("mObjectPath"));
        htype_to_field.insert(binh!("CustomShaderDef"), binh!("objectPath"));
        htype_to_field.insert(binh!("StaticMaterialDef"), binh!("name"));
        htype_to_field.insert(binh!("MapContainer"), binh!("mapPath"));
        htype_to_field.insert(binh!("MapPlaceableContainer"), binh!("path"));
        htype_to_field.insert(binh!("VfxSystemDefinitionData"), binh!("particlePath"));

        for direntry in Self::walk_bins(WalkDir::new(&self.root)) {
            let scanner = PropFile::scan_entries_from_path(direntry.path())?;
            for entry in scanner.filter_parse(|_, htype| htype_to_field.contains_key(&htype)) {
                let entry = entry?;
                //XXX Cannot use `match` because of `binh!`
                let hfield = htype_to_field[&entry.ctype];
                entry.getv::<BinString>(hfield).map(|v| self.finder.check(BinHashKind::EntryPath, &v.0));
            }
        }
        Ok(())
    }

    // Guess `Emblems/{N}` from `data/emblems.bin`
    // Get spells, etc. from non-character .bin (if any)

    /// Helper to filter "good" bin entries from a `WalkDir`
    fn walk_bins(walk: WalkDir) -> impl Iterator<Item=DirEntry> {
        walk.into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.path().extension().map_or(false, |s| s == "bin") &&
                e.file_name().to_str()
                    .map(|s| !NON_PROP_BASENAMES.contains(&s))
                    .unwrap_or(false)
            })
    }
}

