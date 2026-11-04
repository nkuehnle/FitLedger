//! Prints how the target parser reads each distinct exercise note in a Strong export.
//! Usage: cargo run -p fitledger-core --example notes_report -- path/to/strong_workouts.csv
use std::collections::BTreeSet;

fn main() {
    let path = std::env::args().nth(1).expect("path to a Strong workouts CSV");
    let text = std::fs::read_to_string(path).unwrap();
    let imp = fitledger_core::strong::parse_workouts(&text, Default::default()).unwrap();
    let notes: BTreeSet<_> = imp.workouts.iter().flat_map(|w| &w.exercises).filter_map(|e| e.notes.clone()).collect();
    for n in notes {
        let tag = match fitledger_core::targets::parse_target(&n) {
            None => "  -  ".to_string(),
            Some(t) => format!(
                "{} sets={:?} reps={:?} rest={:?}",
                if t.certain { "OK " } else { "?? " },
                t.sets.map(|r| (r.min, r.max)),
                t.reps.map(|r| (r.min, r.max)),
                t.remainder
            ),
        };
        println!("{n:?}\n    {tag}");
    }
}
