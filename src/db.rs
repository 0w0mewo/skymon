use std::{
    fs,
    io::{self, BufRead},
};

use anyhow::{Context, Result, anyhow};
use flate2::read::GzDecoder;
use rusqlite::{self as rqlite, named_params};
use time::UtcDateTime;

use crate::{geo::GeoCoord, utils::AircraftTableRow};

#[derive(Debug, Clone)]
pub struct AircraftEntry {
    pub hexident: u64,
    pub callsign: String,
    pub closest_location: GeoCoord,
    pub closest_dist: f64,
    pub closest_at: UtcDateTime,
}

impl AircraftEntry {
    fn is_valid(&self) -> bool {
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

impl From<&AircraftEntry> for AircraftTableRow {
    fn from(value: &AircraftEntry) -> Self {
        Self {
            hexident: value.hexident,
            callsign: value.callsign.clone(),
            position: value.closest_location.clone(),
            last_seen: value.closest_at,
            dist: value.closest_dist,
        }
    }
}

impl From<AircraftEntry> for AircraftTableRow {
    fn from(value: AircraftEntry) -> Self {
        Self {
            hexident: value.hexident,
            callsign: value.callsign,
            position: value.closest_location,
            last_seen: value.closest_at,
            dist: value.closest_dist,
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

pub struct Database {
    conn: rqlite::Connection,
}

impl Database {
    pub fn open(db_path: &str) -> Result<Database> {
        let conn = rqlite::Connection::open(db_path)?;
        let db = Self { conn };

        db.setup_pragma().context("fail to setup PRAGMAs")?;
        db.setup_tables().context("fail to create table")?;

        Ok(db)
    }

    fn setup_pragma(&self) -> Result<()> {
        let conn = &self.conn;

        conn.pragma_update(None, "foreign_keys", "ON")?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "journal_size_limit", "200000000")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        conn.pragma_update(None, "temp_store", "MEMORY")?;
        conn.pragma_update(None, "cache_size", "-16000")?;

        Ok(())
    }

    fn setup_tables(&self) -> Result<()> {
        self.conn.execute_batch(
            r#"    
    CREATE TABLE IF NOT EXISTS "aircrafts" (
        "hexident" INTEGER NOT NULL,
        "callsign" VARCHAR(8),
        "last_update" DATETIME,
        "closest_dist" NUMERIC,
        "closest_lat" NUMERIC,
        "closest_lon" NUMERIC,
        PRIMARY KEY("hexident", "callsign")
    );

    CREATE INDEX IF NOT EXISTS "aircraft_index_0"
    ON "aircrafts" ("hexident", "last_update");
    

    CREATE TABLE IF NOT EXISTS "registry" (
        "hexident" INTEGER NOT NULL,
        "reg" VARCHAR(20),
        "type" VARCHAR(8),
        "year" INTEGER,
        "descr" TEXT,
        "owner" TEXT,
        PRIMARY KEY("hexident")
    );
        "#,
        )?;

        Ok(())
    }

    pub fn get_records_by_hexident(&self, hexident: u64) -> Result<Vec<AircraftEntry>> {
        let conn = &self.conn;

        let mut stmt = conn.prepare_cached(
            r#"SELECT callsign, last_update, closest_dist, closest_lat, closest_lon 
            FROM aircrafts
            WHERE hexident = :hexident;
            "#,
        )?;

        let res: Vec<AircraftEntry> = stmt
            .query_map(named_params! {":hexident": hexident as i64}, |row| {
                let (lat, lon) = (row.get(3)?, row.get(4)?);

                Ok(AircraftEntry {
                    hexident: hexident,
                    callsign: row.get(0)?,
                    closest_at: row.get(1)?,
                    closest_dist: row.get(2)?,
                    closest_location: GeoCoord::new(lat, lon),
                })
            })?
            .map(|r| r.unwrap_or_default())
            .collect();

        if res.is_empty() {
            Err(rqlite::Error::QueryReturnedNoRows.into())
        } else {
            Ok(res)
        }
    }

    pub fn get_metadata_by_hexident(&self, hexident: u64) -> Result<AircraftMetadataEntry> {
        let conn = &self.conn;

        let mut stmt = conn.prepare_cached(
            r#"SELECT reg, type, descr, year, owner 
            FROM registry
            WHERE hexident = :hexident;
            "#,
        )?;

        let res = stmt.query_one(named_params! {":hexident": hexident as i64}, |row| {
            Ok(AircraftMetadataEntry {
                hexident: hexident,
                reg: row.get(0)?,
                short_type: row.get(1)?,
                descr: row.get(2)?,
                year: row.get(3)?,
                owner: row.get(4)?,
            })
        })?;

        Ok(res)
    }

    pub fn get_all_records(&self, limit: u64) -> Result<Vec<AircraftEntry>> {
        let conn = &self.conn;

        let mut stmt = conn.prepare_cached(
            r#"SELECT hexident, callsign, last_update, closest_dist, closest_lat, closest_lon 
            FROM aircrafts ORDER BY last_update DESC LIMIT :limit;
            "#,
        )?;

        let res: Vec<AircraftEntry> = stmt
            .query_map(named_params! {":limit": limit as i64}, |row| {
                let (lat, lon) = (row.get(4)?, row.get(5)?);
                let hexident: i64 = row.get(0)?;

                Ok(AircraftEntry {
                    hexident: hexident as u64,
                    callsign: row.get(1)?,
                    closest_at: row.get(2)?,
                    closest_dist: row.get(3)?,
                    closest_location: GeoCoord::new(lat, lon),
                    ..Default::default()
                })
            })?
            .map(|r| r.unwrap_or_default())
            .collect();

        if res.is_empty() {
            Err(rqlite::Error::QueryReturnedNoRows.into())
        } else {
            Ok(res)
        }
    }

    pub fn get_records_by_datetime(
        &self,
        start: &UtcDateTime,
        end: &UtcDateTime,
    ) -> Result<Vec<AircraftEntry>> {
        let conn = &self.conn;

        let mut stmt = conn.prepare_cached(
            r#"SELECT hexident, callsign, last_update, closest_dist, closest_lat, closest_lon 
            FROM aircrafts 
            WHERE last_update BETWEEN :start AND :end 
            ORDER BY last_update DESC;
            "#,
        )?;

        let res: Vec<AircraftEntry> = stmt
            .query_map(named_params! {":start": start, ":end": end}, |row| {
                let (lat, lon) = (row.get(4)?, row.get(5)?);
                let hexident: i64 = row.get(0)?;

                Ok(AircraftEntry {
                    hexident: hexident as u64,
                    callsign: row.get(1)?,
                    closest_at: row.get(2)?,
                    closest_dist: row.get(3)?,
                    closest_location: GeoCoord::new(lat, lon),
                })
            })?
            .map(|r| r.unwrap_or_default())
            .collect();

        if res.is_empty() {
            Err(rqlite::Error::QueryReturnedNoRows.into())
        } else {
            Ok(res)
        }
    }

    pub fn insert(&self, a: &AircraftEntry) -> Result<()> {
        if !a.is_valid() {
            return Err(anyhow!("invalid aircraft entry, possible missing fields"));
        }

        if a.closest_location.is_valid() {
            self.conn.execute(
            r#"
            INSERT INTO aircrafts (hexident, callsign, last_update, closest_dist, closest_lat, closest_lon) 
                VALUES (:hexident, :callsign, :last_update, :closest_dist, :closest_lat, :closest_lon) ON CONFLICT(hexident, callsign) 
                DO UPDATE SET 
                    last_update = excluded.last_update,
                    closest_dist = excluded.closest_dist,
                    closest_lat = excluded.closest_lat,
                    closest_lon = excluded.closest_lon;
            "#,
            named_params! {":hexident": a.hexident as i64, ":callsign": a.callsign, ":last_update": a.closest_at, 
            ":closest_dist": a.closest_dist,":closest_lat": a.closest_location.lat, ":closest_lon": a.closest_location.lon },
        )?;
        }

        Ok(())
    }

    pub fn metadata_count(&self) -> Result<usize> {
        let conn = &self.conn;

        let mut stmt = conn.prepare_cached(
            r#"SELECT count(hexident) 
            FROM registry;
            "#,
        )?;

        let res = stmt.query_one([], |row| {
            let count: isize = row.get(0)?;

            Ok(count)
        })?;

        Ok(res as usize)
    }

    pub fn import_metadata_from_gzipped_csv(
        &mut self,
        csv_gz_path: &str,
        batch_size: usize,
    ) -> Result<()> {
        let agz_file = fs::File::open(csv_gz_path).context("fail to open aircrafts gzipped csv")?;
        let agz_file = io::BufReader::with_capacity(1024, agz_file);
        let mut decoded = io::BufReader::new(GzDecoder::new(agz_file));

        // parse and insert csv rows
        let batch_import = |conn: &mut rqlite::Connection, rows: &[String]| -> Result<()> {
            let tx = conn.transaction()?;

            for row in rows {
                let mut splited = row.split(';');
                let (mut hexid, mut reg, mut _type, mut descr, mut year, mut owner) = (0u64, "UNKNOWN", "UNKNOWN", "", 0, "");

                if let Some(hexident) = splited.next() {
                    hexid = u64::from_str_radix(hexident, 16).unwrap_or_default();
                }

                if let Some(r) = splited.next() {
                    reg = r;
                }

                if let Some(t) = splited.next() {
                    _type = t;
                }

                // ignore flag
                _ = splited.next();

                if let Some(d) = splited.next() {
                    descr = d;
                }

                if let Some(y) = splited.next() {
                    year = y.parse().unwrap_or_default();
                }

                if let Some(o) = splited.next() {
                    owner = o;
                }

                let mut stmt = tx.prepare_cached(
                    "
                    INSERT INTO registry (hexident, reg, type, descr, owner, year) 
                        VALUES (:hexident, :reg, :type, :descr, :owner, :year) ON CONFLICT(hexident) 
                        DO UPDATE SET 
                        reg = excluded.reg,
                        type = excluded.type,
                        descr = excluded.descr,
                        owner = excluded.owner,
                        year = excluded.year                        
                        ;",
                )?;

                stmt.execute(named_params! {":hexident": hexid as i64, ":reg": reg, ":type": _type, ":descr": descr, ":owner": owner, ":year": year})?;
            }

            Ok(tx.commit()?)
        };

        let mut batched_rows: Vec<String> = Vec::with_capacity(batch_size);
        loop {
            let mut line_buf = String::new();
            match decoded.read_line(&mut line_buf) {
                Err(e) => return Err(e.into()),
                Ok(0) => {
                    break;
                }

                Ok(_) => {
                    if line_buf.ends_with('\n') {
                        line_buf.pop();
                    }

                    batched_rows.push(line_buf);
                }
            }

            // leave one element in the vec to avoid the expensive resize operation
            if batched_rows.len() > batch_size - 1 {
                batch_import(&mut self.conn, &batched_rows)?;
                batched_rows.clear();
            }
        }

        // remaining parts
        if !batched_rows.is_empty() {
            batch_import(&mut self.conn, &batched_rows)?;
        }

        Ok(())
    }
}
