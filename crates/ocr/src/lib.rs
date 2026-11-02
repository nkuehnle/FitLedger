//! Offline OCR with the ocrs engine. The models are embedded, so it works without a network.

pub mod inbody;

use rten::Model;

pub use inbody::{read_inbody580, InBodyRead};

static DETECTION: &[u8] = include_bytes!("../models/text-detection.onnx");
static RECOGNITION: &[u8] = include_bytes!("../models/text-recognition.onnx");

#[derive(Debug)]
pub struct Error(pub String);

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

fn models() -> Result<(Model, Model), Error> {
    let load = |b: &'static [u8]| Model::load_static_slice(b).map_err(|e| Error(format!("OCR model: {e}")));
    Ok((load(DETECTION)?, load(RECOGNITION)?))
}
