use anyhow::{Context, Result};

use std::collections::{HashMap, HashSet};
use time::{Duration, UtcDateTime};

use crate::{
    Error,
    database::{
        db::Database,
        models::{AircraftEntry, AircraftTableRow},
    },
    feeders::sbs1,
    utils::geo::{CartesianCoord, GeoCoord},
};

const ALT_RESOLUTION: f64 = 1.0; // altitude resoultion in feet
const VRATE_RESOLUTION: f64 = 1.0; // vertical rate resolution in feet
const FEET_PER_METER: f64 = 0.3048;
const UNKNOWN_AIRCRAFT_STR: &str = "Unknown";
const PRE_ALLOCATED_CAP: usize = 20;

const VERSION_URL: &str =
    "https://raw.githubusercontent.com/wiedehopf/tar1090-db/refs/heads/csv/version";

const AIRCRAFT_CSV_GZ_URL: &str =
    "https://raw.githubusercontent.com/wiedehopf/tar1090-db/refs/heads/csv/aircraft.csv.gz";

#[derive(Debug, Clone, Default)]
struct AircraftPositionsTrace(HashSet<GeoCoord>);

impl AircraftPositionsTrace {
    pub fn new() -> Self {
        Self(HashSet::new())
    }

    pub fn insert(&mut self, pos: &GeoCoord) {
        if !pos.is_valid() {
            return;
        }

        self.0.insert(pos.clone());
    }

    pub fn iter(&self) -> impl Iterator<Item = &GeoCoord> {
        self.0.iter()
    }
}

#[derive(Debug, Clone)]
pub struct Aircraft<'p> {
    hexident: u64,
    callsign: String,
    position: GeoCoord, // last position
    observer_position: &'p GeoCoord,
    trace: Option<AircraftPositionsTrace>, // trace of positions
    track: f64,
    ground_speed: f64,
    vertical_rate: f64,
    last_seen: UtcDateTime,
    reg: String,
    short_type: String,
    dist: f64,                  // distance to the last reference location
    closest_dist: f64,          // closest detected distance to the last reference location
    closest_position: GeoCoord, // closest detected position to the last reference location
    closest_at: UtcDateTime,
}

impl std::fmt::Display for Aircraft<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let (hexident, callsign) = self.identification();
        let location = self.get_position().unwrap_or_default();

        write!(
            f,
            "callsign: {:>11} ({:X}) reg: {} type: {}, speed: {:0<.2} km/h, climb rate: {:0<.2} m/s, location: [{}], track: {:0<.2}, last seen: {}",
            callsign,
            hexident,
            self.reg,
            self.short_type,
            self.ground_speed,
            self.vertical_rate,
            location,
            self.track,
            self.last_seen,
        )
    }
}

impl Default for Aircraft<'_> {
    fn default() -> Self {
        Self::new()
    }
}

impl TryInto<AircraftEntry> for &Aircraft<'_> {
    type Error = crate::Error;

    fn try_into(self) -> std::prelude::v1::Result<AircraftEntry, Self::Error> {
        let nearest_location = match self.closest_location() {
            Some(location) => location.clone(),
            None => return Err(crate::Error::InvalidInput),
        };

        let nearest_dist = if self.closest_dist().is_finite() {
            self.closest_dist()
        } else {
            return Err(crate::Error::InvalidInput);
        };

        let (hexident, callsign) = self.identification();

        Ok(AircraftEntry {
            hexident,
            callsign,
            closest_at: self.closest_dist_datetime(),
            closest_dist: nearest_dist,
            closest_location: nearest_location,
            reg: self.reg.clone(),
            short_type: self.short_type.clone(),
        })
    }
}

impl Into<AircraftTableRow> for &Aircraft<'_> {
    fn into(self) -> AircraftTableRow {
        AircraftTableRow {
            hexident: self.hexident,
            callsign: self.callsign.clone(),
            position: self.position.clone(),
            last_seen: self.last_seen,
            dist: self.dist,
            reg: self.reg.clone(),
            short_type: self.short_type.clone(),
        }
    }
}

impl Into<AircraftTableRow> for Aircraft<'_> {
    fn into(self) -> AircraftTableRow {
        AircraftTableRow {
            hexident: self.hexident,
            callsign: self.callsign,
            position: self.position,
            last_seen: self.last_seen,
            dist: self.dist,
            reg: self.reg,
            short_type: self.short_type,
        }
    }
}

impl<'p: 'a, 'a> Aircraft<'a> {
    pub fn new() -> Self {
        Self {
            last_seen: UtcDateTime::UNIX_EPOCH,
            ground_speed: 0.0,
            vertical_rate: 0.0,
            hexident: 0,
            callsign: String::default(),
            position: Default::default(),
            closest_dist: f64::INFINITY,
            closest_position: Default::default(),
            closest_at: UtcDateTime::UNIX_EPOCH,
            dist: f64::INFINITY,
            track: f64::INFINITY,
            reg: Default::default(),
            short_type: Default::default(),
            trace: None,
            observer_position: Default::default(),
        }
    }

    pub fn with_hexident(mut self, hexident: u64) -> Self {
        self.hexident = hexident;
        self
    }

    pub fn with_traces(mut self, enable: bool) -> Self {
        if enable {
            self.trace = Some(AircraftPositionsTrace::new());
        }

        self
    }

    pub fn with_observer_position(mut self, observer: &'p GeoCoord) -> Self {
        self.observer_position = observer;
        self
    }

    /// update current states from SBS1 frame, ignored if hexidents are different
    fn update(&mut self, sbs_frame: &sbs1::Frame) {
        if sbs_frame.hexident != self.hexident {
            return;
        }

        // ensure the aircraft info is always chronological
        if self.last_seen > sbs_frame.datetime_generated.as_utc() {
            return;
        }

        self.last_seen = sbs_frame.datetime_generated.as_utc();

        if let Some(alt) = sbs_frame.altitude {
            self.position.alt = alt * ALT_RESOLUTION * FEET_PER_METER;
        }

        if let Some(lat) = sbs_frame.latitude {
            self.position.lat = lat;
        }
        if let Some(lon) = sbs_frame.longitude {
            self.position.lon = lon;
        }

        if let Some(callsign) = &sbs_frame.callsign {
            self.callsign = str::from_utf8(callsign)
                .unwrap_or_default()
                .trim()
                .to_string();
        }

        if let Some(gnd_speed) = sbs_frame.ground_speed {
            self.ground_speed = gnd_speed * 1.852; // kt -> km/h
        }

        if let Some(vertical_rate) = sbs_frame.vertical_rate {
            self.vertical_rate = vertical_rate * VRATE_RESOLUTION * FEET_PER_METER / 60.0; // ft/min -> m/s
        }

        if let Some(track_angle) = sbs_frame.track {
            self.track = track_angle;
        }

        // current distance to observer
        self.dist = self.distance(self.observer_position);

        // update the closest position to observer in all updates
        if let Some(plane_loc) = self.get_position() {
            let dist = plane_loc - self.observer_position;

            if dist < self.closest_dist {
                self.closest_position = plane_loc.clone();
                self.closest_at = self.last_seen;

                self.closest_dist = dist;
            }
        }

        // insert current position if it's valid
        if let Some(t) = self.trace.as_mut() {
            t.insert(&self.position);
        }
    }

    /// get latitude, longtitude and altitude(in meters) of the aircraft
    pub fn get_position(&self) -> Option<&GeoCoord> {
        if self.position.is_valid() {
            Some(&self.position)
        } else {
            None
        }
    }

    /// get trace of positions, return `None` if positions recording is disabled,
    /// set `with_traces()` with `true` to enable positions recording
    pub fn get_trace(&self) -> Option<impl Iterator<Item = &GeoCoord>> {
        self.trace
            .as_ref()
            .and_then(|trace| Some(trace.iter()))
    }

    pub fn relative_to(&self, reference_location: &GeoCoord) -> Result<CartesianCoord, Error> {
        if let Some(plane_loc) = self.get_position() {
            return Ok(plane_loc.relative_to(reference_location));
        }

        Err(Error::InvalidInput.into())
    }

    /// distance to the reference point
    pub fn distance(&self, reference_location: &GeoCoord) -> f64 {
        if let Some(plane_loc) = self.get_position() {
            plane_loc - reference_location
        } else {
            f64::INFINITY
        }
    }

    /// heading observed by ground in degrees
    pub fn ground_direction(&self) -> f64 {
        self.track
    }

    /// ground speed in km/h
    pub fn ground_speed(&self) -> f64 {
        self.ground_speed
    }

    /// climb rate in m/s
    pub fn vertical_rate(&self) -> f64 {
        self.vertical_rate
    }

    /// datetime of last seen
    pub fn last_seen(&self) -> UtcDateTime {
        self.last_seen
    }

    /// recorded closest distance to the last reference point
    pub fn closest_dist(&self) -> f64 {
        self.closest_dist
    }

    /// recorded closest latitude, longtitude and altitude(in meters) of the aircraft to the last reference point
    pub fn closest_location(&self) -> Option<&GeoCoord> {
        if self.closest_position.is_valid() {
            Some(&self.closest_position)
        } else {
            None
        }
    }

    /// recorded datetime of the closest distance to the last reference point
    pub fn closest_dist_datetime(&self) -> UtcDateTime {
        self.closest_at
    }

    /// aircraft hex identification and callsign in tuple
    /// `(hexident, callsign)`
    pub fn identification(&self) -> (u64, String) {
        let callsign = if self.callsign.is_empty() {
            "NO CALLSIGN".to_string()
        } else {
            self.callsign.to_string()
        };

        (self.hexident, callsign)
    }
}

pub struct Aircrafts<'a> {
    state: HashMap<u64, Aircraft<'a>>, // current state
    home: &'a GeoCoord,
    persistence: Option<Database>,
    persistence_expire_days: Option<u32>,
    should_record_positions: bool,
    max_radius: f64,
    max_altitude: f64,
}

impl Default for Aircrafts<'_> {
    fn default() -> Self {
        Self {
            state: HashMap::with_capacity(PRE_ALLOCATED_CAP),
            home: Default::default(),
            max_radius: -1.0,
            max_altitude: -1.0,
            persistence: None,
            should_record_positions: false,
            persistence_expire_days: None,
        }
    }
}

impl<'a> Aircrafts<'a> {
    pub fn builder() -> AircraftsBuilder<'a> {
        AircraftsBuilder::new()
    }

    /// update/insert an aircraft from SBS1 frame
    pub fn feed(&mut self, frame: &sbs1::Frame) {
        // it gives a mutable reference to aircraft, or insert and return a new aircraft reference if it hasn't seen yet
        let a = self.state.entry(frame.hexident).or_insert(
            Aircraft::new()
                .with_hexident(frame.hexident)
                .with_traces(self.should_record_positions)
                .with_observer_position(&self.home),
        );

        // in-place update the state of the aircraft
        a.update(&frame);

        // update aircraft registration and type only when it is not set to anything
        // it should fetch and update once from the database record
        if a.reg.is_empty() || a.short_type.is_empty() {
            if let Some(db) = self.persistence.as_ref() {
                if let Ok(metadata) = db.get_metadata_by_hexident(a.hexident) {
                    a.reg = metadata.reg;
                    a.short_type = metadata.short_type;
                } else {
                    // avoid triggering database searching again and again because of the
                    // empty row error
                    a.reg = UNKNOWN_AIRCRAFT_STR.into();
                    a.short_type = UNKNOWN_AIRCRAFT_STR.into();
                }
            }
        }
    }

    /// home gps coordinate
    pub fn home(&self) -> Option<&GeoCoord> {
        if self.home.is_valid() {
            Some(&self.home)
        } else {
            None
        }
    }

    /// get aircraft from state cache
    pub fn get(&self, hexident: u64) -> Option<&Aircraft<'a>> {
        self.state.get(&hexident)
    }

    /// dump all seen aircrafts from database
    pub fn dump_all_seen(&self, limit: u64) -> Result<Vec<AircraftEntry>> {
        if let Some(db) = self.persistence.as_ref() {
            Ok(db.get_all_records(limit)?)
        } else {
            Err(Error::InvalidInput.into())
        }
    }

    /// dump all seen aircrafts between `start` and `end` datetime from database,
    /// the datetimes should be UTC
    pub fn dump_seen_by_datetime(
        &self,
        start: &UtcDateTime,
        end: &UtcDateTime,
    ) -> Result<Vec<AircraftEntry>> {
        if let Some(db) = self.persistence.as_ref() {
            Ok(db.get_records_by_datetime(start, end)?)
        } else {
            Err(Error::InvalidInput.into())
        }
    }

    fn fetch_and_import_aircrafts_metadata(&mut self) -> Result<()> {
        if let Some(db) = self.persistence.as_mut() {
            let cur_ver = {
                let mut ver = ureq::get(VERSION_URL)
                    .call()
                    .context("fail to fetch version")?
                    .body_mut()
                    .read_to_string()?;

                if ver.contains('\r') {
                    ver.pop();
                }

                if ver.contains('\n') {
                    ver.pop();
                }

                ver
            };
            let prev_ver = db.get_registry_version()?;
            if cur_ver == prev_ver {
                return Ok(());
            }

            let mut agz_file = ureq::get(AIRCRAFT_CSV_GZ_URL)
                .call()
                .context("fail to fetch aircraft.csv.gz")?
                .into_body();
            db.import_metadata_from_gzipped_csv(agz_file.as_reader(), 10000)?;
            db.insert_registry_version(&cur_ver)?;

            Ok(())
        } else {
            Err(Error::InvalidInput.into())
        }
    }

    /// detection range, return `(max radius, max altitude)`
    pub fn detection_range(&self) -> (f64, f64) {
        (self.max_radius, self.max_altitude)
    }

    /// iterate all recorded aircrafts
    pub fn iter(&self) -> impl Iterator<Item = &Aircraft<'a>> {
        self.state.values()
    }

    /// iterate all recorded aircrafts that have validate location and currently within maximum range
    pub fn iter_within_radius(&self) -> impl Iterator<Item = &Aircraft<'a>> {
        self.iter().filter(|a| {
            // false if the aircraft location is undefined or out of maximum range
            // Notice it's always true if the location is defined and the maximum distance is less than 0
            a.get_position().map_or(false, |a_loc| {
                let reference_point = &self.home;
                let (max_distance, max_altitude) = (self.max_radius, self.max_altitude);

                let is_in_radius =
                    (a_loc - reference_point < max_distance) || (max_distance <= 0.0);
                let is_under_alt = (a_loc.alt < max_altitude) || (max_altitude <= 0.0);

                is_in_radius && is_under_alt
            })
        })
    }

    /// flush valid aircrafts, it should be called periodically
    pub fn flush(&mut self) {
        // clean up database
        if let Some(db) = &self.persistence {
            // save only the aircrafts that were historically in range
            let filtered_aircrafts = self.iter().filter(|a| a.closest_dist() < self.max_radius);
            for a in filtered_aircrafts {
                let a = a.try_into().unwrap();
                db.insert(&a).unwrap_or_else(|err| {
                    eprintln!("fail to insert {}: {}", a.hexident, err);
                });
            }

            // clean up all recorded aircrafts that older than some days
            if let Some(expire_days) = self.persistence_expire_days {
                let older = UtcDateTime::now().replace_time(time::Time::MIDNIGHT)
                    - time::Duration::days(expire_days as i64);
                db.delete_records_older_than(&older).unwrap_or_else(|err| {
                    eprintln!("fail to delete records: {}", err);
                });
            }
        }

        // clean up expired aircrafts in the state cache
        self.state
            .retain(|_, a| UtcDateTime::now() - a.last_seen <= Duration::minutes(1));
        self.state.shrink_to(PRE_ALLOCATED_CAP);
    }
}

impl Drop for Aircrafts<'_> {
    fn drop(&mut self) {
        self.flush();
    }
}

pub struct AircraftsBuilder<'b> {
    aircrafts: Aircrafts<'b>,
}

impl Default for AircraftsBuilder<'_> {
    fn default() -> Self {
        Self {
            aircrafts: Default::default(),
        }
    }
}

impl<'p: 'b, 'b> AircraftsBuilder<'b> {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn home(mut self, home: &'p GeoCoord) -> Self {
        self.aircrafts.home = home;

        self
    }

    pub fn persistence(mut self, db_path: &str) -> Self {
        self.aircrafts.persistence = Database::open(db_path)
            .or_else(|err| {
                eprintln!("fail to open database, skip persistence: {err}");
                Err(())
            })
            .ok();

        self
    }

    pub fn persistence_expire_days(mut self, days: u32) -> Self {
        self.aircrafts.persistence_expire_days = if days == 0 { None } else { Some(days) };

        self
    }

    pub fn radius(mut self, radius: f64) -> Self {
        self.aircrafts.max_radius = radius;

        self
    }

    pub fn altitude(mut self, alt: f64) -> Self {
        self.aircrafts.max_altitude = alt;

        self
    }

    pub fn record_positions(mut self, enable: bool) -> Self {
        self.aircrafts.should_record_positions = enable;

        self
    }

    pub fn build(mut self) -> Result<Aircrafts<'b>> {
        if self.aircrafts.persistence.is_some() {
            let start = std::time::Instant::now();
            self.aircrafts
                .fetch_and_import_aircrafts_metadata()
                .context("fail to import metadata")?;
            println!("imported: took {} seconds", start.elapsed().as_secs_f64());
        }

        Ok(self.aircrafts)
    }
}

#[cfg(test)]
mod test {
    use cli_table::{WithTitle, print_stdout};

    use crate::aircraft::*;
    use crate::database::models::AircraftMetadataEntry;
    use crate::feeders::sbs1::Frame;

    const TEST_SBS1_FRAMES: &[&str] = &[
        "MSG,4,5,211,4CA2D6,10057,2008/11/28,14:53:49.986,2008/11/28,14:58:51.153,,,408.3,146.4,,,64,,,,,\r\n",
        "MSG,8,5,211,4CA2D6,10057,2008/11/28,14:53:50.391,2008/11/28,14:58:51.153,,,,,,,,,,,,0\r\n",
        "MSG,4,5,211,4CA2D6,10057,2008/11/28,14:53:50.391,2008/11/28,14:58:51.153,,,408.3,146.4,,,64,,,,,\r\n",
        "MSG,3,5,211,4CA2D6,10057,2008/11/28,14:53:50.594,2008/11/28,14:58:51.153,,37000,,,51.45735,-1.02826,,,0,0,0,0\r\n",
        "MSG,1,5,211,4CA2D6,11267,2008/11/28,23:48:18.611,2008/11/28,23:53:19.161,TEST123 ,,,,,,,,,,,\r\n",
        "MSG,8,5,812,ABBEE3,10095,2008/11/28,14:53:50.594,2008/11/28,14:58:51.153,,,,,,,,,,,,0\r\n",
        "MSG,3,5,276,4010E9,10088,2008/11/28,14:53:49.986,2008/11/28,14:58:51.153,,28000,,,53.02551,-2.91389,,,0,0,0,0\r\n",
        "MSG,4,5,276,4010E9,10088,2008/11/28,14:53:50.188,2008/11/28,14:58:51.153,,,459.4,20.2,,,64,,,,,\r\n",
        "MSG,8,5,276,4010E9,10088,2008/11/28,14:53:50.594,2008/11/28,14:58:51.153,,,,,,,,,,,,0\r\n",
        "MSG,3,5,276,4010E9,10088,2008/11/28,14:53:50.594,2008/11/28,14:58:51.153,,28000,,,53.02677,-2.91310,,,0,0,0,0\r\n",
        "MSG,4,5,769,4CA2CB,10061,2008/11/28,14:53:50.188,2008/11/28,14:58:51.153,,,367.7,138.6,,,-2432,,,,,\r\n",
        "MSG,8,5,769,4CA2CB,10061,2008/11/28,14:53:50.391,2008/11/28,14:58:51.153,,,,,,,,,,,,0\r\n",
    ];

    #[test]
    fn test_aircraft() {
        let mut air1 = Aircraft::new().with_hexident(0x4CA2D6);
        let mut air2 = Aircraft::new().with_hexident(0x4010E9);

        let frames: Vec<Frame> = TEST_SBS1_FRAMES
            .iter()
            .map(|&x| Frame::parse(&x).unwrap())
            .collect();

        for frame in &frames {
            air1.update(&frame);
            air2.update(&frame);
        }

        assert!(air1.get_trace().is_none());
        assert!(air2.get_trace().is_none());

        let air1_loc = air1.get_position();
        let air2_loc = air2.get_position();

        assert!(air1_loc.is_some());
        assert!(air2_loc.is_some());

        assert_eq!(air1_loc.unwrap().alt, 11277.6);
        assert_eq!(air1_loc.unwrap().lat, 51.45735);
        assert_eq!(air1_loc.unwrap().lon, -1.02826);
        assert_eq!(air1.callsign, "TEST123");

        assert_eq!(air2_loc.unwrap().alt, 8534.4);
        assert_eq!(air2_loc.unwrap().lat, 53.02677);
        assert_eq!(air2_loc.unwrap().lon, -2.91310);
        assert_eq!(air2.callsign, "");
    }

    #[test]
    fn test_aircrafts() {
        let frames: Vec<Frame> = TEST_SBS1_FRAMES
            .iter()
            .map(|&x| Frame::parse(&x).unwrap())
            .collect();

        let mut aircrafts = Aircrafts::builder().record_positions(true).build().unwrap();

        for frame in &frames {
            aircrafts.feed(frame);
        }

        // getting aircrafts that have seen
        let air1 = aircrafts.get(0x4CA2D6).unwrap();
        let air2 = aircrafts.get(0x4010E9).unwrap();
        let air3 = aircrafts.get(0x4CA2CB).unwrap();

        assert_eq!(air1.get_trace().unwrap().count(), 1);
        assert_eq!(air2.get_trace().unwrap().count(), 2);
        assert_eq!(air3.get_trace().unwrap().count(), 0);

        let air1_loc = air1.get_position();
        let air2_loc = air2.get_position();
        let air3_loc = air3.get_position();

        assert!(air1_loc.is_some());
        assert!(air2_loc.is_some());
        assert!(air3_loc.is_none());

        assert_eq!(air1_loc.unwrap().alt, 11277.6);
        assert_eq!(air1_loc.unwrap().lat, 51.45735);
        assert_eq!(air1_loc.unwrap().lon, -1.02826);
        assert_eq!(air1.callsign, "TEST123");

        assert_eq!(air2_loc.unwrap().alt, 8534.4);
        assert_eq!(air2_loc.unwrap().lat, 53.02677);
        assert_eq!(air2_loc.unwrap().lon, -2.91310);
        assert_eq!(air2.callsign, "");

        // getting aircraft that haven't seen
        let air_not_seen_yet = aircrafts.get(0xaabb);
        assert!(air_not_seen_yet.is_none());

        aircrafts.flush();
        assert_eq!(aircrafts.iter().count(), 0);
    }

    #[test]
    fn test_aircrafts_iter() {
        let frames: Vec<Frame> = TEST_SBS1_FRAMES
            .iter()
            .map(|&x| Frame::parse(&x).unwrap())
            .collect();

        let mut aircrafts = Aircrafts::builder().build().unwrap();

        for frame in &frames {
            aircrafts.feed(frame);
        }

        // iter() method
        assert_eq!(aircrafts.iter().count(), 4);
        assert!(aircrafts.iter().any(|a| { a.get_position().is_some() }));
    }

    #[test]
    fn test_aircraft_dist_filter_abormal() {
        let home: GeoCoord = "0.0, 0.0".parse().unwrap();
        let frames: Vec<Frame> = TEST_SBS1_FRAMES
            .iter()
            .map(|&x| Frame::parse(&x).unwrap())
            .collect();
        let mut aircrafts = AircraftsBuilder::new()
            .home(&home)
            .radius(-1.0)
            .build()
            .unwrap();

        for frame in &frames {
            aircrafts.feed(frame);
        }

        let plane_count = aircrafts.iter().count();
        assert_eq!(plane_count, 4);

        let plane_count = aircrafts.iter_within_radius().count();
        assert!(plane_count != 0);
    }

    #[test]
    fn test_aircraft_dist_filter() {
        let home: GeoCoord = "51.455, -1.0281".parse().unwrap();
        let frames: Vec<Frame> = TEST_SBS1_FRAMES
            .iter()
            .map(|&x| Frame::parse(&x).unwrap())
            .collect();
        let mut aircrafts = AircraftsBuilder::new()
            .home(&home)
            .radius(10_000.0)
            .build()
            .unwrap();

        for frame in &frames {
            aircrafts.feed(frame);
        }

        let plane_count = aircrafts.iter().count();
        assert_eq!(plane_count, 4);

        let plane_count = aircrafts.iter_within_radius().count();
        assert!(plane_count != 0);
    }

    #[test]
    fn test_aircrafts_presist() {
        let home: GeoCoord = "51.455, -1.0281".parse().unwrap();
        let frames: Vec<Frame> = TEST_SBS1_FRAMES
            .iter()
            .map(|&x| Frame::parse(&x).unwrap())
            .collect();
        let mut aircrafts = AircraftsBuilder::new()
            .home(&home)
            .radius(10_000.0)
            .persistence(":memory:")
            .build()
            .unwrap();

        for frame in &frames {
            aircrafts.feed(frame);
        }

        // test table
        let planes: Vec<AircraftTableRow> = aircrafts.iter().map(|a| a.into()).collect();
        let tab = planes.with_title();
        assert!(print_stdout(tab).is_ok());

        aircrafts.flush();

        // database test
        let db = aircrafts.persistence.as_mut().unwrap();

        let air1 = db.get_records_by_hexident(0x4CA2D6).unwrap();
        assert!(!air1.is_empty() && air1.len() == 1);
        assert!(air1[0].callsign == "TEST123");
        assert!(air1[0].closest_at != UtcDateTime::UNIX_EPOCH);
        assert!(air1[0].closest_dist.is_finite());

        assert!(db.get_records_by_hexident(0x4010E9).is_err());
        assert!(db.get_records_by_hexident(0x4CA2CB).is_err());

        assert_eq!(db.get_all_records(4).unwrap().len(), 1);

        // 0x4CA2D6
        let AircraftMetadataEntry {
            reg, short_type, ..
        } = db.get_metadata_by_hexident(0x4CA2D6).unwrap_or_default();
        assert_eq!(reg, "EI-DLJ");
        assert_eq!(short_type, "B738");

        // something else
        let AircraftMetadataEntry {
            reg,
            short_type,
            descr,
            ..
        } = db.get_metadata_by_hexident(0x004013).unwrap_or_default();
        assert_eq!(reg, "Z-FJF");
        assert_eq!(short_type, "E145");
        assert!(!descr.is_empty());

        // time search
        let test_datetime = UtcDateTime::from_unix_timestamp(1213551070).unwrap();
        let test_datetime_end = UtcDateTime::from_unix_timestamp(1229362270).unwrap();
        let res = db
            .get_records_by_datetime(&test_datetime, &test_datetime_end)
            .unwrap();
        assert_eq!(res.len(), 1);
        assert!(!res[0].reg.is_empty());
        assert!(!res[0].short_type.is_empty());

        // delete by datetime
        // this one shouldn't be deleted
        db.delete_records_older_than(&test_datetime).unwrap();
        let res = db.get_records_by_datetime(&test_datetime, &test_datetime_end);
        assert!(res.is_ok() || res.unwrap_or_default().len() != 0);

        // delete by datetime
        // this one should be deleted
        db.delete_records_older_than(&test_datetime_end).unwrap();

        let res = db.get_records_by_datetime(&test_datetime, &test_datetime_end);
        assert!(res.is_err() || res.unwrap_or_default().len() == 0);

        assert_eq!(aircrafts.iter().count(), 0);
    }
}
