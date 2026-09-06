//! Regenerate the small UI table: cargo run -p jinn-todo --example status-table.
fn main() {
    let mut table = serde_json::Map::new();
    for status in jinn_todo::Status::ALL {
        table.insert(
            status.tag().into(),
            serde_json::to_value(status.allows()).unwrap(),
        );
    }
    println!("{}", serde_json::to_string_pretty(&table).unwrap());
}
