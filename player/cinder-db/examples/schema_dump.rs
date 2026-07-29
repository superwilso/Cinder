//! Dump the schema (and a few sample rows) of a real MTPDB.dat pulled off the device.
//! Offline RE aid: `cargo run -p cinder-db --example schema_dump -- <path-to-MTPDB.dat>`

use cinder_db::Db;

fn main() {
    let path = std::env::args().nth(1).expect("usage: schema_dump <MTPDB.dat>");
    let db = Db::open(&path).expect("open");
    let conn = db.conn();

    println!("=== TABLES ===");
    let mut st = conn
        .prepare("SELECT name, sql FROM sqlite_master WHERE type='table' ORDER BY name")
        .unwrap();
    let rows = st
        .query_map([], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?))
        })
        .unwrap();
    let mut names = Vec::new();
    for row in rows {
        let (name, sql) = row.unwrap();
        let count: i64 = conn
            .query_row(&format!("SELECT COUNT(*) FROM \"{name}\""), [], |r| r.get(0))
            .unwrap_or(-1);
        println!("\n-- {name}  ({count} rows)");
        if let Some(s) = sql {
            println!("{s}");
        }
        names.push(name);
    }

    // Anything that smells like playlist membership gets a sample dump.
    println!("\n=== SAMPLES (playlist-ish tables) ===");
    for name in names.iter().filter(|n| {
        let l = n.to_lowercase();
        l.contains("play") || l.contains("list") || l.contains("tag") || l.contains("group")
    }) {
        println!("\n-- {name} (first 10)");
        dump_rows(conn, name, 10);
    }
}

fn dump_rows(conn: &rusqlite::Connection, table: &str, limit: usize) {
    let Ok(mut st) = conn.prepare(&format!("SELECT * FROM \"{table}\" LIMIT {limit}")) else {
        println!("   (unreadable)");
        return;
    };
    let cols: Vec<String> = st.column_names().iter().map(|c| c.to_string()).collect();
    println!("   cols: {}", cols.join(", "));
    let mut rows = st.query([]).unwrap();
    while let Some(r) = rows.next().unwrap() {
        let cells: Vec<String> = (0..cols.len())
            .map(|i| match r.get_ref(i) {
                Ok(rusqlite::types::ValueRef::Null) => "NULL".into(),
                Ok(rusqlite::types::ValueRef::Integer(v)) => v.to_string(),
                Ok(rusqlite::types::ValueRef::Real(v)) => v.to_string(),
                Ok(rusqlite::types::ValueRef::Text(v)) => {
                    let s = String::from_utf8_lossy(v);
                    if s.len() > 60 { format!("{}…", &s[..60.min(s.len())]) } else { s.into_owned() }
                }
                Ok(rusqlite::types::ValueRef::Blob(b)) => format!("<blob {} B>", b.len()),
                Err(_) => "?".into(),
            })
            .collect();
        println!("   {}", cells.join(" | "));
    }
}
