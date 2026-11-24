//! Logging-hulpjes. Geheimen mogen nooit in logs of foutmeldingen belanden
//! (harde regel 4), dus alles gaat door `redact` voordat het wordt weggeschreven.

const MASK: &str = "[redacted]";

fn looks_secret(word: &str) -> bool {
    let w = word.trim_matches(|c: char| !c.is_ascii_alphanumeric() && c != '-' && c != '_' && c != '.');
    let long = w.len() >= 20;
    (w.starts_with("sk-") && long)
        || (w.starts_with("ghp_") && long)
        || (w.starts_with("github_pat_") && long)
        || (w.starts_with("AIza") && long)
        || (w.starts_with("eyJ") && w.matches('.').count() == 2 && long)
        || (w.starts_with("xox") && w.contains('-') && long)
}

fn is_secret_key(word: &str) -> bool {
    let w = word.to_ascii_lowercase();
    ["token", "secret", "password", "passwd", "apikey", "api_key", "authorization", "api-key"]
        .iter()
        .any(|k| w.contains(k))
}

/// Maskeert API-sleutels, tokens en `key=value`-paren met gevoelige namen.
pub fn redact(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut mask_next = false;
    for (i, part) in input.split(' ').enumerate() {
        if i > 0 {
            out.push(' ');
        }
        if mask_next && !part.is_empty() {
            out.push_str(MASK);
            mask_next = false;
            continue;
        }
        if let Some((k, _)) = part.split_once('=').or_else(|| part.split_once(':')) {
            if is_secret_key(k) && part.len() > k.len() + 1 {
                out.push_str(k);
                out.push(if part.contains('=') { '=' } else { ':' });
                out.push_str(MASK);
                continue;
            }
        }
        if looks_secret(part) {
            out.push_str(MASK);
            continue;
        }
        let lower = part.to_ascii_lowercase();
        if lower == "bearer" || lower == "basic" || lower == "token" {
            mask_next = true;
        }
        out.push_str(part);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::redact;

    #[test]
    fn masks_api_keys_and_bearers() {
        assert_eq!(redact("key sk-ant-api03-abcdefghijklmnopqrstuv end"), "key [redacted] end");
        assert_eq!(redact("Authorization: Bearer abc123def456"), "Authorization: Bearer [redacted]");
        assert_eq!(redact("token=abcd1234"), "token=[redacted]");
        assert_eq!(redact("ghp_abcdefghijklmnopqrstuvwxyz0123"), "[redacted]");
    }

    #[test]
    fn leaves_normal_text_alone() {
        let s = "flutter doctor finished in 12s: 3 issues";
        assert_eq!(redact(s), s);
        assert_eq!(redact("sk-short"), "sk-short");
    }
}
