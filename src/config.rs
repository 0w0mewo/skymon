use anyhow::Result;

#[derive(Debug, Clone, serde::Deserialize)]
pub struct Config {
    #[serde(default = "Config::default_sbs1_server")]
    pub sbs1_server: String,

    #[serde(rename(deserialize = "max_dist"), default = "Config::default_max_dist")]
    pub detection_dist: f64,

    #[serde(rename(deserialize = "max_alt"), default = "Config::default_max_alt")]
    pub detection_altitude: f64,

    #[serde(rename(deserialize="home_coord"), default = "Config::default_home")]
    pub home: String,

    #[serde(
        rename(deserialize = "sqlite_path"),
        default = "Config::default_db_path"
    )]
    pub db_path: String,

    #[serde(default = "Config::default_flush_period_mins")]
    pub flush_period_mins: i64,

    #[serde(default = "Config::default_slient_mode")]
    pub slient: bool,

    #[serde(
        rename(deserialize = "disp_all_aircrafts"),
        default = "Config::default_display_all_aircrafts"
    )]
    pub disp_all: bool,

    #[serde(default = "Config::minimum_refresh_rate_ms")]
    pub disp_refresh_rate_ms: u64,
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
sqlite DB path: {}"#,
            self.sbs1_server,
            self.detection_altitude,
            self.detection_dist,
            self.disp_all,
            self.slient,
            self.flush_period_mins,
            self.home,
            self.db_path,
        )
    }
}

impl Config {
    pub fn new() -> Result<Self> {
        envy::from_env().map_err(|e| e.into())
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
}

#[cfg(test)]
mod test {
    use crate::{config::*, geo::GeoCoord};

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
        assert_eq!(conf.home, Config::default_home());
        assert!(conf.home.parse::<GeoCoord>().is_ok());
    }
}
