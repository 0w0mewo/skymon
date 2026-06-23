use std::io::{self, BufRead};

use flate2::read::GzDecoder;
use rusqlite::{self as rqlite, named_params};
use time::UtcDateTime;

use crate::{
    database::{
        Error, QueryResult,
        models::{AircraftEntry, AircraftMetadataEntry},
    },
    utils::geo::GeoCoord,
};

pub(crate) struct Database {
    conn: rqlite::Connection,
}

impl Database {
    pub fn open(db_path: &str) -> QueryResult<Database> {
        let conn = rqlite::Connection::open(db_path)?;
        let db = Self { conn };

        db.setup_pragma()?;
        db.setup_tables()?;

        Ok(db)
    }

    fn setup_pragma(&self) -> rqlite::Result<()> {
        let conn = &self.conn;

        conn.pragma_update(None, "foreign_keys", "ON")?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "journal_size_limit", "200000000")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        conn.pragma_update(None, "temp_store", "MEMORY")?;
        conn.pragma_update(None, "cache_size", "-16000")?;

        Ok(())
    }

    fn setup_tables(&self) -> rqlite::Result<()> {
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

    CREATE TABLE IF NOT EXISTS "registry_version" (
        "id" INTEGER NOT NULL,
        "hash" VARCHAR(64) NOT NULL UNIQUE,
        PRIMARY KEY("id")
    );
    
    CREATE INDEX IF NOT EXISTS "rv_index_0"
    ON "registry_version" ("hash");
    "#,
        )?;

        Ok(())
    }

    #[allow(dead_code)]
    pub fn get_records_by_hexident(&self, hexident: u64) -> QueryResult<Vec<AircraftEntry>> {
        let conn = &self.conn;

        let mut stmt = conn.prepare_cached(
            r#"SELECT aircrafts.callsign, registry.reg, registry.type, last_update, closest_dist, closest_lat, closest_lon  
            FROM aircrafts 
            INNER JOIN registry ON aircrafts.hexident=registry.hexident
            WHERE aircrafts.hexident = :hexident;
            "#,
        )?;

        let res: Vec<AircraftEntry> = stmt
            .query_map(named_params! {":hexident": hexident}, |row| {
                let (lat, lon) = (row.get(5)?, row.get(6)?);

                Ok(AircraftEntry {
                    hexident: hexident,
                    callsign: row.get(0)?,
                    reg: row.get(1)?,
                    short_type: row.get(2)?,
                    closest_at: row.get(3)?,
                    closest_dist: row.get(4)?,
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

    pub fn get_metadata_by_hexident(&self, hexident: u64) -> QueryResult<AircraftMetadataEntry> {
        let conn = &self.conn;

        let mut stmt = conn.prepare_cached(
            r#"SELECT reg, type, descr, year, owner 
            FROM registry
            WHERE hexident = :hexident;
            "#,
        )?;

        let res = stmt.query_one(named_params! {":hexident": hexident}, |row| {
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

    pub fn get_all_records(&self, limit: u64) -> QueryResult<Vec<AircraftEntry>> {
        let conn = &self.conn;

        let mut stmt = conn.prepare_cached(
            r#"SELECT aircrafts.hexident, aircrafts.callsign, registry.reg, registry.type, last_update, closest_dist, closest_lat, closest_lon  
            FROM aircrafts 
            INNER JOIN registry ON aircrafts.hexident=registry.hexident
            ORDER BY last_update DESC LIMIT :limit;
            "#,
        )?;

        let res: Vec<AircraftEntry> = stmt
            .query_map(named_params! {":limit": limit}, |row| {
                let (lat, lon) = (row.get(4)?, row.get(5)?);

                Ok(AircraftEntry {
                    hexident: row.get(0)?,
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
    ) -> QueryResult<Vec<AircraftEntry>> {
        let conn = &self.conn;

        let mut stmt = conn.prepare_cached(
            r#"SELECT aircrafts.hexident, aircrafts.callsign, registry.reg, registry.type, last_update, closest_dist, closest_lat, closest_lon  
            FROM aircrafts 
            INNER JOIN registry ON aircrafts.hexident=registry.hexident 
            WHERE last_update BETWEEN :start AND :end 
            ORDER BY last_update DESC;
            "#,
        )?;

        let res: Vec<AircraftEntry> = stmt
            .query_map(named_params! {":start": start, ":end": end}, |row| {
                let (lat, lon) = (row.get(6)?, row.get(7)?);

                Ok(AircraftEntry {
                    hexident: row.get(0)?,
                    callsign: row.get(1)?,
                    reg: row.get(2)?,
                    short_type: row.get(3)?,
                    closest_at: row.get(4)?,
                    closest_dist: row.get(5)?,
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

    pub fn delete_records_older_than(&self, start: &UtcDateTime) -> QueryResult<()> {
        let conn = &self.conn;

        let mut stmt = conn.prepare_cached(
            r#"DELETE FROM aircrafts
            WHERE last_update < :start;
            "#,
        )?;

        stmt.execute(named_params! {":start": start})?;

        Ok(())
    }

    pub fn insert(&self, a: &AircraftEntry) -> QueryResult<()> {
        if !a.is_valid() {
            return Err(Error::InvalidInput);
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
            named_params! {":hexident": a.hexident, ":callsign": a.callsign, ":last_update": a.closest_at, 
            ":closest_dist": a.closest_dist,":closest_lat": a.closest_location.lat, ":closest_lon": a.closest_location.lon },
        )?;
        }

        Ok(())
    }

    pub(crate) fn get_registry_version(&self) -> QueryResult<String> {
        let conn = &self.conn;
        let mut stmt = conn.prepare_cached(
            r#"SELECT hash
            FROM registry_version;
            "#,
        )?;

        // we don't care about Err(QueryReturnedNoRows), it just mean there is not thing imported to the `registry` table
        let res = stmt
            .query_one([], |row| Ok(row.get(0)?))
            .unwrap_or_default();

        Ok(res)
    }

    pub(crate) fn insert_registry_version(&self, hash: &str) -> QueryResult<()> {
        self.conn.execute(
            r#"INSERT INTO registry_version (id, hash) 
                VALUES (1, :hash) ON CONFLICT(id) 
                DO UPDATE SET
                    hash = excluded.hash;"#,
            named_params! {":hash": hash},
        )?;

        Ok(())
    }

    pub fn import_metadata_from_gzipped_csv<Z: io::Read>(
        &mut self,
        csv_gz: Z,
        batch_size: usize,
    ) -> QueryResult<()> {
        let mut decoded = io::BufReader::new(GzDecoder::new(csv_gz));
        let mut total_imported = 0usize;
        let mut expecting_total_imported = 0usize;

        // parse and insert csv rows
        let batch_import = |conn: &mut rqlite::Connection, rows: &[String]| -> QueryResult<()> {
            let tx = conn.transaction()?;

            for row in rows {
                let mut splited = row.split(';');
                let (mut hexid, mut reg, mut _type, mut descr, mut year, mut owner) =
                    (0u64, "UNKNOWN", "UNKNOWN", "", 0, "");

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

                stmt.execute(named_params! {":hexident": hexid, ":reg": reg, ":type": _type, ":descr": descr, ":owner": owner, ":year": year})?;
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
                    expecting_total_imported += 1;
                }
            }

            // leave one element in the vec to avoid the expensive resize operation
            if batched_rows.len() > batch_size - 1 {
                batch_import(&mut self.conn, &batched_rows)?;
                total_imported += batched_rows.len();
                batched_rows.clear();
            }
        }

        // remaining parts
        if !batched_rows.is_empty() {
            batch_import(&mut self.conn, &batched_rows)?;
            total_imported += batched_rows.len();
        }

        if total_imported != expecting_total_imported {
            return Err(Error::Unknown(format!(
                "some rows are failed for unknown reason: expecting: {}, got: {}",
                expecting_total_imported, total_imported
            )));
        }

        Ok(())
    }
}
