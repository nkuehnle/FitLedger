//! `cargo run --release -p fitledger-ocr --example dump -- sheet.png ...` prints what the reader sees.
fn main() {
    for path in std::env::args().skip(1) {
        let r = fitledger_ocr::read_inbody580(&std::fs::read(&path).unwrap(), std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/models"))).unwrap();
        println!("{path}\t{}\t{}\t{}\t{:?}", r.measured_at.unwrap_or_default(), serde_json_like(&r.values), r.doubtful.join(","), r.mass_unit);
    }
}
fn serde_json_like(v: &std::collections::BTreeMap<String, f64>) -> String {
    v.iter().map(|(k, x)| format!("{k}={x}")).collect::<Vec<_>>().join(";")
}
