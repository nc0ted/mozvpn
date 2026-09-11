#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Location {
    pub code: &'static str,
    pub name: &'static str,
    pub host: &'static str,
}

pub static LOCATIONS: &[Location] = &[
    Location { code: "US", name: "United States", host: "us.m1.fastly-masque.net" },
    Location { code: "CA", name: "Canada", host: "ca.m1.fastly-masque.net" },
    Location { code: "GB", name: "United Kingdom", host: "eglc860.m1.fastly-masque.net" },
    Location { code: "DE", name: "Germany", host: "muc139.m1.fastly-masque.net" },
    Location { code: "FR", name: "France", host: "lfpb115.m1.fastly-masque.net" },
    Location { code: "NL", name: "Netherlands", host: "ehrd229.m1.fastly-masque.net" },
    Location { code: "SE", name: "Sweden", host: "essb127.m1.fastly-masque.net" },
    Location { code: "NO", name: "Norway", host: "osl65.m1.fastly-masque.net" },
    Location { code: "FI", name: "Finland", host: "hel141.m1.fastly-masque.net" },
    Location { code: "DK", name: "Denmark", host: "cph232.m1.fastly-masque.net" },
    Location { code: "IE", name: "Ireland", host: "dub43.m1.fastly-masque.net" },
    Location { code: "ES", name: "Spain", host: "leto235.m1.fastly-masque.net" },
    Location { code: "IT", name: "Italy", host: "mxp69.m1.fastly-masque.net" },
    Location { code: "AT", name: "Austria", host: "vie63.m1.fastly-masque.net" },
    Location { code: "BE", name: "Belgium", host: "bru148.m1.fastly-masque.net" },
    Location { code: "PT", name: "Portugal", host: "lis149.m1.fastly-masque.net" },
    Location { code: "BG", name: "Bulgaria", host: "sof151.m1.fastly-masque.net" },
    Location { code: "JP", name: "Japan", host: "rjtf770.m1.fastly-masque.net" },
    Location { code: "SG", name: "Singapore", host: "wsat188.m1.fastly-masque.net" },
    Location { code: "AU", name: "Australia", host: "ysbk106.m1.fastly-masque.net" },
    Location { code: "NZ", name: "New Zealand", host: "akl103.m1.fastly-masque.net" },
    Location { code: "KR", name: "Korea", host: "icn145.m1.fastly-masque.net" },
    Location { code: "BR", name: "Brazil", host: "sbsp209.m1.fastly-masque.net" },
    Location { code: "AR", name: "Argentina", host: "eze223.m1.fastly-masque.net" },
    Location { code: "CL", name: "Chile", host: "scl222.m1.fastly-masque.net" },
    Location { code: "CO", name: "Colombia", host: "skbo234.m1.fastly-masque.net" },
    Location { code: "MX", name: "Mexico", host: "mmqt107.m1.fastly-masque.net" },
    Location { code: "MY", name: "Malaysia", host: "kul98.m1.fastly-masque.net" },
    Location { code: "TH", name: "Thailand", host: "bkk228.m1.fastly-masque.net" },
    Location { code: "PH", name: "Philippines", host: "mnl215.m1.fastly-masque.net" },
    Location { code: "GH", name: "Ghana", host: "acc96.m1.fastly-masque.net" },
];

pub fn get_exit_host(code: &str) -> String {
    LOCATIONS
        .iter()
        .find(|l| l.code.eq_ignore_ascii_case(code))
        .map(|l| l.host.to_string())
        .unwrap_or_else(|| "us.m1.fastly-masque.net".to_string())
}

pub const EXIT_PORT: u16 = 2499;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_location_hosts() {
        assert_eq!(get_exit_host("NO"), "osl65.m1.fastly-masque.net");
        assert_eq!(get_exit_host("US"), "us.m1.fastly-masque.net");
        assert_eq!(get_exit_host("UNKNOWN"), "us.m1.fastly-masque.net");
    }
}
