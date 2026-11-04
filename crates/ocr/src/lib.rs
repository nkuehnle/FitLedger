//! OCR with the ocrs engine. The models (12 MB) aren't bundled with the app: `ensure_models`
//! downloads them once into a folder, checked against known hashes; after that it works offline.

pub mod inbody;

use std::io::Write;
use std::path::Path;

use rten::Model;
use sha2::{Digest, Sha256};

pub use inbody::{read_inbody580, InBodyRead};

const DETECTION: &str = "text-detection.onnx";
const RECOGNITION: &str = "text-recognition.onnx";
/// The pre-trained Ocrs models and their SHA-256 (see models/README.md).
const MODELS: [(&str, &str); 2] = [
    (DETECTION, "a917b23dbd9524b465df7e922641b3ff2981623df4ded5a0234004ef2fee7cfe"),
    (RECOGNITION, "86c145c2edb96c8caed5b1ebb8f44d706408922211451c309c625157dd6061c5"),
];
const SOURCE: &str = "https://ocrs-models.s3-accelerate.amazonaws.com/";

#[derive(Debug)]
pub struct Error(pub String);

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Downloads whichever models are missing from `dir`. A file only lands there (atomically) once
/// its hash matches, so a present file is a good one.
pub fn ensure_models(dir: &Path) -> Result<(), Error> {
    let fail = |e: &dyn std::fmt::Display| Error(format!("couldn't download the sheet reader (needs internet the first time): {e}"));
    std::fs::create_dir_all(dir).map_err(|e| fail(&e))?;
    for (name, sha) in MODELS {
        let path = dir.join(name);
        if path.exists() {
            continue;
        }
        let bytes = ureq::get(&format!("{SOURCE}{name}"))
            .call()
            .map_err(|e| fail(&e))?
            .body_mut()
            .with_config()
            .limit(32 << 20)
            .read_to_vec()
            .map_err(|e| fail(&e))?;
        let got: String = Sha256::digest(&bytes).iter().map(|b| format!("{b:02x}")).collect();
        if got != sha {
            return Err(fail(&format!("{name} has the wrong checksum")));
        }
        let tmp = dir.join(format!("{name}.part"));
        let mut f = std::fs::File::create(&tmp).map_err(|e| fail(&e))?;
        f.write_all(&bytes).and_then(|_| f.sync_all()).map_err(|e| fail(&e))?;
        std::fs::rename(&tmp, &path).map_err(|e| fail(&e))?;
    }
    Ok(())
}

fn models(dir: &Path) -> Result<(Model, Model), Error> {
    let load = |name: &str| Model::load_file(dir.join(name)).map_err(|e| Error(format!("OCR model: {e}")));
    Ok((load(DETECTION)?, load(RECOGNITION)?))
}
