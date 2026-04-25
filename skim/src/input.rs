use std::net::IpAddr;

use serde::Deserialize;

#[derive(Deserialize)]
struct DnsLine {
    status: String,
    data: Option<DnsData>,
}

#[derive(Deserialize)]
struct DnsData {
    answers: Option<Vec<DnsAnswer>>,
}

#[derive(Deserialize)]
struct DnsAnswer {
    #[serde(rename = "type")]
    type_: String,
    name: String,
    data: String,
}

/// Parse one massdns NDJSON line. Returns `Some((host, ip))` for the first
/// `A` record on a `NOERROR` response; `None` for any other shape, including
/// records whose `data` field doesn't parse as an IP address.
pub fn parse_dns_line(line: &str) -> Option<(String, IpAddr)> {
    let dns: DnsLine = serde_json::from_str(line).ok()?;
    if dns.status != "NOERROR" {
        return None;
    }
    let a = dns.data?.answers?.into_iter().find(|a| a.type_ == "A")?;
    let ip: IpAddr = a.data.parse().ok()?;
    let mut host = a.name;
    if host.ends_with('.') {
        host.pop();
    }
    Some((host, ip))
}

#[cfg(test)]
mod tests {
    use std::net::{IpAddr, Ipv4Addr};

    use super::parse_dns_line;

    #[test]
    fn extracts_first_a_record() {
        let line = r#"{"name":"example.com.","type":"A","class":"IN","status":"NOERROR","data":{"answers":[{"ttl":300,"type":"A","class":"IN","name":"example.com.","data":"1.2.3.4"}]}}"#;
        assert_eq!(
            parse_dns_line(line),
            Some((
                "example.com".to_string(),
                IpAddr::V4(Ipv4Addr::new(1, 2, 3, 4))
            ))
        );
    }

    #[test]
    fn skips_nxdomain() {
        let line = r#"{"name":"nope.example.","type":"A","status":"NXDOMAIN","data":{}}"#;
        assert_eq!(parse_dns_line(line), None);
    }

    #[test]
    fn skips_response_without_answers() {
        let line = r#"{"name":"x.com.","type":"A","status":"NOERROR","data":{"authorities":[{"type":"SOA"}]}}"#;
        assert_eq!(parse_dns_line(line), None);
    }

    #[test]
    fn skips_garbage() {
        assert_eq!(parse_dns_line("not json"), None);
    }

    #[test]
    fn skips_unparseable_ip() {
        let line = r#"{"name":"x.com.","type":"A","status":"NOERROR","data":{"answers":[{"type":"A","name":"x.com.","data":"not-an-ip"}]}}"#;
        assert_eq!(parse_dns_line(line), None);
    }
}
