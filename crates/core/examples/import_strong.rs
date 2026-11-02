//! Imports Strong CSV exports into a Liftbook database file, exactly as the app's Import screen does
//! (import, then auto-map exercise names). Useful for loading test data.
//! Usage: cargo run -p liftbook-core --example import_strong -- <db path> <csv>...
fn main() {
    let mut args = std::env::args().skip(1);
    let db_path = args.next().expect("db path");
    let mut db = liftbook_core::db::Db::open(&db_path).unwrap();
    for path in args {
        let text = std::fs::read_to_string(&path).unwrap();
        let header = text.lines().next().unwrap_or("").to_lowercase();
        if header.contains("exercise") {
            let imp = liftbook_core::strong::parse_workouts(&text, Default::default()).unwrap();
            let out = db.import_strong_workouts(&imp, &path).unwrap();
            println!("{path}: {out:?}, auto-mapped {}", db.auto_map_names().unwrap());
        } else {
            let imp = liftbook_core::strong::parse_bodyweight(&text, Default::default()).unwrap();
            println!("{path}: {:?}", db.import_bodyweight(&imp, &path, "strong").unwrap());
        }
    }
    if db.setting("weight_unit").unwrap().is_none() {
        db.set_setting("weight_unit", "lb").unwrap();
    }
}
