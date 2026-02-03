use burrow_lib::commands::math::try_calculate;
use burrow_lib::commands::settings::search_settings;
use burrow_lib::commands::ssh::{filter_hosts, parse_ssh_config_content};
use burrow_lib::router::{classify_query, RouteKind};

#[test]
fn math_expression_returns_result() {
    let result = try_calculate("2+2").unwrap();
    assert_eq!(result.name, "= 4");
    assert_eq!(result.category, "math");
}

#[test]
fn settings_prefix_returns_actions() {
    let results = search_settings("reindex").unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].category, "action");
}

#[test]
fn empty_query_classifies_as_history() {
    assert_eq!(classify_query(""), RouteKind::History);
}

#[test]
fn colon_classifies_as_settings() {
    assert_eq!(classify_query(":anything"), RouteKind::Settings);
}

#[test]
fn ssh_classifies_correctly() {
    assert_eq!(classify_query("ssh host"), RouteKind::Ssh);
    assert_eq!(classify_query("ssh"), RouteKind::Ssh);
    // "sshfs" is not ssh
    assert_eq!(classify_query("sshfs"), RouteKind::App);
}

#[test]
fn ssh_config_parse_and_filter_integration() {
    let config = "\
Host prod
    HostName prod.example.com
    User deploy

Host staging
    HostName staging.example.com
    User deploy

Host local
    HostName 127.0.0.1
";
    let hosts = parse_ssh_config_content(config);
    assert_eq!(hosts.len(), 3);

    let results = filter_hosts(hosts, "prod");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].name, "prod");
    // exec now contains just the host alias (no shell command for security)
    assert_eq!(results[0].exec, "prod");
}

#[test]
fn all_settings_have_consistent_structure() {
    let results = search_settings("").unwrap();
    for r in &results {
        assert_eq!(r.category, "action");
        assert!(r.exec.is_empty());
        assert!(r.name.starts_with(':'));
        assert!(!r.id.is_empty());
        assert!(!r.description.is_empty());
    }
}

#[test]
fn math_complex_expressions() {
    assert!(try_calculate("(10 + 5) * 2").is_some());
    // mexe doesn't support ^ or %, only +-*/
    assert!(try_calculate("2^8").is_none());
    assert!(try_calculate("100 % 7").is_none());
}

#[test]
fn math_non_expressions_return_none() {
    assert!(try_calculate("firefox").is_none());
    assert!(try_calculate("").is_none());
    assert!(try_calculate("hello world").is_none());
    assert!(try_calculate("42").is_none());
}
