use std::{
    io::{self, BufRead},
    net,
};
use time::{Duration, PrimitiveDateTime, UtcOffset};

use crate::Error;

#[derive(Debug, Clone, Copy, Default)]
pub(crate) enum MsgType {
    #[default]
    Unknown,
    EsIdentAndCategory,
    EsSurfacePos,
    EsAirbornePos,
    EsAirborneVel,
    SurveillanceAlt,
    SurveillanceId,
    AirToAir,
    AllCallReply,
}

impl TryFrom<&str> for MsgType {
    type Error = Error;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "0" => Ok(Self::Unknown),
            "1" => Ok(Self::EsIdentAndCategory),
            "2" => Ok(Self::EsSurfacePos),
            "3" => Ok(Self::EsAirbornePos),
            "4" => Ok(Self::EsAirborneVel),
            "5" => Ok(Self::SurveillanceAlt),
            "6" => Ok(Self::SurveillanceId),
            "7" => Ok(Self::AirToAir),
            "8" => Ok(Self::AllCallReply),
            _ => Err(Error::ParseError),
        }
    }
}

impl std::fmt::Display for MsgType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Self::Unknown => "Unknown Message Type",
            Self::EsIdentAndCategory => "ES Identification and Category",
            Self::EsSurfacePos => "ES Surface Position Message",
            Self::EsAirbornePos => "ES Airborne Position Message",
            Self::EsAirborneVel => "ES Airborne Velocity Message",
            Self::SurveillanceAlt => "Surveillance Alt Message",
            Self::SurveillanceId => "Surveillance ID Message",
            Self::AirToAir => "Air To Air Message",
            Self::AllCallReply => "All Call Reply",
        };

        write!(f, "{}", s)
    }
}

#[derive(Debug, Clone)]
pub struct Frame {
    tx_type: MsgType,
    pub hexident: u64,             // mode s hexadecimal code
    pub callsign: Option<[u8; 8]>, // 8 digits flight ID
    pub altitude: Option<f64>,
    pub ground_speed: Option<f64>,
    pub track: Option<f64>,
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
    pub vertical_rate: Option<f64>,
    pub is_grounded: Option<bool>,
    pub datetime_generated: PrimitiveDateTime, // local datetime of message generated
    pub datetime_logged: PrimitiveDateTime,    // local datetime of message logged
}

impl Frame {
    pub fn parse<T: AsRef<str>>(buf: &T) -> Result<Frame, Error> {
        let s = buf.as_ref();
        if !s.contains("\n") {
            return Err(Error::IncompleteFrame);
        }

        let mut s = s.to_string();
        s.pop(); // '\n'
        if s.contains('\r') {
            s.pop(); // '\r'
        }

        let splited: Vec<&str> = s.split(',').collect();
        if splited.len() != 22 {
            return Err(Error::UnknownFrame);
        }

        // we only care about parsing 'MSG' message here
        if splited[0].to_ascii_uppercase() != "MSG" {
            return Err(Error::UnknownFrame);
        }

        // destructing fields
        let tx_type = splited[1].try_into()?;
        let hexident = u64::from_str_radix(splited[4], 16).unwrap_or_default();
        let datetime_generated = parse_sbs1_datetime(splited[6], splited[7]).unwrap(); // it always presents, just unwrap it without checking
        let datetime_logged = parse_sbs1_datetime(splited[8], splited[9]).unwrap();
        let callsign = splited[10].as_bytes().as_array::<8>().copied();
        let altitude = splited[11].parse::<f64>().ok();
        let ground_speed = splited[12].parse().ok();
        let track = splited[13].parse().ok();
        let latitude = splited[14].parse().ok();
        let longitude = splited[15].parse().ok();
        let vertical_rate = splited[16].parse::<f64>().ok();
        let is_grounded = splited[21].parse::<i8>().ok().map(|raw| raw == -1);

        Ok(Frame {
            tx_type,
            hexident,
            altitude,
            ground_speed,
            latitude,
            longitude,
            track,
            vertical_rate,
            callsign,
            is_grounded,
            datetime_generated,
            datetime_logged,
        })
    }
}

const NO_CALLSIGN: [u8; 8] = [78, 79, 67, 65, 76, 83, 73, 71];
impl std::fmt::Display for Frame {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let callsign = {
            let s = self.callsign.unwrap_or(NO_CALLSIGN);
            str::from_utf8(&s).ok().unwrap().trim().to_string()
        };

        write!(
            f,
            "{}, callsign: \"{}\" ({:X}), speed: {}, alt: {}, lat: {}, lon: {}, track: {}, vrate: {}, grounded: {}, generated at {}, logged at {}",
            self.tx_type,
            callsign,
            self.hexident,
            self.ground_speed.unwrap_or_default(),
            self.altitude.unwrap_or_default(),
            self.latitude.unwrap_or_default(),
            self.longitude.unwrap_or_default(),
            self.track.unwrap_or_default(),
            self.vertical_rate.unwrap_or_default(),
            self.is_grounded.unwrap_or_default(),
            self.datetime_generated,
            self.datetime_logged,
        )
    }
}

pub struct TcpFetcher {
    stream: io::BufReader<net::TcpStream>,
}

impl TcpFetcher {
    pub fn new(socket: net::TcpStream) -> Self {
        Self {
            stream: io::BufReader::new(socket),
        }
    }

    /// read and parse one SBS1 frame
    pub fn read_frame(&mut self) -> Result<Frame, Error> {
        let mut line = String::new();

        // read and parse one line at a time
        match self.stream.read_line(&mut line) {
            // the socket is closed
            Ok(0) => {
                if line.is_empty() {
                    // it closes normally
                    return Err(Error::ConnectionClosed(io::ErrorKind::ConnectionAborted.into()).into());
                } else {
                    // reset by peer because there are still some bytes left without finish processing
                    return Err(Error::ConnectionClosed(io::ErrorKind::ConnectionReset.into()).into());
                }
            }
            // socket is not closed
            Ok(n) => n,
            Err(e) => {
                return Err(e.into());
            }
        };

        // try to parse frames
        match Frame::parse(&line) {
            Ok(frame) => Ok(frame),
            Err(e) => Err(e.into()),
        }
    }
}

#[inline]
fn parse_sbs1_datetime(date_str: &str, time_str: &str) -> Result<PrimitiveDateTime, Error> {
    let dt_fmt = time::format_description::parse(
        "[year]/[month]/[day] [hour]:[minute]:[second].[subsecond digits:3]",
    )
    .unwrap();
    let datetime_str = format!("{} {}", date_str, time_str);

    PrimitiveDateTime::parse(&datetime_str, &dt_fmt)
        .map(|datetime| {
            // correct to UTC time because SBS1 server is sending with local time
            let (off_h, off_m, off_s) = UtcOffset::current_local_offset()
                .unwrap_or(UtcOffset::UTC)
                .as_hms();
            datetime
                - (Duration::hours(off_h as i64)
                    + Duration::minutes(off_m as i64)
                    + Duration::seconds(off_s as i64))
        })
        .map_err(|_| Error::ParseError)
}

#[cfg(test)]
mod test {
    use crate::feeders::sbs1::*;

    const TEST_SBS1_FRAMES: &[&str] = &[
        "MSG,4,5,211,4CA2D6,10057,2008/11/28,14:53:49.986,2008/11/28,14:58:51.153,,,408.3,146.4,,,64,,,,,\r\n",
        "MSG,8,5,211,4CA2D6,10057,2008/11/28,14:53:50.391,2008/11/28,14:58:51.153,,,,,,,,,,,,0\r\n",
        "MSG,4,5,211,4CA2D6,10057,2008/11/28,14:53:50.391,2008/11/28,14:58:51.153,,,408.3,146.4,,,64,,,,,\r\n",
        "MSG,3,5,211,4CA2D6,10057,2008/11/28,14:53:50.594,2008/11/28,14:58:51.153,,37000,,,51.45735,-1.02826,,,0,0,0,0\r\n",
        "MSG,1,5,211,4CA2D6,11267,2008/11/28,23:48:18.611,2008/11/28,23:53:19.161,RJA1118 ,,,,,,,,,,,\r\n",
        "MSG,8,5,812,ABBEE3,10095,2008/11/28,14:53:50.594,2008/11/28,14:58:51.153,,,,,,,,,,,,0\r\n",
        "MSG,3,5,276,4010E9,10088,2008/11/28,14:53:49.986,2008/11/28,14:58:51.153,,28000,,,53.02551,-2.91389,,,0,0,0,0\r\n",
        "MSG,4,5,276,4010E9,10088,2008/11/28,14:53:50.188,2008/11/28,14:58:51.153,,,459.4,20.2,,,64,,,,,\r\n",
        "MSG,8,5,276,4010E9,10088,2008/11/28,14:53:50.594,2008/11/28,14:58:51.153,,,,,,,,,,,,0\r\n",
        "MSG,3,5,276,4010E9,10088,2008/11/28,14:53:50.594,2008/11/28,14:58:51.153,,28000,,,53.02677,-2.91310,,,0,0,0,0\r\n",
        "MSG,4,5,769,4CA2CB,10061,2008/11/28,14:53:50.188,2008/11/28,14:58:51.153,,,367.7,138.6,,,-2432,,,,,\r\n",
        "MSG,8,5,769,4CA2CB,10061,2008/11/28,14:53:50.391,2008/11/28,14:58:51.153,,,,,,,,,,,,0\r\n",
    ];

    #[test]
    fn test_frame_parser() {
        let parsed: Vec<Result<Frame, Error>> =
            TEST_SBS1_FRAMES.iter().map(|f| Frame::parse(f)).collect();

        let is_any_errs = parsed.iter().any(|v| v.is_err());
        assert_eq!(is_any_errs, false);
    }
}
