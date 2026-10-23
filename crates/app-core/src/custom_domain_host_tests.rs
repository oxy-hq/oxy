use super::*;

fn zones() -> Vec<String> {
    vec![
        "oxygen-hq.com".to_string(),
        "customer-apps.oxygen-hq.com".to_string(),
        "dev.oxy.tech".to_string(),
    ]
}

#[test]
fn normalizes_case_and_trailing_dot() {
    assert_eq!(
        normalize_domain_against("  App.Acme.COM. ", &zones()),
        Ok("app.acme.com".to_string())
    );
}

#[test]
fn normalizes_an_internationalised_name_to_punycode() {
    assert_eq!(
        normalize_domain_against("café.example.org", &zones()),
        Ok("xn--caf-dma.example.org".to_string())
    );
}

#[test]
fn refuses_urls_ports_paths_and_wildcards() {
    for input in [
        "https://app.acme.com",
        "app.acme.com/",
        "app.acme.com:443",
        "*.acme.com",
        "user@acme.com",
        "app acme.com",
        "",
    ] {
        assert_eq!(
            normalize_domain_against(input, &zones()),
            Err(DomainError::NotAHostname),
            "{input}"
        );
    }
}

#[test]
fn refuses_ip_literals() {
    assert_eq!(
        normalize_domain_against("10.0.0.1", &zones()),
        Err(DomainError::IpAddress)
    );
    assert_eq!(
        normalize_domain_against("1.2.3", &zones()),
        Err(DomainError::IpAddress)
    );
}

#[test]
fn refuses_a_single_label() {
    assert_eq!(
        normalize_domain_against("intranet", &zones()),
        Err(DomainError::SingleLabel)
    );
}

#[test]
fn refuses_bad_labels() {
    for input in [
        "-app.acme.com",
        "app-.acme.com",
        "a..acme.com",
        "app_1.acme.com",
    ] {
        assert!(
            normalize_domain_against(input, &zones()).is_err(),
            "{input}"
        );
    }
    let long = format!("{}.acme.com", "a".repeat(64));
    assert_eq!(
        normalize_domain_against(&long, &zones()),
        Err(DomainError::NotAHostname)
    );
}

#[test]
fn refuses_reserved_suffixes() {
    assert_eq!(
        normalize_domain_against("app.localhost", &zones()),
        Err(DomainError::Reserved("localhost".to_string()))
    );
    assert_eq!(
        normalize_domain_against("oxy.oxy.svc.cluster.local", &zones()),
        Err(DomainError::Reserved("local".to_string()))
    );
}

#[test]
fn refuses_every_platform_zone_and_its_apex() {
    for input in [
        "oxygen-hq.com",
        "app.oxygen-hq.com",
        "acme--store.customer-apps.oxygen-hq.com",
        "pokehouse.dev.oxy.tech",
    ] {
        assert!(
            matches!(
                normalize_domain_against(input, &zones()),
                Err(DomainError::PlatformZone(_))
            ),
            "{input}"
        );
    }
}

#[test]
fn a_lookalike_of_a_platform_zone_is_not_inside_it() {
    // `evil-oxygen-hq.com` ends with the zone's text but is not below it.
    assert_eq!(
        normalize_domain_against("evil-oxygen-hq.com", &zones()),
        Ok("evil-oxygen-hq.com".to_string())
    );
}

#[test]
fn candidate_host_skips_platform_ip_and_internal_hosts() {
    let z = zones();
    for host in [
        "app.oxygen-hq.com",
        "acme--store.customer-apps.oxygen-hq.com:443",
        "localhost:3000",
        "127.0.0.1:3000",
        "[::1]:3000",
        "oxy-serve",
        "oxy-serve.oxy.svc.cluster.local",
        "10.0.4.17",
    ] {
        assert_eq!(candidate_host_against(host, &z), None, "{host}");
    }
}

#[test]
fn candidate_host_normalizes_a_customer_host() {
    assert_eq!(
        candidate_host_against("App.Acme.com:443", &zones()),
        Some("app.acme.com".to_string())
    );
    assert_eq!(
        candidate_host_against("app.acme.com.", &zones()),
        Some("app.acme.com".to_string())
    );
}

#[test]
fn challenge_record_names_and_values() {
    assert_eq!(
        challenge_name("app.acme.com"),
        "_oxygen-challenge.app.acme.com"
    );
    assert_eq!(challenge_value("abc"), "oxygen-verification=abc");
}
