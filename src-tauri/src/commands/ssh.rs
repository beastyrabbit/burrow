use crate::router::SearchResult;
use std::fs;

#[derive(Debug, Clone, PartialEq)]
struct SshHost {
    name: String,
    hostname: String,
    user: String,
}

fn parse_ssh_config_content(content: &str) -> Vec<SshHost> {
    let mut hosts = Vec::new();
    let mut current_name = String::new();
    let mut current_hostname = String::new();
    let mut current_user = String::new();

    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        if let Some(host) = line.strip_prefix("Host ").or_else(|| line.strip_prefix("Host\t")) {
            if !current_name.is_empty() && current_name != "*" {
                hosts.push(SshHost {
                    name: current_name.clone(),
                    hostname: if current_hostname.is_empty() {
                        current_name.clone()
                    } else {
                        current_hostname.clone()
                    },
                    user: current_user.clone(),
                });
            }
            current_name = host.trim().to_string();
            current_hostname.clear();
            current_user.clear();
        } else if let Some(val) = line
            .strip_prefix("HostName ")
            .or_else(|| line.strip_prefix("HostName\t"))
            .or_else(|| line.strip_prefix("Hostname "))
        {
            current_hostname = val.trim().to_string();
        } else if let Some(val) = line
            .strip_prefix("User ")
            .or_else(|| line.strip_prefix("User\t"))
        {
            current_user = val.trim().to_string();
        }
    }

    if !current_name.is_empty() && current_name != "*" {
        hosts.push(SshHost {
            name: current_name.clone(),
            hostname: if current_hostname.is_empty() {
                current_name
            } else {
                current_hostname
            },
            user: current_user,
        });
    }

    hosts
}

fn parse_ssh_config() -> Vec<SshHost> {
    let config_path = dirs::home_dir()
        .map(|h| h.join(".ssh/config"))
        .unwrap_or_default();

    match fs::read_to_string(&config_path) {
        Ok(content) => parse_ssh_config_content(&content),
        Err(_) => vec![],
    }
}

fn filter_hosts(hosts: Vec<SshHost>, query: &str) -> Vec<SearchResult> {
    let query_lower = query.to_lowercase();

    hosts
        .into_iter()
        .filter(|h| {
            query.is_empty()
                || h.name.to_lowercase().contains(&query_lower)
                || h.hostname.to_lowercase().contains(&query_lower)
        })
        .take(10)
        .map(|h| {
            let user_prefix = if h.user.is_empty() {
                String::new()
            } else {
                format!("{}@", h.user)
            };
            SearchResult {
                id: format!("ssh-{}", h.name),
                name: h.name.clone(),
                description: format!("{}{}", user_prefix, h.hostname),
                icon: "".into(),
                category: "ssh".into(),
                exec: format!("kitty ssh {}{}", user_prefix, h.name),
            }
        })
        .collect()
}

pub fn search_ssh(query: &str) -> Result<Vec<SearchResult>, String> {
    let hosts = parse_ssh_config();
    Ok(filter_hosts(hosts, query))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_CONFIG: &str = "\
Host server1
    HostName 192.168.1.10
    User admin

Host server2
    HostName example.com
    User deploy

Host dev-box
    Hostname 10.0.0.5

# This is a comment
Host *
    ServerAliveInterval 60
";

    #[test]
    fn parse_basic_config() {
        let hosts = parse_ssh_config_content(SAMPLE_CONFIG);
        assert_eq!(hosts.len(), 3);
    }

    #[test]
    fn parse_host_names() {
        let hosts = parse_ssh_config_content(SAMPLE_CONFIG);
        assert_eq!(hosts[0].name, "server1");
        assert_eq!(hosts[1].name, "server2");
        assert_eq!(hosts[2].name, "dev-box");
    }

    #[test]
    fn parse_hostnames() {
        let hosts = parse_ssh_config_content(SAMPLE_CONFIG);
        assert_eq!(hosts[0].hostname, "192.168.1.10");
        assert_eq!(hosts[1].hostname, "example.com");
        assert_eq!(hosts[2].hostname, "10.0.0.5");
    }

    #[test]
    fn parse_users() {
        let hosts = parse_ssh_config_content(SAMPLE_CONFIG);
        assert_eq!(hosts[0].user, "admin");
        assert_eq!(hosts[1].user, "deploy");
        assert_eq!(hosts[2].user, ""); // no user specified
    }

    #[test]
    fn wildcard_host_excluded() {
        let hosts = parse_ssh_config_content(SAMPLE_CONFIG);
        assert!(!hosts.iter().any(|h| h.name == "*"));
    }

    #[test]
    fn parse_empty_config() {
        let hosts = parse_ssh_config_content("");
        assert!(hosts.is_empty());
    }

    #[test]
    fn parse_comments_only() {
        let hosts = parse_ssh_config_content("# comment\n# another comment\n");
        assert!(hosts.is_empty());
    }

    #[test]
    fn parse_host_without_hostname_uses_name() {
        let config = "Host myserver\n    User root\n";
        let hosts = parse_ssh_config_content(config);
        assert_eq!(hosts[0].hostname, "myserver");
    }

    #[test]
    fn parse_tab_separated() {
        let config = "Host\ttabhost\n\tHostName\t10.0.0.1\n\tUser\troot\n";
        let hosts = parse_ssh_config_content(config);
        assert_eq!(hosts.len(), 1);
        assert_eq!(hosts[0].name, "tabhost");
        assert_eq!(hosts[0].hostname, "10.0.0.1");
        assert_eq!(hosts[0].user, "root");
    }

    #[test]
    fn parse_single_host_at_end() {
        let config = "Host lasthost\n    HostName last.example.com\n";
        let hosts = parse_ssh_config_content(config);
        assert_eq!(hosts.len(), 1);
        assert_eq!(hosts[0].name, "lasthost");
    }

    #[test]
    fn filter_empty_query_returns_all() {
        let hosts = parse_ssh_config_content(SAMPLE_CONFIG);
        let results = filter_hosts(hosts, "");
        assert_eq!(results.len(), 3);
    }

    #[test]
    fn filter_by_name() {
        let hosts = parse_ssh_config_content(SAMPLE_CONFIG);
        let results = filter_hosts(hosts, "server1");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "server1");
    }

    #[test]
    fn filter_by_hostname() {
        let hosts = parse_ssh_config_content(SAMPLE_CONFIG);
        let results = filter_hosts(hosts, "example");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "server2");
    }

    #[test]
    fn filter_case_insensitive() {
        let hosts = parse_ssh_config_content(SAMPLE_CONFIG);
        let results = filter_hosts(hosts, "SERVER1");
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn filter_no_match() {
        let hosts = parse_ssh_config_content(SAMPLE_CONFIG);
        let results = filter_hosts(hosts, "nonexistent");
        assert!(results.is_empty());
    }

    #[test]
    fn result_has_ssh_category() {
        let hosts = parse_ssh_config_content(SAMPLE_CONFIG);
        let results = filter_hosts(hosts, "server1");
        assert_eq!(results[0].category, "ssh");
    }

    #[test]
    fn result_exec_uses_kitty() {
        let hosts = parse_ssh_config_content(SAMPLE_CONFIG);
        let results = filter_hosts(hosts, "server1");
        assert!(results[0].exec.starts_with("kitty ssh"));
    }

    #[test]
    fn result_includes_user_in_description() {
        let hosts = parse_ssh_config_content(SAMPLE_CONFIG);
        let results = filter_hosts(hosts, "server1");
        assert!(results[0].description.contains("admin@"));
    }

    #[test]
    fn result_no_user_no_prefix() {
        let hosts = parse_ssh_config_content(SAMPLE_CONFIG);
        let results = filter_hosts(hosts, "dev-box");
        assert!(!results[0].description.contains("@"));
    }

    #[test]
    fn filter_limits_to_10() {
        let mut config = String::new();
        for i in 0..20 {
            config.push_str(&format!("Host host{i}\n    HostName {i}.example.com\n\n"));
        }
        let hosts = parse_ssh_config_content(&config);
        let results = filter_hosts(hosts, "host");
        assert_eq!(results.len(), 10);
    }
}
