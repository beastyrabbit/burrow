use crate::router::SearchResult;
use std::fs;

#[derive(Debug)]
struct SshHost {
    name: String,
    hostname: String,
    user: String,
}

fn parse_ssh_config() -> Vec<SshHost> {
    let config_path = dirs::home_dir()
        .map(|h| h.join(".ssh/config"))
        .unwrap_or_default();

    let content = match fs::read_to_string(&config_path) {
        Ok(c) => c,
        Err(_) => return vec![],
    };

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

pub fn search_ssh(query: &str) -> Result<Vec<SearchResult>, String> {
    let hosts = parse_ssh_config();
    let query_lower = query.to_lowercase();

    let results: Vec<SearchResult> = hosts
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
        .collect();

    Ok(results)
}
