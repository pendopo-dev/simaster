//! Automatic classification of image CAPTCHA challenges.
//!
//! The ddddocr model is downloaded once into the cache directory rather than
//! committed; classification then works offline. Failures never abort the
//! login — they return `None` and the caller falls back to a human answer.

use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
    sync::OnceLock,
    time::Duration,
};

use ddddocr_core::traits::Loader;
use ddddocr_core::{ModelMetadata, Normalization, OcrBuilder, OcrResult, Resize};
use ddddocr_tract2::{OcrRuntime, loader::ModelLoader};
use sha2::{Digest, Sha256};
use tracing::{info, warn};

/// Upstream ddddocr `common.onnx` (MIT). The classic `common_old.onnx` the
/// prior-art client defaults to quantizes an LSTM op that the pure-Rust tract
/// backend cannot run, so it is not used here.
const MODEL_URL: &str =
    "https://raw.githubusercontent.com/sml2h3/ddddocr/master/ddddocr/common.onnx";
const MODEL_SHA256: &str = "33b5cd351ee94e73a6bf8fa18c415ed8b819b3ffd342e267c30d8ad8334e34e8";
const MAX_MODEL_BYTES: u64 = 128 * 1024 * 1024;

pub(crate) fn cache_path() -> Option<PathBuf> {
    directories::ProjectDirs::from("id", "ugm", "simaster")
        .map(|dirs| dirs.cache_dir().join("common.onnx"))
}

pub(crate) fn verify(bytes: &[u8]) -> Option<()> {
    (format!("{:x}", Sha256::digest(bytes)) == MODEL_SHA256).then_some(())
}

pub(crate) fn read_verified(path: &Path) -> Option<Vec<u8>> {
    let bytes = fs::read(path).ok()?;
    verify(&bytes)?;
    Some(bytes)
}

/// Download once, verify, and cache. A checksum mismatch or a network
/// failure leaves no file behind.
fn fetch(path: &Path) -> Option<Vec<u8>> {
    info!("downloading the CAPTCHA model once to {}", path.display());
    let mut response = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(300))
        .build()
        .ok()?
        .get(MODEL_URL)
        .send()
        .map_err(|error| warn!("CAPTCHA model download failed: {error}"))
        .ok()?
        .error_for_status()
        .map_err(|error| warn!("CAPTCHA model download failed: {error}"))
        .ok()?;
    let mut bytes = Vec::new();
    response
        .by_ref()
        .take(MAX_MODEL_BYTES)
        .read_to_end(&mut bytes)
        .map_err(|error| warn!("CAPTCHA model download failed: {error}"))
        .ok()?;
    verify(&bytes).or_else(|| {
        warn!("CAPTCHA model checksum did not match {MODEL_SHA256}");
        None
    })?;
    let parent = path.parent()?;
    fs::create_dir_all(parent).ok()?;
    let partial = path.with_extension("onnx.partial");
    fs::write(&partial, &bytes).ok()?;
    fs::rename(&partial, path).ok()?;
    Some(bytes)
}

fn model() -> Option<&'static [u8]> {
    static MODEL: OnceLock<Option<Vec<u8>>> = OnceLock::new();
    MODEL
        .get_or_init(|| {
            let path = cache_path()?;
            read_verified(&path).or_else(|| fetch(&path))
        })
        .as_deref()
}

fn engine() -> Option<&'static OcrRuntime> {
    static ENGINE: OnceLock<Option<OcrRuntime>> = OnceLock::new();
    ENGINE
        .get_or_init(|| {
            let session = ModelLoader::default()
                .build_from_bytes(model()?)
                .map_err(|error| warn!("could not load the CAPTCHA model: {error}"))
                .ok()?;
            Some(OcrRuntime::new(
                session,
                ModelMetadata::from_builtin_beta(
                    false,
                    Resize::DynamicWidth(64),
                    1,
                    Normalization::MinusOneToOne,
                ),
            ))
        })
        .as_ref()
}

/// Read the CAPTCHA text from image bytes, or `None` when the challenge is
/// not an image the model can read or inference fails.
pub fn classify(bytes: &[u8]) -> Option<String> {
    let runtime = engine()?;
    let image = image::load_from_memory(bytes)
        .map_err(|error| warn!("CAPTCHA image did not decode: {error}"))
        .ok()?;
    let result = OcrBuilder::new()
        .build_with(runtime)
        .predict(&image)
        .map_err(|error| warn!("CAPTCHA inference failed: {error}"))
        .ok()?;
    match result {
        OcrResult::Text(text) | OcrResult::Probability { text, .. } => {
            (!text.is_empty()).then_some(text)
        }
        OcrResult::Unsupported { message } => {
            warn!("CAPTCHA model rejected the challenge: {message}");
            None
        }
    }
}
