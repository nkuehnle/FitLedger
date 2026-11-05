//! Builds data/norms/crossfit.json from the CrossFit athletes dataset (Ulrik Thyge Pedersen, Kaggle,
//! CC BY 4.0): https://www.kaggle.com/datasets/ulrikthygepedersen/crossfit-athletes
//! Only the percentile tables are kept, not the athletes' rows.
//! Usage: cargo run -p fitledger-core --example build_crossfit -- <athletes.csv> <out.json>
use sha2::{Digest, Sha256};
use fitledger_core::norms;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let [_, csv, out] = &args[..] else { panic!("usage: build_crossfit <athletes.csv> <out.json>") };
    let bytes = std::fs::read(csv).expect("read the CSV");
    let got: String = Sha256::digest(&bytes).iter().map(|b| format!("{b:02x}")).collect();
    assert_eq!(got, norms::CROSSFIT_CSV_SHA256, "not the expected athletes.csv");
    let tables = norms::crossfit_tables(&bytes[..]).expect("build the tables");
    std::fs::write(out, serde_json::to_string(&tables).unwrap() + "\n").expect("write");
}
