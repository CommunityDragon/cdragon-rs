//! Download game files from Riot's CDN

use std::io::{Read, BufRead, BufReader, BufWriter};
use std::path::Path;
use std::collections::HashMap;
use reqwest::{Url, header, IntoUrl, blocking::{Client, Response}};
use cdragon_utils::{GuardedFile, Result};
use cdragon_rman::FileBundleRanges;
// Re-exports
pub use serde_json;

mod guarded_map;
use guarded_map::GuardedMmap;

pub mod storage;


/// CDN from which game files can be downloaded
pub struct CdnDownloader {
    client: Client,
    url: Url,
}

impl CdnDownloader {
    /// Default CDN URL
    pub const DEFAULT_URL: &'static str = "https://lol.dyn.riotcdn.net";

    /// Use default Riot CDN
    pub fn new() -> Result<Self> {
        Self::from_base_url(Self::DEFAULT_URL)
    }

    /// Use given URL as base for all downloads
    pub fn from_base_url(url: &str) -> Result<Self> {
        let client = Client::new();
        let url = Url::parse(url)?;
        Ok(Self { client, url })
    }

    /// Build a bundle URL path from its ID
    pub fn bundle_path(bundle_id: u64) -> String {
        format!("channels/public/bundles/{:016X}.bundle", bundle_id)
    }

    /// Build a manifest URL path from its ID
    pub fn manifest_path(manifest_id: u64) -> String {
        format!("channels/public/releases/{:016X}.manifest", manifest_id)
    }

    /// Download a CDN path to a file
    pub fn download_path(&self, path: &str, output: &Path) -> Result<()> {
        self.download_url_(self.url.join(path)?, output)
    }

    /// Download any URL to a file, using the instance client
    pub fn download_url<U: IntoUrl>(&self, url: U, output: &Path) -> Result<()> {
        self.download_url_(url.into_url()?, output)
    }

    fn download_url_(&self, url: Url, output: &Path) -> Result<()> {
        let mut response = self.client
            .get(url)
            .send()?
            .error_for_status()?;
        //TODO check if buffering is required for reponse

        let mut gfile = GuardedFile::create(output)?;
        {
            let mut writer = BufWriter::new(gfile.as_file_mut());
            std::io::copy(&mut response, &mut writer)?;
        }
        gfile.persist();

        Ok(())
    }

    /// Download bundle chunks to a file
    pub fn download_bundle_chunks(&self, file_size: u64, bundle_ranges: &FileBundleRanges, path: &Path) -> Result<()> {
        // Open output file, map it to memory
        let mut mmap = GuardedMmap::create(path, file_size)?;

        // Download chunks, bundle per bundle
        for (bundle_id, ranges) in bundle_ranges {
            let cdn_path = Self::bundle_path(*bundle_id);
            // File ranges to slices
            let buf: &mut [u8] = &mut mmap.mmap();
            let mut download_ranges = Vec::<((u32, u32), &mut [u8])>::with_capacity(ranges.len());
            ranges
                .iter()
                .fold((buf, 0), |(buf, offset), range| {
                    let (begin, end) = range.target.clone();
                    let (_, buf) = buf.split_at_mut((begin - offset) as usize);
                    let (out, buf) = buf.split_at_mut((end - begin) as usize);
                    download_ranges.push((range.bundle.clone(), out));
                    (buf, end)
                });
            self.download_ranges(&cdn_path, download_ranges)?;
        }

        mmap.persist();

        Ok(())
    }

    /// Request a path from a CDN using given ranges
    ///
    /// Return a `reqwest::Response` object, which implements `std::io::Read`.
    fn get_ranges(&self, path: &str, ranges: &[(u32, u32)]) -> Result<Response> {
        let url = self.url.join(path)?;
        let range_header = build_range_header(ranges);
        let response = self.client
            .get(url)
            .header(header::RANGE, range_header)
            .send()?
            .error_for_status()?;
        Ok(response)
    }

    /// Download multiple ranges of a bundle to the given buffers
    fn download_ranges(&self, path: &str, ranges: Vec<((u32, u32), &mut [u8])>) -> Result<()> {
        let cdn_ranges: Vec<(u32, u32)> = ranges.iter().map(|r| r.0).collect();
        let response = self.get_ranges(&path, &cdn_ranges)?;

        // Check for multipart response body
        let is_multipart = response.headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .map_or(false, |v| v.starts_with("multipart/byteranges; boundary="));
        let mut reader = BufReader::new(response);

        // Download individual chunks
        for (chunk_range, buf) in ranges.into_iter() {
            // Skip the "multipart/byteranges" header if needed
            if is_multipart {
                // Skip until boundary (lazy check)
                // Only wait for a line starting with "--".
                loop {
                    let mut line = String::new();
                    if reader.read_line(&mut line).expect("read error") == 0 {
                        panic!("range part boundary not found");
                    }
                    if line.starts_with("--") {
                        break;
                    }
                }
                // Skip until part body
                loop {
                    let mut line = String::new();
                    if reader.read_line(&mut line).expect("read error") == 0 {
                        panic!("range part header end not found");
                    }
                    if line.as_str() == "\r\n" {
                        break;
                    }
                }
            }

            let reader = (&mut reader).take((chunk_range.1 - chunk_range.0) as u64);
            let mut decoder = zstd::stream::Decoder::new(reader)?;
            decoder.read_exact(buf)?;
        }

        Ok(())
    }
}


#[derive(Debug)]
pub enum Product {
    /// League of Legends client
    LolClient,
    /// League of Legends game application
    LolGame,
}

/// Information on a specific product release
///
/// This is a common format for all products. Available information are different for each one.
#[derive(Debug)]
pub struct ReleaseInfo {
    pub product: Product,
    /// URL of the manifest with patches to download
    pub manifest_url: String,
    /// Metadata, product-specific
    pub metadata: HashMap<&'static str, String>,
}


/// Get the latest release information of LoL client
pub fn get_latest_lol_client_release(client: &mut Client, patchline: &str, region: &str) -> Result<ReleaseInfo> {
    let url = "https://clientconfig.rpg.riotgames.com/api/v1/config/public?namespace=keystone.products.league_of_legends.patchlines";
    let response = client
        .get(url)
        .send()?
        .error_for_status()?;
    let data: serde_json::Value = serde_json::from_reader(response)?;
    let root_key = format!("keystone.products.league_of_legends.patchlines.{}", patchline);

    let data = &data[root_key];
    let configs = data["configurations"]
        .as_array().ok_or(serde_error("invalid 'configuration' value"))?;
    let config = configs.iter().find(|v| match &v["id"] {
        serde_json::Value::String(s) => s == region,
        _ => false,
    }).ok_or(serde_error("region not found"))?;
    let manifest_url = config["patch_url"]
        .as_str().ok_or(serde_error("invalid 'patch_url' value"))?;

    let mut metadata = HashMap::new();
    metadata.insert("patchline", patchline.into());
    metadata.insert("region", region.into());

    Ok(ReleaseInfo {
        product: Product::LolClient,
        manifest_url: manifest_url.into(),
        metadata
    })
}

/// Get the latest release information of LoL game
pub fn get_latest_lol_game_release(client: &mut Client, platform: &str) -> Result<ReleaseInfo> {
    let url = format!("https://sieve.services.riotcdn.net/api/v1/products/lol/version-sets/{}?q[platform]=windows&q[published]=true", platform);
    let response = client
        .get(&url)
        .send()?
        .error_for_status()?;
    let data: serde_json::Value = serde_json::from_reader(response)?;

    // Note: assume there is only one result
    let data = &data[0];
    let labels = &data["release"]["labels"];
    let revision = labels["riot:revision"]["values"][0]
        .as_str().ok_or(serde_error("unexpected 'riot:revision' type"))?;
    let manifest_url = data["download"]["url"]
        .as_str().ok_or(serde_error("unexpected 'download.url' type"))?;

    let mut metadata = HashMap::new();
    metadata.insert("platform", platform.into());
    metadata.insert("revision", revision.into());

    Ok(ReleaseInfo {
        product: Product::LolClient,
        manifest_url: manifest_url.into(),
        metadata,
    })
}


/// Build Range header value from a list of ranges
fn build_range_header(ranges: &[(u32, u32)]) -> String {
    let http_ranges = ranges
        .iter()
        .map(|(begin, end)| format!("{}-{}", begin, end))
        .collect::<Vec<String>>()
        .join(",");
    format!("bytes={}", http_ranges)
}

/// Build a custom serde error, used when parsing JSON data
fn serde_error<T: std::fmt::Display>(msg: T) -> serde_json::Error {
    use serde::de::Error;
    serde_json::Error::custom(msg)
}

