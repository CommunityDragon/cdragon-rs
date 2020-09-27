use std::path::{Path, PathBuf};
use super::{
    PropFile,
    BinHashKind,
    BinHashSets,
    BinHashMappers,
    compute_binhash,
    Result,
};
use super::data::*;
use walkdir::{WalkDir, DirEntry};


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

    /// Find all character names from Character and derived entries
    pub fn collect_character_names(&self) -> Result<Vec<String>> {
        let mut names = Vec::new();

        let paths = [
            "data/maps/shipping/common/common.bin",
            "data/maps/shipping/map11/map11.bin",
            "data/maps/shipping/map12/map12.bin",
            "data/maps/shipping/map21/map21.bin",
            "data/maps/shipping/map22/map22.bin",
            "global/champions/champions.bin",
        ];
        let is_char_type = |_, htype| htype == binh!("Character") || htype == binh!("Champion") || htype == binh!("Companion");

        for path in &paths {
            let scanner = PropFile::scan_entries_from_path(self.root.join(path))?;
            for entry in scanner.filter_parse(is_char_type) {
                let entry = entry?;
                if let Some(name) = entry.getv::<BinString>(binh!("name")) {
                    names.push(name.0.clone());
                }
            }
        }

        Ok(names)
    }

    /// Guess everything possible
    pub fn guess_all(&mut self) -> Result<()> {
        self.guess_from_characters()?;
        self.guess_from_items()?;
        self.guess_from_companions()?;
        self.guess_from_summoner_emotes()?;
        self.guess_from_tft_map_skins()?;
        self.guess_from_perks()?;
        self.guess_from_shaders()?;
        Ok(())
    }

    /// Guess hashes from all characters
    pub fn guess_from_characters(&mut self) -> Result<()> {
        for name in self.collect_character_names()? {
            self.guess_from_character(&name)?;
        }
        Ok(())
    }

    /// Guess hashes from character name
    pub fn guess_from_character(&mut self, name: &str) -> Result<()> {
        let prefix = format!("Characters/{}", name);
        let lname = name.to_ascii_lowercase();
        let path = self.root.join("data/characters").join(&lname);

        // Guess common hashes
        // Note: possible entries actually depend on the character subtype, but it does not cost
        // much to check them all.
        self.finder.check_from_iter(BinHashKind::EntryPath, vec![
            format!("{}", prefix),
            format!("{}/CharacterRecords/Root", prefix),
            format!("{}/CharacterRecords/SLIME", prefix),
            format!("{}/CharacterRecords/URF", prefix),
            format!("{}/Skins/Meta", prefix),
            format!("{}/Skins/Root", prefix),
        ].into_iter());

        // Open character's bin file
        let scanner = PropFile::scan_entries_from_path(path.join(format!("{}.bin", lname)))?;
        for entry in scanner.filter_parse(|_, htype| htype == binh!("CharacterRecord")) {
            let entry = entry?;
            if entry.ctype == binh!("CharacterRecord") {
                //XXX Spells can be found in different "directories"
                // A lot are in `Shared/Spells` but there could be cross-character spells too
                if let Some(spell_names) = entry.getv::<BinList>(binh!("spellNames")) {
                    let it = spell_names.downcast::<BinString>().unwrap().iter()
                        .map(|v| format!("{}/Spells/{}", prefix, v.0));
                    self.finder.check_from_iter(BinHashKind::EntryPath, it);
                }
                if let Some(spell_names) = entry.getv::<BinList>(binh!("extraSpells")) {
                    let it = spell_names.downcast::<BinString>().unwrap().iter()
                        .filter(|v| v.0 != "BaseSpell")
                        .map(|v| format!("{}/Spells/{}", prefix, v.0));
                    self.finder.check_from_iter(BinHashKind::EntryPath, it);
                }
            }
        }

        // Check skins
        for entry in Self::walk_bins(WalkDir::new(path.join("skins")).max_depth(1)) {
            let mut skin_name = entry.path().file_name().unwrap().to_str().unwrap().to_owned();
            if let Some(s) = skin_name.get_mut(0..1) {
                s.make_ascii_uppercase();
            }
            let prefix = format!("{}/Skins/{}", prefix, skin_name);

            self.finder.check_from_iter(BinHashKind::EntryPath, vec![
                format!("{}", prefix),
                format!("{}/Resources", prefix),
            ].into_iter());

            // Parse all entries, don't bother filtering
            for entry in PropFile::from_path(entry.path())?.entries {
                if entry.ctype == binh!("SkinCharacterDataProperties") {
                    //TODO mContextualActionData is usually included in another file
                    entry.getv::<BinString>(binh!("name")).map(|v| self.finder.check(BinHashKind::EntryPath, &v.0));
                } else if entry.ctype == binh!("StaticMaterialDef") {
                    entry.getv::<BinString>(binh!("name")).map(|v| self.finder.check(BinHashKind::EntryPath, &v.0));
                } else if entry.ctype == binh!("ContextualActionData") {
                    entry.getv::<BinString>(binh!("mObjectPath")).map(|v| self.finder.check(BinHashKind::EntryPath, &v.0));
                } else if entry.ctype == binh!("VfxSystemDefinitionData") {
                    entry.getv::<BinString>(binh!("mParticlePath")).map(|v| self.finder.check(BinHashKind::EntryPath, &v.0));
                }
            }
        }

        // Check animations
        for entry in Self::walk_bins(WalkDir::new(path.join("animations")).max_depth(1)) {
            let mut skin_name = entry.path().file_name().unwrap().to_str().unwrap().to_owned();
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
            } else if entry.ctype == binh!("VfxSystemDefinitionData") {
                entry.getv::<BinString>(binh!("mParticlePath")).map(|v| self.finder.check(BinHashKind::EntryPath, &v.0));
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
            } else if entry.ctype == binh!("VfxSystemDefinitionData") {
                entry.getv::<BinString>(binh!("mParticlePath")).map(|v| self.finder.check(BinHashKind::EntryPath, &v.0));
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

    pub fn guess_from_perks(&mut self) -> Result<()> {
        for entry in PropFile::from_path(self.root.join("global/perks/perks.bin"))?.entries {
            if entry.ctype == binh!("VfxSystemDefinitionData") {
                entry.getv::<BinString>(binh!("mParticlePath")).map(|v| self.finder.check(BinHashKind::EntryPath, &v.0));
            }
        }
        Ok(())
    }

    pub fn guess_from_shaders(&mut self) -> Result<()> {
        for entry in PropFile::from_path(self.root.join("data/shaders/shaders.bin"))?.entries {
            if entry.ctype == binh!("CustomShaderDef") {
                entry.getv::<BinString>(binh!("objectPath")).map(|v| self.finder.check(BinHashKind::EntryPath, &v.0));
            }
        }
        Ok(())
    }

    // Guess `Emblems/{N}` from `data/emblems.bin`
    // Paths of some entry types can be found directly in the entry itself; factorize these cases if they can be found in multiple files
    //   `VfxSystemDefinitionData`
    //   `CustomShaderDef`
    // Get spells, etc. from non-character .bin (if any)


    /// Helper to filter "good" bin entries from a `WalkDir`
    fn walk_bins(walk: WalkDir) -> impl Iterator<Item=DirEntry> {
        walk.into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().map_or(false, |v| v == "bin"))
    }
}


/*

files with character records
    game/data/maps/shipping/common/common.bin
    game/data/maps/shipping/map11/map11.bin
    game/data/maps/shipping/map12/map12.bin
    game/data/maps/shipping/map21/map21.bin
    game/data/maps/shipping/map22/map22.bin
    game/global/champions/champions.bin

- root of bin files
- list of known hashes
- list of unknown hashes
  - parsed from bin files
  - or from given files (faster)

- characters
  - guess list from entries


 TFT: list of 

<BinEntry 'Characters/TFT_Garen' Character [
  <name STRING 'TFT_Garen'>
]>


Main game, list of:

<BinEntry 'Characters/Garen' Champion [
  <name STRING 'Garen'>

game/global/champions/champions.bin.json

11c780d4 Characters/TFT2_Aatrox
6e5ef1e6 Characters/TFT2_Aatrox/Animations/Skin0
a7c11057 Characters/TFT2_Aatrox/CharacterRecords/Root
ff79ff32 Characters/TFT2_Aatrox/Skins/Root
234a8029 Characters/TFT2_Aatrox/Skins/Skin0


*/
