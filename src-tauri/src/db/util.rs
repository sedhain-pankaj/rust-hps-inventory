use argon2::{
    password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};
use chrono::{Datelike, Local, NaiveDate};
use rand_core::OsRng;
use sha2::{Digest, Sha256};

pub fn now_string() -> String {
    Local::now().format("%Y-%m-%dT%H:%M:%S").to_string()
}

pub fn today_string() -> String {
    Local::now().date_naive().format("%Y-%m-%d").to_string()
}

pub fn parse_date_or_today(value: Option<String>) -> NaiveDate {
    value
        .and_then(|text| NaiveDate::parse_from_str(&text, "%Y-%m-%d").ok())
        .unwrap_or_else(|| Local::now().date_naive())
}

pub fn week_start_for(date: NaiveDate) -> NaiveDate {
    let weekday = date.weekday().num_days_from_monday() as i64;
    let wednesday = 2_i64;
    let delta = (weekday + 7 - wednesday) % 7;
    date - chrono::Duration::days(delta)
}

pub fn hash_password(password: &str) -> String {
    let salt = SaltString::generate(&mut OsRng);
    if let Ok(hash) = Argon2::default().hash_password(password.as_bytes(), &salt) {
        return hash.to_string();
    }
    legacy_sha256(password)
}

pub fn verify_password(stored_hash: &str, password: &str) -> bool {
    if stored_hash.starts_with("$argon2") {
        let Ok(parsed) = PasswordHash::new(stored_hash) else {
            return false;
        };
        return Argon2::default()
            .verify_password(password.as_bytes(), &parsed)
            .is_ok();
    }
    stored_hash == legacy_sha256(password)
}

pub fn is_legacy_password_hash(stored_hash: &str) -> bool {
    !stored_hash.starts_with("$argon2")
}

fn legacy_sha256(password: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(password.as_bytes());
    to_hex(&hasher.finalize())
}

fn to_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }
    output
}

pub fn format_seconds(seconds: i64) -> String {
    let hours = seconds / 3600;
    let minutes = (seconds % 3600) / 60;
    format!("{hours:02}:{minutes:02}")
}

pub fn clean_cell(value: impl AsRef<str>) -> String {
    value.as_ref().trim().trim_matches('"').trim().to_string()
}

pub(crate) fn clean_series(value: String) -> String {
    clean_cell(value).replace(['\u{201c}', '\u{201d}'], "")
}

pub fn normalize_unit(unit: &str) -> String {
    let t = unit.trim();
    if t.is_empty() || t == "??" {
        "Unknown".to_string()
    } else {
        t.to_string()
    }
}

pub fn expand_ambiguous(model: &str, unit: &str) -> Vec<(String, String)> {
    let parts: Vec<String> = unit
        .split(" or ")
        .map(|p| p.trim().to_string())
        .filter(|p| !p.is_empty())
        .collect();
    if parts.len() <= 1 {
        vec![(model.to_string(), normalize_unit(unit))]
    } else {
        parts
            .iter()
            .enumerate()
            .map(|(i, p)| (format!("{} (ambg {})", model, i + 1), normalize_unit(p)))
            .collect()
    }
}

pub fn unit_value(unit: &str) -> Option<f64> {
    let t = unit.trim();
    if t.is_empty() || t.eq_ignore_ascii_case("unknown") {
        return None;
    }
    if let Some((a, b)) = t.split_once('/') {
        if let (Ok(num), Ok(den)) = (a.trim().parse::<f64>(), b.trim().parse::<f64>()) {
            if den != 0.0 {
                return Some(num / den);
            }
        }
    }
    if let Ok(n) = t.parse::<f64>() {
        return Some(n);
    }
    first_number(t)
}

fn first_number(value: &str) -> Option<f64> {
    let mut started = false;
    let mut number = String::new();
    for ch in value.chars() {
        if ch.is_ascii_digit() || (ch == '.' && started) {
            started = true;
            number.push(ch);
        } else if started {
            break;
        }
    }
    number.parse::<f64>().ok()
}

pub(crate) fn parse_csv(content: &str) -> Vec<Vec<String>> {
    let mut rows = Vec::new();
    let mut row = Vec::new();
    let mut cell = String::new();
    let mut chars = content.chars().peekable();
    let mut in_quotes = false;

    while let Some(ch) = chars.next() {
        match ch {
            '"' if in_quotes && chars.peek() == Some(&'"') => {
                cell.push('"');
                chars.next();
            }
            '"' => in_quotes = !in_quotes,
            ',' if !in_quotes => {
                row.push(cell.trim().to_string());
                cell.clear();
            }
            '\n' if !in_quotes => {
                row.push(cell.trim_end_matches('\r').trim().to_string());
                cell.clear();
                if row.iter().any(|value| !value.is_empty()) {
                    rows.push(row);
                }
                row = Vec::new();
            }
            _ => cell.push(ch),
        }
    }

    if !cell.is_empty() || !row.is_empty() {
        row.push(cell.trim_end_matches('\r').trim().to_string());
        if row.iter().any(|value| !value.is_empty()) {
            rows.push(row);
        }
    }

    rows
}

#[cfg(test)]
mod tests {
    use super::*;
#[test]
    fn normalize_unit_converts_question_marks() {
        assert_eq!(normalize_unit("??"), "Unknown");
        assert_eq!(normalize_unit("  ??  "), "Unknown");
        assert_eq!(normalize_unit(""), "Unknown");
        assert_eq!(normalize_unit("1.5"), "1.5");
    }

    #[test]
    fn expand_ambiguous_splits_or_units() {
        assert_eq!(
            expand_ambiguous("491", "1.2 or 2"),
            vec![
                ("491 (ambg 1)".to_string(), "1.2".to_string()),
                ("491 (ambg 2)".to_string(), "2".to_string()),
            ]
        );
        assert_eq!(
            expand_ambiguous("404", "1.5"),
            vec![("404".to_string(), "1.5".to_string())]
        );
        assert_eq!(
            expand_ambiguous("722", "??"),
            vec![("722".to_string(), "Unknown".to_string())]
        );
        assert_eq!(
            expand_ambiguous("909", "2 or 15/6"),
            vec![
                ("909 (ambg 1)".to_string(), "2".to_string()),
                ("909 (ambg 2)".to_string(), "15/6".to_string()),
            ]
        );
    }

    #[test]
    fn unit_value_parses_numbers_fractions_and_unknown() {
        assert_eq!(unit_value("1.25"), Some(1.25));
        assert_eq!(unit_value("2"), Some(2.0));
        assert_eq!(unit_value("15/6"), Some(2.5));
        assert_eq!(unit_value("Unknown"), None);
        assert_eq!(unit_value("??"), None);
        assert_eq!(unit_value(""), None);
    }
}
