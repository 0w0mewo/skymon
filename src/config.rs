use anyhow::Result;
use clap::Parser;

#[derive(Debug, Clone, Parser)]
#[command(version, about, long_about=None)]
pub struct Config {
    #[arg(long, short='s', default_value_t=Config::default_sbs1_server(), help="SBS1 server")]
    pub sbs1_server: String,

    #[arg(long="dist", env="MAX_DIST", default_value_t=Config::default_max_dist(), help="Maximum distance of aircrafts that will be logged to database")]
    pub detection_dist: f64,

    #[arg(long="alt",env="MAX_ALT" ,default_value_t=Config::default_max_alt(), help="Maximum altitude of aircrafts that will be logged to database")]
    pub detection_altitude: f64,

    #[arg(long, short='H', env="HOME_COORD", default_value_t=Config::default_home(), help="Home GPS coordinate")]
    pub home: String,

    #[arg(long="sqlite-path", short='d', env="SQLITE_PATH", default_value_t=Config::default_db_path(), help="Path to sqlite3 database")]
    pub db_path: String,

    #[arg(long="flush-period", env="FLUSH_PERIOD" ,default_value_t=Config::default_flush_period_mins(), help="Database flush period in minutes")]
    pub flush_period_mins: i64,

    #[arg(long, default_value_t=Config::default_slient_mode(), help="Slient mode")]
    pub slient: bool,

    #[arg(long, env="DISP_ALL_AIRCRAFTS", default_value_t=Config::default_display_all_aircrafts(), help="Display all aircrafts, otherwise only display the aircrafts within 'dist' and 'alt'")]
    pub disp_all: bool,

    #[arg(long, default_value_t=Config::minimum_refresh_rate_ms(), help="Display refresh rate in ms")]
    pub disp_refresh_rate_ms: u64,

    #[arg(long, default_value_t=Config::default_should_record_positions(), help="")]
    pub enable_position_recording: bool,

    #[arg(long, short='D', default_value_t=Config::default_clean_older_than_days(), help="Delete recorded aircrafts older than some days")]
    pub delete_older_than_days: u32,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            sbs1_server: Self::default_sbs1_server(),
            detection_dist: Self::default_max_dist(),
            detection_altitude: Self::default_max_alt(),
            home: Self::default_home(),
            db_path: Self::default_db_path(),
            flush_period_mins: Self::default_flush_period_mins(),
            slient: Self::default_slient_mode(),
            disp_all: Self::default_display_all_aircrafts(),
            disp_refresh_rate_ms: Self::minimum_refresh_rate_ms(),
            enable_position_recording: Self::default_should_record_positions(),
            delete_older_than_days: Self::default_clean_older_than_days(),
        }
    }
}

impl std::fmt::Display for Config {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            r#"SBS1 server: {}, Max altitude: {}, Max distance: {}, 
Display all aircrafts: {}, Slient mode: {}, 
Flush period: {} mins, 
Home: {}, 
sqlite DB path: {},
Enable position recording: {}
"#,
            self.sbs1_server,
            self.detection_altitude,
            self.detection_dist,
            self.disp_all,
            self.slient,
            self.flush_period_mins,
            self.home,
            self.db_path,
            self.enable_position_recording,
        )
    }
}

impl Config {
    pub fn new() -> Result<Self> {
        Ok(Self::try_parse()?)
    }

    #[inline]
    fn default_home() -> String {
        "0.0,0.0".into()
    }

    #[inline]
    fn default_db_path() -> String {
        ":memory:".into()
    }

    #[inline]
    fn default_max_alt() -> f64 {
        3000.0
    }

    #[inline]
    fn default_max_dist() -> f64 {
        5000.0
    }

    #[inline]
    fn default_slient_mode() -> bool {
        false
    }

    #[inline]
    fn default_display_all_aircrafts() -> bool {
        true
    }

    #[inline]
    fn default_flush_period_mins() -> i64 {
        5
    }

    #[inline]
    fn default_sbs1_server() -> String {
        "127.0.0.1:30003".into()
    }

    #[inline]
    pub fn minimum_refresh_rate_ms() -> u64 {
        450
    }

    #[inline]
    fn default_should_record_positions() -> bool {
        false
    }

    #[inline]
    fn default_clean_older_than_days() -> u32 {
        30
    }
}

#[cfg(test)]
mod test {
    use crate::{config::*, utils::geo::GeoCoord};

    #[test]
    fn test_default_config() {
        let conf = Config::new().unwrap();

        assert_eq!(conf.db_path, Config::default_db_path());
        assert_eq!(conf.detection_altitude, Config::default_max_alt());
        assert_eq!(conf.detection_dist, Config::default_max_dist());
        assert_eq!(conf.disp_all, Config::default_display_all_aircrafts());
        assert_eq!(conf.flush_period_mins, Config::default_flush_period_mins());
        assert_eq!(conf.disp_refresh_rate_ms, Config::minimum_refresh_rate_ms());
        assert_eq!(conf.sbs1_server, Config::default_sbs1_server());
        assert_eq!(conf.slient, Config::default_slient_mode());
        assert_eq!(
            conf.enable_position_recording,
            Config::default_should_record_positions()
        );
        assert_eq!(conf.home, Config::default_home());
        assert!(conf.home.parse::<GeoCoord>().is_ok());
    }
}
