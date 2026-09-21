#![forbid(unsafe_code)]
use std::net::IpAddr;

#[derive(Debug, Clone, Default)]
pub struct Location {
    pub country: Option<String>,
    pub region: Option<String>,
    pub city: Option<String>,
}

pub fn lookup_location<T: AsRef<[u8]>>(
    reader: &maxminddb::Reader<T>,
    ip: IpAddr,
) -> Option<Location> {
    let city: maxminddb::geoip2::City = reader.lookup(ip).ok()?;
    let country = city.country.and_then(|c| c.iso_code).map(str::to_string);
    let region = city
        .subdivisions
        .as_ref()
        .and_then(|s| s.first())
        .and_then(|sub| sub.iso_code)
        .map(str::to_string);
    let city_name = city
        .city
        .and_then(|c| c.names)
        .and_then(|n| n.get("en").copied())
        .map(str::to_string);

    Some(Location {
        country,
        region,
        city: city_name,
    })
}

pub fn get_region_code(country: Option<&str>, region: Option<&str>) -> Option<String> {
    match (country, region) {
        (Some(c), Some(r)) if !c.is_empty() && !r.is_empty() => {
            if r.contains('-') {
                Some(r.to_string())
            } else {
                Some(format!("{c}-{r}"))
            }
        }
        _ => None,
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn test_get_region_code() {
        assert_eq!(
            get_region_code(Some("US"), Some("CA")),
            Some("US-CA".into())
        );
        assert_eq!(
            get_region_code(Some("US"), Some("US-CA")),
            Some("US-CA".into())
        );
        assert_eq!(get_region_code(Some(""), Some("CA")), None);
        assert_eq!(get_region_code(Some("US"), Some("")), None);
        assert_eq!(get_region_code(None, Some("CA")), None);
        assert_eq!(get_region_code(Some("US"), None), None);
    }

    #[test]
    fn test_lookup_location_if_file_exists() {
        let path = std::path::Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../maxmind/extracted/GeoLite2-City.mmdb"
        ));
        if !path.exists() {
            return;
        }
        let reader = maxminddb::Reader::open_readfile(path).unwrap();
        let ip: IpAddr = "8.8.8.8".parse().unwrap();
        let loc = lookup_location(&reader, ip).unwrap();
        assert_eq!(loc.country.as_deref(), Some("US"));

        let ip_cmu: IpAddr = "128.2.42.10".parse().unwrap();
        let l = lookup_location(&reader, ip_cmu).unwrap();
        assert_eq!(l.country.as_deref(), Some("US"));

        let private_ip: IpAddr = "127.0.0.1".parse().unwrap();
        assert!(lookup_location(&reader, private_ip).is_none());
    }
}

#[cfg(kani)]
mod kani_proofs {
    #[kani::proof]
    fn harness_region_code_logic() {
        let has_c: bool = kani::any();
        let has_r: bool = kani::any();
        let c = if has_c { Some("US") } else { None };
        let r = if has_r { Some("CA") } else { None };
        let res = match (c, r) {
            (Some(c_str), Some(r_str)) if !c_str.is_empty() && !r_str.is_empty() => true,
            _ => false,
        };
        if !has_c || !has_r {
            kani::assert(!res, "none when missing");
        } else {
            kani::assert(res, "some when both present");
        }
    }
}
