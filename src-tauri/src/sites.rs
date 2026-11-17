//! "Сайты" in Settings: the user pastes a link and presses Добавить / Удалить.
//!
//! Добавить: the site goes through the bypass (zapret's list-general-user.txt;
//!           taken out of list-exclude-user.txt if it was excluded).
//! Удалить:  the site stops going through the bypass (removed from the user
//!           list, or added to list-exclude-user.txt when it comes from
//!           zapret's own list).

use std::fs;
use std::path::Path;

pub enum SiteError {
    Invalid,
    Exists,
    Missing,
    Io(String),
}

impl SiteError {
    pub fn code(&self) -> String {
        match self {
            SiteError::Invalid => "invalid".into(),
            SiteError::Exists => "exists".into(),
            SiteError::Missing => "missing".into(),
            SiteError::Io(e) => format!("io:{e}"),
        }
    }
}

/// `https://www.example.com/page?x` -> `example.com`
pub fn parse(raw: &str) -> Option<String> {
    let mut s = raw.trim().to_lowercase();
    if let Some(i) = s.find("://") {
        if s[..i].chars().all(|c| c.is_ascii_alphanumeric() || "+.-".contains(c)) {
            s = s[i + 3..].to_string();
        }
    }
    let s = s.split(['/', '?', '#', ' ', '\t']).next().unwrap_or("");
    let s = s.rsplit('@').next().unwrap_or("");
    let s = match s.rfind(':') {
        Some(i) if s[i + 1..].chars().all(|c| c.is_ascii_digit()) => &s[..i],
        _ => s,
    };
    let s = s.trim_start_matches("www.").trim_end_matches('.');
    let labels: Vec<&str> = s.split('.').collect();
    if labels.len() < 2 || s.len() > 253 {
        return None;
    }
    let label_ok = |l: &str| {
        !l.is_empty() && l.len() <= 63 && !l.starts_with('-') && !l.ends_with('-') && l.chars().all(|c| c.is_alphanumeric() || c == '-')
    };
    if !labels.iter().all(|l| label_ok(l)) {
        return None;
    }
    let tld = labels[labels.len() - 1];
    if !(tld.starts_with("xn--") || (tld.chars().count() >= 2 && tld.chars().all(|c| c.is_alphabetic()))) {
        return None;
    }
    Some(s.to_string())
}

fn read_list(p: &Path) -> Vec<String> {
    fs::read_to_string(p)
        .unwrap_or_default()
        .lines()
        .map(|l| l.trim().to_string())
        .collect()
}

fn entries(lines: &[String]) -> impl Iterator<Item = String> + '_ {
    lines
        .iter()
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(|l| l.trim_start_matches('^').to_lowercase())
}

/// zapret hostlists also match subdomains: `youtube.com` covers `m.youtube.com`.
fn covers(lines: &[String], domain: &str) -> bool {
    entries(lines).any(|e| domain == e || domain.ends_with(&format!(".{e}")))
}

fn has_exact(lines: &[String], domain: &str) -> bool {
    entries(lines).any(|e| e == domain)
}

fn write_list(p: &Path, lines: &[String], placeholder: bool) -> Result<(), SiteError> {
    let mut out: Vec<String> = lines.iter().filter(|l| !l.is_empty()).cloned().collect();
    // winws will not start with an empty list
    if placeholder && !out.iter().any(|l| !l.starts_with('#')) {
        out.push("domain.example.abc".into());
    }
    let mut text = out.join("\r\n");
    text.push_str("\r\n");
    fs::write(p, text).map_err(|e| SiteError::Io(e.to_string()))
}

fn remove_exact(lines: &[String], domain: &str) -> Vec<String> {
    lines
        .iter()
        .filter(|l| l.trim_start_matches('^').to_lowercase() != domain)
        .cloned()
        .collect()
}

pub fn add(install_dir: &Path, raw: &str) -> Result<String, SiteError> {
    let d = parse(raw).ok_or(SiteError::Invalid)?;
    let lists = install_dir.join("lists");
    let user_p = lists.join("list-general-user.txt");
    let excl_p = lists.join("list-exclude-user.txt");
    let user = read_list(&user_p);
    let general = read_list(&lists.join("list-general.txt"));
    let excl = read_list(&excl_p);

    let excluded = has_exact(&excl, &d);
    let covered = covers(&user, &d) || covers(&general, &d);
    if covered && !excluded {
        return Err(SiteError::Exists);
    }
    if excluded {
        write_list(&excl_p, &remove_exact(&excl, &d), true)?;
    }
    if !covered {
        let mut u = user;
        u.push(d.clone());
        write_list(&user_p, &u, true)?;
    }
    Ok(d)
}

pub fn remove(install_dir: &Path, raw: &str) -> Result<String, SiteError> {
    let d = parse(raw).ok_or(SiteError::Invalid)?;
    let lists = install_dir.join("lists");
    let user_p = lists.join("list-general-user.txt");
    let excl_p = lists.join("list-exclude-user.txt");
    let user = read_list(&user_p);
    let general = read_list(&lists.join("list-general.txt"));
    let excl = read_list(&excl_p);

    if has_exact(&excl, &d) {
        return Err(SiteError::Missing);
    }
    if has_exact(&user, &d) {
        write_list(&user_p, &remove_exact(&user, &d), true)?;
        if !covers(&general, &d) {
            return Ok(d);
        }
    }
    if covers(&general, &d) || covers(&user, &d) {
        let mut e = excl;
        e.push(d.clone());
        write_list(&excl_p, &e, true)?;
        return Ok(d);
    }
    Err(SiteError::Missing)
}

#[cfg(test)]
mod tests {
    use super::parse;

    #[test]
    fn parses_links() {
        assert_eq!(parse("https://www.YouTube.com/watch?v=1").as_deref(), Some("youtube.com"));
        assert_eq!(parse("discord.gg").as_deref(), Some("discord.gg"));
        assert_eq!(parse("http://user@site.org:8080/x").as_deref(), Some("site.org"));
        assert_eq!(parse("сайт.рф").as_deref(), Some("сайт.рф"));
        assert_eq!(parse("привет"), None);
        assert_eq!(parse("http://localhost"), None);
        assert_eq!(parse(""), None);
    }
}
