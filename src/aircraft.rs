use anyhow::{Result, anyhow};

use std::collections::HashMap;
use time::{Duration, UtcDateTime};

use crate::{
    db::{AircraftEntry, Database},
    geo::{CartesianCoord, GeoCoord},
    sbs1, utils::AircraftTableRow,
};

const ALT_RESOLUTION: f64 = 1.0; // altitude resoultion in feet
const VRATE_RESOLUTION: f64 = 1.0; // vertical rate resolution in feet
const FEET_PER_METER: f64 = 0.3048;

#[derive(Debug, Clone)]
pub struct Aircraft {
    hexident: u64,
    callsign: String,
    position: GeoCoord,
    track: f64,
    ground_speed: f64,
    vertical_rate: f64,
    last_seen: UtcDateTime,
    dist: f64,                  // distance to the last reference location
    closest_dist: f64,          // closest detected distance to the last reference location
    closest_position: GeoCoord, // closest detected position to the last reference location
    closest_at: UtcDateTime,
}

impl PartialEq for Aircraft {
    fn eq(&self, other: &Self) -> bool {
        self.hexident == other.hexident
    }
}

impl PartialOrd for Aircraft {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Aircraft {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.last_seen.cmp(&other.last_seen).reverse() // latest first
    }
}

impl Eq for Aircraft {}

impl std::fmt::Display for Aircraft {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let (hexident, callsign) = self.identification();
        let location = self.get_position().unwrap_or_default();

        write!(
            f,
            "callsign: {:>11} ({:X}), speed: {:0<.2} km/h, climb rate: {:0<.2} m/s, location: [{}], track: {:0<.2}, last seen: {}",
            callsign,
            hexident,
            self.ground_speed,
            self.vertical_rate,
            location,
            self.track,
            self.last_seen,
        )
    }
}

impl Default for Aircraft {
    fn default() -> Self {
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
        }
    }
}

impl TryFrom<&Aircraft> for AircraftEntry {
    type Error = crate::Error;

    fn try_from(aircraft: &Aircraft) -> std::prelude::v1::Result<Self, Self::Error> {
        let nearest_location = match aircraft.closest_location() {
            Some(location) => location.clone(),
            None => return Err(crate::Error::InvalidInput),
        };

        let nearest_dist = if aircraft.closest_dist().is_finite() {
            aircraft.closest_dist()
        } else {
            return Err(crate::Error::InvalidInput);
        };

        let (hexident, callsign) = aircraft.identification();

        Ok(Self {
            hexident,
            callsign,
            closest_at: aircraft.closest_dist_datetime(),
            closest_dist: nearest_dist,
            closest_location: nearest_location,
        })
    }
}

impl From<&Aircraft> for AircraftTableRow {
    fn from(value: &Aircraft) -> Self {
        Self {
            hexident: value.hexident,
            callsign: value.callsign.clone(),
            position: value.position.clone(),
            last_seen: value.last_seen,
            dist: value.dist,
        }
    }
}

impl From<Aircraft> for AircraftTableRow {
    fn from(value: Aircraft) -> Self {
        Self {
            hexident: value.hexident,
            callsign: value.callsign,
            position: value.position,
            last_seen: value.last_seen,
            dist: value.dist,
        }
    }
}

impl Aircraft {
    pub fn new() -> Self {
        Default::default()
    }

    pub fn with_hexident(mut self, hexident: u64) -> Self {
        self.hexident = hexident;
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
    }

    /// get latitude, longtitude and altitude(in meters) of the aircraft
    pub fn get_position(&self) -> Option<&GeoCoord> {
        if self.position.is_valid() {
            Some(&self.position)
        } else {
            None
        }
    }

    pub fn relative_to(&self, reference_location: &GeoCoord) -> Result<CartesianCoord> {
        if let Some(plane_loc) = self.get_position() {
            return Ok(plane_loc.relative_to(reference_location));
        }

        Err(anyhow!("invalid location"))
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

    /// set distance to the reference point
    fn update_distance(&mut self, reference_location: &GeoCoord) {
        self.dist = self.distance(reference_location);
    }

    /// update closest distance and geolocation of the aircraft to `reference_location`
    fn update_closest(&mut self, reference_location: &GeoCoord) {
        if let Some(plane_loc) = self.get_position() {
            let dist = plane_loc - reference_location;

            if dist < self.closest_dist {
                self.closest_position = plane_loc.clone();
                self.closest_at = self.last_seen;

                self.closest_dist = dist;
            }
        }
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

pub struct Aircrafts {
    state: HashMap<u64, Aircraft>, // current state
    home: GeoCoord,
    persistence: Option<Database>,
    max_radius: f64,
    max_altitude: f64,
}

impl Default for Aircrafts {
    fn default() -> Self {
        Self {
            state: HashMap::new(),
            home: Default::default(),
            max_radius: -1.0,
            max_altitude: -1.0,
            persistence: None,
        }
    }
}

impl Aircrafts {
    pub fn builder() -> AircraftsBuilder {
        AircraftsBuilder::new()
    }

    /// update/insert an aircraft from SBS1 frame
    pub fn feed(&mut self, frame: &sbs1::Frame) {
        let update_aircraft = |a: &mut Aircraft| {
            a.update(&frame);
            a.update_closest(&self.home);
            a.update_distance(&self.home);
        };
        
        if let Some(aircraft) = self.state.get_mut(&frame.hexident) {
            update_aircraft(aircraft);
        } else {
            let mut aircraft = Aircraft::new().with_hexident(frame.hexident);
            update_aircraft(&mut aircraft);

            self.state.insert(frame.hexident, aircraft);
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
    pub fn get(&self, hexident: u64) -> Option<&Aircraft> {
        self.state.get(&hexident)
    }

    /// dump all seen aircrafts from database
    pub fn dump_all_seen(&self, limit: u64) -> Result<Vec<AircraftEntry>> {
        if let Some(db) = self.persistence.as_ref() {
            db.get_all_records(limit)
        } else {
            Err(anyhow!("database is not set"))
        }
    }

    /// dump all seen aircrafts between `start` and `end` datetime from database,
    /// the datetimes should be UTC
    pub fn dump_seen_by_datetime(
        &self, start: &UtcDateTime, end: &UtcDateTime
    ) -> Result<Vec<AircraftEntry>> {
        if let Some(db) = self.persistence.as_ref() {
            db.get_records_by_datetime(start, end)
        } else {
            Err(anyhow!("database is not set"))
        }
    }

    /// detection range, return `(max radius, max altitude)`
    pub fn detection_range(&self) -> (f64, f64) {
        (self.max_radius, self.max_altitude)
    }

    /// iterate all recorded aircrafts
    pub fn iter(&self) -> impl Iterator<Item = &Aircraft> {
        self.state.values()
    }

    /// iterate all recorded aircrafts that have validate location and currently within maximum range
    pub fn iter_within_radius(&self) -> impl Iterator<Item = &Aircraft> {
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
        // save only the aircrafts that were historically in range
        if let Some(db) = &self.persistence {
            let filtered_aircrafts = self.iter().filter(|a| a.closest_dist() < self.max_radius);
            for a in filtered_aircrafts {
                let a = a.try_into().unwrap();
                _ = db.insert(&a).unwrap_or_else(|err| {
                    eprintln!("fail to insert {}: {}", a.hexident, err);
                });
            }
        }

        // clean up expired aircrafts
        self.state
            .retain(|_, a| UtcDateTime::now() - a.last_seen <= Duration::minutes(1));
    }
}

impl Drop for Aircrafts {
    fn drop(&mut self) {
        self.flush();
    }
}

pub struct AircraftsBuilder(Aircrafts);

impl AircraftsBuilder {
    pub fn new() -> Self {
        Self(Aircrafts::default())
    }

    pub fn home(mut self, home: &GeoCoord) -> Self {
        self.0.home = home.clone();

        self
    }

    pub fn persistence(mut self, db_path: &str) -> Self {
        let db = Database::open(db_path).expect("fail to open DB");
        self.0.persistence = Some(db);

        self
    }

    pub fn radius(mut self, radius: f64) -> Self {
        self.0.max_radius = radius;

        self
    }

    pub fn altitude(mut self, alt: f64) -> Self {
        self.0.max_altitude = alt;

        self
    }

    pub fn build(self) -> Aircrafts {
        self.0
    }
}

#[cfg(test)]
mod test {
    use cli_table::{WithTitle, print_stdout};

    use crate::aircraft::*;
    use crate::sbs1::Frame;

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

        let mut aircrafts = Aircrafts::builder().build();

        for frame in &frames {
            aircrafts.feed(frame);
        }

        // getting aircrafts that have seen
        let air1 = aircrafts.get(0x4CA2D6).unwrap();
        let air2 = aircrafts.get(0x4010E9).unwrap();
        let air3 = aircrafts.get(0x4CA2CB).unwrap();

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
        assert_eq!(air_not_seen_yet, None);

        aircrafts.flush();
        assert_eq!(aircrafts.iter().count(), 0);
    }

    #[test]
    fn test_aircrafts_iter() {
        let frames: Vec<Frame> = TEST_SBS1_FRAMES
            .iter()
            .map(|&x| Frame::parse(&x).unwrap())
            .collect();

        let mut aircrafts = Aircrafts::builder().build();

        for frame in &frames {
            aircrafts.feed(frame);
        }

        // iter() method
        assert_eq!(aircrafts.iter().count(), 4);
        assert!(aircrafts.iter().any(|a| { a.get_position().is_some() }));

        //sort
        let mut airs: Vec<&Aircraft> = aircrafts.iter().collect();
        airs.sort();
        assert!(airs.is_sorted());
    }

    #[test]
    fn test_aircraft_dist_filter_abormal() {
        let home: GeoCoord = "0.0, 0.0".parse().unwrap();
        let frames: Vec<Frame> = TEST_SBS1_FRAMES
            .iter()
            .map(|&x| Frame::parse(&x).unwrap())
            .collect();
        let mut aircrafts = AircraftsBuilder::new().home(&home).radius(-1.0).build();

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
        let mut aircrafts = AircraftsBuilder::new().home(&home).radius(10_000.0).build();

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
            .build();

        for frame in &frames {
            aircrafts.feed(frame);
        }

        aircrafts.flush();

        let db = aircrafts.persistence.as_ref().unwrap();

        let air1 = db.get_records_by_hexident(0x4CA2D6).unwrap_or_default();
        assert!(!air1.is_empty() && air1.len() == 1);
        assert!(air1[0].callsign == "TEST123");
        assert!(air1[0].closest_at != UtcDateTime::UNIX_EPOCH);
        assert!(air1[0].closest_dist.is_finite());

        assert!(db.get_records_by_hexident(0x4010E9).is_err());
        assert!(db.get_records_by_hexident(0x4CA2CB).is_err());

        assert_eq!(db.get_all_records(4).unwrap().len(), 1);

        let test_datetime = UtcDateTime::from_unix_timestamp(1213551070).unwrap();
        let test_datetime_end = UtcDateTime::from_unix_timestamp(1229362270).unwrap();
        assert_eq!(
            db.get_records_by_datetime(&test_datetime, &test_datetime_end)
                .unwrap()
                .len(),
            1
        );

        assert_eq!(aircrafts.iter().count(), 0);
    }

    #[test]
    fn test_table_print() {
        let home: GeoCoord = "51.455, -1.0281".parse().unwrap();
        let frames: Vec<Frame> = TEST_SBS1_FRAMES
            .iter()
            .map(|&x| Frame::parse(&x).unwrap())
            .collect();
        let mut aircrafts = AircraftsBuilder::new().home(&home).radius(10_000.0).build();

        for frame in &frames {
            aircrafts.feed(frame);
        }

        let planes: Vec<AircraftTableRow> = aircrafts.iter().map(|a| a.into()).collect();
        let tab = planes.with_title();
        assert!(print_stdout(tab).is_ok());
    }
}
