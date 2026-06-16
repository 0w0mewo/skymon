use crate::geo::GeoCoord;
use cli_table::format::Justify;
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