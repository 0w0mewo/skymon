use time::UtcDateTime;

use crate::utils::geo::GeoCoord;
use cli_table::format::Justify;

#[derive(Debug, Clone)]
pub struct AircraftEntry {
    pub hexident: u64,
    pub callsign: String,
    pub reg: String,
    pub short_type: String,
    pub closest_location: GeoCoord,
    pub closest_dist: f64,
    pub closest_at: UtcDateTime,
}

impl AircraftEntry {
    pub(crate) fn is_valid(&self) -> bool {
        self.hexident != 0
            && (!self.callsign.is_empty())
            && self.closest_at != UtcDateTime::UNIX_EPOCH
            && self.closest_dist.is_finite()
            && self.closest_location.is_valid()
    }
}

impl Default for AircraftEntry {
    fn default() -> Self {
        Self {
            hexident: Default::default(),
            callsign: Default::default(),
            closest_at: UtcDateTime::UNIX_EPOCH,
            closest_dist: f64::INFINITY,
            closest_location: Default::default(),
            reg: Default::default(),
            short_type: Default::default(),
        }
    }
}

impl std::fmt::Display for AircraftEntry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} ({:x}) [{}] dist: {:.3}, recorded at: {}",
            self.callsign,
            self.hexident,
            self.closest_location,
            self.closest_dist,
            self.closest_at.truncate_to_second()
        )
    }
}

impl Into<AircraftTableRow> for &AircraftEntry {
    fn into(self) -> AircraftTableRow {
        AircraftTableRow {
            hexident: self.hexident,
            callsign: self.callsign.clone(),
            position: self.closest_location.clone(),
            last_seen: self.closest_at,
            dist: self.closest_dist,
            reg: self.reg.clone(),
            short_type: self.short_type.clone(),
        }
    }
}

impl Into<AircraftTableRow> for AircraftEntry {
    fn into(self) -> AircraftTableRow {
        AircraftTableRow {
            hexident: self.hexident,
            callsign: self.callsign,
            position: self.closest_location,
            last_seen: self.closest_at,
            dist: self.closest_dist,
            reg: self.reg,
            short_type: self.short_type,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct AircraftMetadataEntry {
    pub hexident: u64,
    pub reg: String,
    pub short_type: String,
    pub descr: String,
    pub year: u16,
    pub owner: String,
}

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
