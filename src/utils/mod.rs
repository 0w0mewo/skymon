use anyhow::Result;
use sha2::Digest;
use std::fmt::Write;
use std::io::{self, Read};

pub mod geo;

pub fn sleep_ms(ms: u64) {
    std::thread::sleep(std::time::Duration::from_millis(ms));
}

pub fn sha256_digest<R: io::Read>(input: R) -> Result<String> {
    let mut reader = io::BufReader::new(input);
    let mut hasher = sha2::Sha256::new();
    let mut buffer = [0u8; 4096];

    loop {
        let bytes_read = reader.read(&mut buffer)?;
        if bytes_read == 0 {
            break;
        } else {
            hasher.update(&buffer[..bytes_read]);
        }
    }

    let hash = hasher
        .finalize()
        .iter()
        .try_fold::<String, _, Result<String>>(String::new(), |mut out, b| {
            write!(out, "{b:02x}")?;
            Ok(out)
        })?;

    Ok(hash)
}

#[cfg(feature = "download_aircrafts_metadata")]
pub fn fetch_aircrafts_csv_gz(url: &str) -> Result<impl io::Read + io::Seek> {
    let url = if url.is_empty() {
        "https://raw.githubusercontent.com/wiedehopf/tar1090-db/refs/heads/csv/aircraft.csv.gz"
    } else {
        url
    };

    let resp = ureq::get(url).call()?.body_mut().read_to_vec()?;

    Ok(io::Cursor::new(resp))
}

#[cfg(not(feature = "download_aircrafts_metadata"))]
pub fn fetch_aircrafts_csv_gz(csv_gzpath: &str) -> Result<impl io::Read + io::Seek> {
    use anyhow::Context;
    use std::fs;

    Ok(fs::File::open(csv_gzpath).context("fail to open aircrafts gzipped csv")?)
}

#[cfg(test)]
mod test {
    use std::fs;

    use crate::utils::*;

    #[test]
    fn test_sha256_digest() {
        let agz_file = fs::File::open("assets/aircraft.csv.gz").unwrap();
        assert!(sha256_digest(agz_file).is_ok())
    }
}
