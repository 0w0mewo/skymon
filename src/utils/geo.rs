use map_3d::{self};
use std::{fmt::write, hash::Hash, ops::Sub};

#[derive(Debug, Clone, thiserror::Error)]
pub enum Error {
    #[error("inconsistent coordinate type")]
    InconsistCoordType,
}

/// cartesian coordinate
#[derive(Debug, Clone)]
pub enum CartesianCoord {
    ThreeDimension { x: f64, y: f64, z: f64 },
    TwoDimension { x: f64, y: f64 },
}

impl CartesianCoord {
    pub fn new_3d(x: f64, y: f64, z: f64) -> Self {
        Self::ThreeDimension { x, y, z }
    }

    pub fn new_2d(x: f64, y: f64) -> Self {
        Self::TwoDimension { x, y }
    }

    /// distance to another coord
    pub fn distance(&self, to: &CartesianCoord) -> Result<f64, Error> {
        match self {
            CartesianCoord::ThreeDimension { x, y, z } => match to {
                CartesianCoord::ThreeDimension {
                    x: to_x,
                    y: to_y,
                    z: to_z,
                } => Ok(((x - to_x) * (x - to_x)
                    + (y - to_y) * (y - to_y)
                    + (z - to_z) * (z - to_z))
                    .sqrt()),
                _ => Err(Error::InconsistCoordType),
            },
            CartesianCoord::TwoDimension { x, y } => match to {
                CartesianCoord::TwoDimension { x: to_x, y: to_y } => {
                    Ok(((x - to_x) * (x - to_x) + (y - to_y) * (y - to_y)).sqrt())
                }
                _ => Err(Error::InconsistCoordType),
            },
        }
    }

    pub fn to_canvas_coord(
        &self,
        rotation_angle: f64,
        canvas_width: u64,
        canvas_height: u64,
        geo_scale: f64,
    ) -> Self {
        let (x, y) = match self {
            CartesianCoord::ThreeDimension { x, y, .. } => (*x, *y),
            CartesianCoord::TwoDimension { x, y } => (*x, *y),
        };

        // precalc cosine and sine of rotation angle for transform
        let (c, s) = (
            rotation_angle.to_radians().cos(),
            rotation_angle.to_radians().sin(),
        );

        // apply roation transform
        let x = x * c - y * s;
        let y = x * s + y * c;

        // scale
        let pixel_per_meter = (canvas_width.min(canvas_height) as f64) / geo_scale;
        let x = 0.5 * ((canvas_width as f64) + x * pixel_per_meter);
        let y = 0.5 * ((canvas_height as f64) + y * pixel_per_meter);

        Self::TwoDimension { x, y }
    }
}

impl From<SphericalCoord> for CartesianCoord {
    fn from(value: SphericalCoord) -> Self {
        (&value).into()
    }
}

impl From<&SphericalCoord> for CartesianCoord {
    fn from(value: &SphericalCoord) -> Self {
        let (x, y, z) = map_3d::aer2enu(value.azimuth, value.elevation, value.radius);

        Self::new_3d(x, y, z)
    }
}

impl From<GeoCoord> for CartesianCoord {
    fn from(value: GeoCoord) -> Self {
        (&value).into()
    }
}

impl From<&GeoCoord> for CartesianCoord {
    fn from(value: &GeoCoord) -> Self {
        let (x, y, z) = map_3d::geodetic2ecef(
            value.lat.to_radians(),
            value.lon.to_radians(),
            value.alt,
            map_3d::Ellipsoid::default(),
        );

        Self::new_3d(x, y, z)
    }
}

impl std::fmt::Display for CartesianCoord {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ThreeDimension { x, y, z } => write(f, format_args!("({}, {}, {})", x, y, z)),
            Self::TwoDimension { x, y } => write(f, format_args!("({}, {})", x, y)),
        }
    }
}

/// spherical coordinate, angles are in degrees
pub struct SphericalCoord {
    pub azimuth: f64,   // azimuth angle in degrees (x-y angle)
    pub elevation: f64, // elevation angle in degrees (x-z angle)
    pub radius: f64,    // slant range in meters
}

impl SphericalCoord {
    pub fn new(azimuth: f64, elevation: f64, radius: f64) -> Self {
        Self {
            azimuth,
            elevation,
            radius,
        }
    }
}

impl TryFrom<&CartesianCoord> for SphericalCoord {
    type Error = Error;

    fn try_from(value: &CartesianCoord) -> Result<Self, Self::Error> {
        match value {
            CartesianCoord::ThreeDimension { x, y, z } => {
                let (az, el, r) = map_3d::enu2aer(*x, *y, *z);

                Ok(Self {
                    azimuth: az.to_degrees(),
                    elevation: el.to_degrees(),
                    radius: r,
                })
            }
            _ => Err(Error::InconsistCoordType),
        }
    }
}

impl TryFrom<CartesianCoord> for SphericalCoord {
    type Error = Error;

    fn try_from(value: CartesianCoord) -> Result<Self, Self::Error> {
        (&value).try_into()
    }
}

impl std::fmt::Display for SphericalCoord {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write(
            f,
            format_args!("({}, {}, {})", self.azimuth, self.elevation, self.radius),
        )
    }
}

/// GPS coordinate, lantitude and longtitude are in degrees
#[derive(Debug, Clone)]
pub struct GeoCoord {
    pub lat: f64, // lantitude in degrees
    pub lon: f64, // longtitude in degrees
    pub alt: f64, // altitude in meters
}

const INVALIDATE_GEOCOORD: GeoCoord = GeoCoord {
    lat: f64::INFINITY,
    lon: f64::INFINITY,
    alt: f64::INFINITY,
};

impl Default for GeoCoord {
    fn default() -> Self {
        INVALIDATE_GEOCOORD
    }
}

impl Default for &GeoCoord {
    fn default() -> Self {
        &INVALIDATE_GEOCOORD
    }
}

impl GeoCoord {
    pub fn new(lat: f64, lon: f64) -> Self {
        Self {
            lat,
            lon,
            alt: f64::default(),
        }
    }

    pub fn new_with_altitude(lat: f64, lon: f64, alt: f64) -> Self {
        Self { lat, lon, alt }
    }

    pub fn is_valid(&self) -> bool {
        self.lat.is_finite() && self.lon.is_finite()
    }

    /// relative position, return `XyzCoord`
    pub fn relative_to(&self, reference: &GeoCoord) -> CartesianCoord {
        let (x, y, z) = map_3d::geodetic2enu(
            self.lat.to_radians(),
            self.lon.to_radians(),
            self.alt,
            reference.lat.to_radians(),
            reference.lon.to_radians(),
            reference.alt,
            map_3d::Ellipsoid::default(),
        );

        // let(x,y,z) = map_3d::enu2uvw(x, y, z, reference.lan.to_radians(), reference.lon.to_radians());

        CartesianCoord::new_3d(x, y, z)
    }

    /// relative position, return `SphericalCoord`
    pub fn spherical_relative_to(&self, reference: &GeoCoord) -> SphericalCoord {
        let (azimuth, elevation, radius) = map_3d::geodetic2aer(
            self.lat.to_radians(),
            self.lon.to_radians(),
            self.alt,
            reference.lat.to_radians(),
            reference.lon.to_radians(),
            reference.alt,
            map_3d::Ellipsoid::default(),
        );

        SphericalCoord {
            azimuth,
            elevation,
            radius,
        }
    }

    /// distance to destination coordinate
    pub fn distance(&self, to: &GeoCoord) -> f64 {
        map_3d::distance((self.lat, self.lon), (to.lat, to.lon))
    }
}

impl Sub for GeoCoord {
    type Output = f64;

    fn sub(self, rhs: Self) -> Self::Output {
        self.distance(&rhs)
    }
}

impl Sub for &GeoCoord {
    type Output = f64;

    fn sub(self, rhs: Self) -> Self::Output {
        self.distance(rhs)
    }
}

impl std::fmt::Display for GeoCoord {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if !self.is_valid() {
            write!(f, "None")
        } else {
            match self.alt {
                0.0 => write!(f, "lat: {:0<.5}, lon: {:0<.5}", self.lat, self.lon),
                _ => write!(
                    f,
                    "lat: {:0<.5}, lon: {:0<.5}, alt: {:0<.2}",
                    self.lat, self.lon, self.alt
                ),
            }
        }
    }
}

impl std::str::FromStr for GeoCoord {
    type Err = crate::Error;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let mut splitted = value.split(",");
        if splitted.clone().count() != 2 {
            return Err(Self::Err::ParseError);
        }

        let (lat, lon) = (splitted.next().unwrap(), splitted.next().unwrap());
        let Ok(lat) = lat.trim().parse::<f64>() else {
            return Err(Self::Err::ParseError);
        };

        let Ok(lon) = lon.trim().parse::<f64>() else {
            return Err(Self::Err::ParseError);
        };

        Ok(GeoCoord { lat, lon, alt: 0.0 })
    }
}

impl std::cmp::Ord for GeoCoord {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        let dist = self - other;
        dist.total_cmp(&0.0)
    }
}

impl PartialOrd for GeoCoord {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for GeoCoord {
    fn eq(&self, other: &Self) -> bool {
        self.lat == other.lat && self.lon == other.lon
    }
}

impl Eq for GeoCoord {}

impl Hash for GeoCoord {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        // derive from 'ordered-float' crate
        let f64_to_bits = |v: &f64| -> u64 {
            if v.is_nan() {
                0x7ff8000000000000 // NaN
            } else {
                (v + 0.0).to_bits()
            }
        };

        f64_to_bits(&self.lat).hash(state);
        f64_to_bits(&self.lon).hash(state);
        f64_to_bits(&self.alt).hash(state);
    }
}

#[cfg(test)]
mod test {
    use crate::utils::geo::{self, GeoCoord};

    #[test]
    fn test_geo() {
        let cur = GeoCoord::new_with_altitude(-33.723, 150.882, 3560.0);
        let home = GeoCoord::new(-33.8064, 151.0781);

        let dist = &cur - &home;
        assert!(dist - 20361.76 < 0.01);

        let p_xyz = cur.relative_to(&home);
        let expected_p_xyz = geo::CartesianCoord::new_3d(-18185.35, 9238.44, 3527.41);
        assert!(p_xyz.distance(&expected_p_xyz).unwrap() < 0.01);

        let p_sphere: geo::SphericalCoord = p_xyz.clone().try_into().unwrap();
        assert!(p_sphere.azimuth - 296.93 < 0.01);
        assert!(p_sphere.elevation - 9.81 < 0.01);
        assert!(p_sphere.radius - 20700.20 < 0.01);
    }

    #[test]
    fn test_geo_parsing() {
        let should_parsed_coord: Result<GeoCoord, crate::Error> = "-0.8064, 0.0781".parse();
        assert!(should_parsed_coord.is_ok_and(|x| { x.lat == -0.8064 && x.lon == 0.0781 }));

        let incorrect_corrd: Result<GeoCoord, crate::Error> = "-, 2".parse();
        assert!(incorrect_corrd.is_err());

        let incorrect_corrd: Result<GeoCoord, crate::Error> = "-, ".parse();
        assert!(incorrect_corrd.is_err());
    }

    #[test]
    fn test_geo_disp() {
        let cur = geo::GeoCoord::new(-33.723, 150.882);
        println!("{}", cur);

        let default = geo::GeoCoord::default();
        println!("{}", default);
    }
}
