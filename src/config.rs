use std::env;

use anyhow::{Context, Result};

use crate::geo::GeoCoord;

#[derive(Debug, Clone)]
pub struct Config {
    pub sbs1_server: String,
    pub detection_dist: f64,
    pub detection_altitude: f64,
    pub home: GeoCoord,
    pub db_path: String,
    pub flsuh_period: time::Duration,
    pub slient: bool,
    pub disp_all: bool
}

impl Default for Config {
    fn default() -> Self {
        Self {
            sbs1_server: "127.0.0.1:30003".into(),
            detection_dist: 5000.0,
            detection_altitude: 2000.0,
            home: "0.0,0.0".parse().unwrap(),
            db_path: ":memory:".into(),
            flsuh_period: time::Duration::minutes(5),
            slient: false,
            disp_all: true
        }
    }
}

impl Config {
    pub fn from_env() -> Result<Self> {
        let sbs1_server = env::var("SBS1_SERVER").context("missing SBS1 server")?;
        let detection_dist = env::var("MAX_DIST")
            .unwrap_or_default()
            .parse::<f64>()
            .context("missing detection distance")?;
        let detection_altitude = env::var("MAX_ALT")
            .unwrap_or_default()
            .parse::<f64>()
            .context("missing detection altitude")?;
        let home = env::var("HOME")
            .context("missing home coordinate")?
            .parse()
            .context("fail to parse home coordinate")?;
        let db_path = env::var("SQLITE_PATH").unwrap_or(":memory:".into());
        let flush_period_minutes = env::var("FLUSH_PERIOD")
            .unwrap_or_default()
            .parse::<i64>()
            .unwrap_or(5);
        let slient = env::var("SLIENT").unwrap_or_default().parse().unwrap_or(true);
        let disp_all = env::var("DISP_ALL_AIRCRAFTS").unwrap_or_default().parse::<bool>().unwrap_or(false);

        Ok(Self {
            sbs1_server,
            detection_dist,
            detection_altitude,
            home: home,
            db_path,
            flsuh_period: time::Duration::minutes(flush_period_minutes),
            slient,
            disp_all,
        })
    }
}
