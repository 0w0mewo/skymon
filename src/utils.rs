use std::fmt::Write;
use std::io::{self, Read};

use crate::geo::GeoCoord;
use anyhow::Result;
use cli_table::format::Justify;
use sha2::Digest;
use time::UtcDateTime;

fn table_f64_display(v: &f64) -> impl std::fmt::Display {
    format!("{:0<.2}", v)
}

fn table_u64_hex_display(v: &u64) -> impl std::fmt::Display {
    format!("{:X}", v)
}

fn table_datetime_display(v: &UtcDateTime) -> impl std::fmt::Display {
    v.truncate_to_second()
}

#[derive(Debug, Clone, cli_table::Table)]
pub struct AircraftTableRow {
    #[table(title = "Hexident", display_fn = "table_u64_hex_display")]
    pub hexident: u64,
    #[table(title = "Callsign")]
    pub callsign: String,
    #[table(title = "Registration", justify = "Justify::Center")]
    pub reg: String,
    #[table(title = "Type")]
    pub short_type: String,
    #[table(title = "Last position")]
    pub position: GeoCoord,
    #[table(
        title = "Last distance to home (m)",
        display_fn = "table_f64_display",
        justify = "Justify::Center"
    )]
    pub dist: f64, // distance to the last reference location
    #[table(title = "Last update", display_fn = "table_datetime_display")]
    pub last_seen: UtcDateTime,
}

impl Default for AircraftTableRow {
    fn default() -> Self {
        Self {
            last_seen: UtcDateTime::UNIX_EPOCH,
            hexident: u64::MAX,
            callsign: Default::default(),
            reg: Default::default(),
            short_type: Default::default(),
            position: Default::default(),
            dist: f64::INFINITY,
        }
    }
}

impl PartialOrd for AircraftTableRow {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for AircraftTableRow {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.dist.total_cmp(&other.dist)
    }
}

impl PartialEq for AircraftTableRow {
    fn eq(&self, other: &Self) -> bool {
        self.hexident == other.hexident
    }
}

impl Eq for AircraftTableRow {}

pub fn sleep_ms(ms: u64) {
    std::thread::sleep(std::time::Duration::from_millis(ms));
}

pub fn sha256_digest(input: impl io::Read) -> Result<String> {
    let mut reader = io::BufReader::new(input);
    let mut hasher = sha2::Sha256::new();
    let mut buffer = [0u8; 4096];

    loop {
        let bytes_read = reader.read(&mut buffer)?;
        if bytes_read == 0 {
            break;
        } else {
            hasher.update(&buffer[..bytes_read]);
        }
    }

    let hash = hasher
        .finalize()
        .iter()
        .try_fold::<String, _, Result<String>>(String::new(), |mut out, b| {
            write!(out, "{b:02x}")?;
            Ok(out)
        })?;

    Ok(hash)
}

#[cfg(test)]
mod test {
    use std::fs;

    use crate::utils::*;

    #[test]
    fn test_sha256_digest() {
        let agz_file = fs::File::open("assets/aircraft.csv.gz").unwrap();
        assert!(sha256_digest(agz_file).is_ok())
    }
}
