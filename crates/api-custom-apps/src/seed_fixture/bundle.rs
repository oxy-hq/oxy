//! The fixture's five bundles, from the plain files in
//! `customer-apps/examples/pulse-demo/` (a `base/` and the overlays `r2/`..`r5/`,
//! each holding only the files that release changes), embedded at compile time.
//!
//! Two things are done to the files on the way into a tarball, the two a
//! bundler would otherwise do:
//!
//! - **Assets are content-hashed.** Everything under `assets/` is served with a
//!   one-year `immutable` Cache-Control by prefix, so an `assets/app.js` shared
//!   by five builds would pin a returning browser to whichever it saw first.
//!   `assets/app.js` becomes `assets/app-<sha10>.js` and `index.html` is
//!   rewritten to match.
//! - **Two `<meta>` tags go into `index.html`:** the release label and the
//!   functions its manifest declares, which the page reads to label itself and
//!   to show only the buttons this release can answer.

use std::collections::BTreeMap;

use sha2::{Digest, Sha256};

/// One embedded file: its path in the bundle and its text.
type File = (&'static str, &'static str);

macro_rules! bundle_file {
    ($dir:literal, $path:literal) => {
        (
            $path,
            include_str!(concat!(
                "../../../../customer-apps/examples/pulse-demo/",
                $dir,
                "/",
                $path
            )),
        )
    };
}

const BASE: &[File] = &[
    bundle_file!("base", "index.html"),
    bundle_file!("base", "oxy-app.json"),
    bundle_file!("base", "icon.svg"),
    bundle_file!("base", "assets/app.js"),
    bundle_file!("base", "assets/app.css"),
    bundle_file!("base", "functions/stores.js"),
    bundle_file!("base", "functions/store.js"),
    bundle_file!("base", "functions/snapshot.js"),
    bundle_file!("base", "functions/smoke.js"),
];

const R2: &[File] = &[bundle_file!("r2", "functions/store.js")];
const R3: &[File] = &[
    bundle_file!("r3", "oxy-app.json"),
    bundle_file!("r3", "functions/digest.js"),
];
const R4: &[File] = &[bundle_file!("r4", "functions/store.js")];
const R5: &[File] = &[
    bundle_file!("r5", "oxy-app.json"),
    bundle_file!("r5", "functions/store.js"),
];

/// The overlays in release order; release `k` is `BASE` plus the first `k - 1`.
const OVERLAYS: [&[File]; 4] = [R2, R3, R4, R5];

/// How many releases the fixture has.
pub const RELEASES: usize = OVERLAYS.len() + 1;

/// One release's bundle, ready to publish.
#[derive(Debug, Clone)]
pub struct Release {
    /// 1-based: `r1` .. `r5`.
    pub index: usize,
    /// The publish id: `pulse-r<k>-<sha12>` of the final files, so an edited
    /// file is a new build id and a rebuilt fixture.
    pub build_id: String,
    /// The bundle as published: hashed asset names, stamped `index.html`.
    pub files: BTreeMap<String, Vec<u8>>,
}

impl Release {
    pub fn label(&self) -> String {
        format!("r{}", self.index)
    }

    /// The merged `oxy-app.json`.
    pub fn manifest(&self) -> serde_json::Value {
        let raw = self
            .files
            .get("oxy-app.json")
            .expect("every release has a manifest");
        serde_json::from_slice(raw).expect("the embedded manifests are JSON")
    }

    /// The functions the manifest declares, sorted.
    pub fn functions(&self) -> Vec<String> {
        let manifest = self.manifest();
        let mut names: Vec<String> = manifest["functions"]
            .as_object()
            .map(|m| m.keys().cloned().collect())
            .unwrap_or_default();
        names.sort();
        names
    }

    /// The tar.gz `oxyc publish` would upload.
    pub fn tarball(&self) -> Vec<u8> {
        let mut builder = tar::Builder::new(flate2::write::GzEncoder::new(
            Vec::new(),
            flate2::Compression::default(),
        ));
        for (path, bytes) in &self.files {
            let mut header = tar::Header::new_gnu();
            header.set_size(bytes.len() as u64);
            header.set_mode(0o644);
            header.set_mtime(0);
            header.set_cksum();
            builder
                .append_data(&mut header, path, bytes.as_slice())
                .expect("append to an in-memory tar");
        }
        builder
            .into_inner()
            .expect("finish an in-memory tar")
            .finish()
            .expect("finish an in-memory gzip stream")
    }
}

/// Every release, `r1` first.
pub fn releases() -> Vec<Release> {
    (1..=RELEASES).map(release).collect()
}

fn release(index: usize) -> Release {
    let mut merged: BTreeMap<&str, &str> = BASE.iter().copied().collect();
    for overlay in &OVERLAYS[..index - 1] {
        merged.extend(overlay.iter().copied());
    }
    let label = format!("r{index}");
    let files = finish(&merged, &label);
    let build_id = format!("pulse-{label}-{}", digest_files(&files, 12));
    Release {
        index,
        build_id,
        files,
    }
}

/// Hash the assets, rewrite `index.html` to their new names and stamp it.
fn finish(merged: &BTreeMap<&str, &str>, label: &str) -> BTreeMap<String, Vec<u8>> {
    let mut out = BTreeMap::new();
    let mut renames = Vec::new();
    for (path, text) in merged {
        let name = match path.strip_prefix("assets/") {
            Some(file) => {
                let hashed = hashed_name(file, text);
                renames.push((path.to_string(), format!("assets/{hashed}")));
                format!("assets/{hashed}")
            }
            None => path.to_string(),
        };
        out.insert(name, text.as_bytes().to_vec());
    }
    let functions = declared_functions(merged["oxy-app.json"]);
    let mut html = merged["index.html"].to_string();
    for (from, to) in &renames {
        html = html.replace(&format!("\"{from}\""), &format!("\"{to}\""));
    }
    let stamp = format!(
        "    <meta name=\"pulse-release\" content=\"{label}\" />\n    \
         <meta name=\"pulse-functions\" content=\"{}\" />\n  </head>",
        functions.join(",")
    );
    html = html.replacen("  </head>", &stamp, 1);
    out.insert("index.html".to_string(), html.into_bytes());
    out
}

/// `app.js` → `app-<sha10>.js`.
fn hashed_name(file: &str, text: &str) -> String {
    let sha = hex::encode(Sha256::digest(text.as_bytes()));
    match file.rsplit_once('.') {
        Some((stem, ext)) => format!("{stem}-{}.{ext}", &sha[..10]),
        None => format!("{file}-{}", &sha[..10]),
    }
}

fn declared_functions(manifest: &str) -> Vec<String> {
    let value: serde_json::Value =
        serde_json::from_str(manifest).expect("the embedded manifests are JSON");
    let mut names: Vec<String> = value["functions"]
        .as_object()
        .map(|m| m.keys().cloned().collect())
        .unwrap_or_default();
    names.sort();
    names
}

/// The first `len` hex chars of a digest over every path and its bytes.
fn digest_files(files: &BTreeMap<String, Vec<u8>>, len: usize) -> String {
    let mut sha = Sha256::new();
    for (path, bytes) in files {
        sha.update(path.as_bytes());
        sha.update([0]);
        sha.update(bytes);
        sha.update([0]);
    }
    hex::encode(sha.finalize())[..len].to_string()
}
