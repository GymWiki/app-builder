//! De scrubber (fase 10d): alle tekst die naar de UI, de logs, de chat of het model gaat, loopt eerst langs dit filter.
//! Bekende geheime waarden (uit de kluis) worden door •••• vervangen, ook in veelvoorkomende varianten (JSON-escaped,
//! URL-gecodeerd, regel voor regel bij sleutelbestanden). Gangbare sleutelpatronen vangt `logging::redact` af.
//!
//! Randgevallen: waarden korter dan 8 tekens worden niet vervangen (anders verminkt een wachtwoord als "1234" gewone tekst),
//! overlappende waarden worden van lang naar kort vervangen en een waarde die midden in een andere tekst voorkomt wordt daar
//! ook vervangen (liever te veel dan een lek).

use serde_json::Value;
use std::collections::BTreeMap;
use std::sync::RwLock;

pub const MASK: &str = "••••";
const MIN_LEN: usize = 8;

static KNOWN: RwLock<BTreeMap<String, Vec<String>>> = RwLock::new(BTreeMap::new());

fn percent_encode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// De waarde zoals die in andere tekst kan voorkomen.
pub fn variants(raw: &str) -> Vec<String> {
    let mut v = vec![raw.to_string()];
    let json = serde_json::to_string(raw).unwrap_or_default();
    if json.len() >= 2 {
        v.push(json[1..json.len() - 1].to_string());
    }
    v.push(percent_encode(raw));
    v.retain(|x| x.chars().count() >= MIN_LEN);
    v.sort();
    v.dedup();
    v
}

/// Waarden die uit een bestand in andere tekst kunnen belanden (service-account-JSON, PEM-sleutel). Binaire bestanden
/// (keystores) leveren niets op: die kunnen niet als tekst voorkomen.
pub fn values_from_file(bytes: &[u8]) -> Vec<String> {
    let Ok(text) = std::str::from_utf8(bytes) else { return vec![] };
    let mut out = vec![text.trim().to_string()];
    if let Ok(v) = serde_json::from_str::<Value>(text) {
        fn walk(v: &Value, out: &mut Vec<String>) {
            match v {
                Value::Object(m) => {
                    for (k, x) in m {
                        let k = k.to_lowercase();
                        if let Value::String(s) = x {
                            if ["private_key", "secret", "password", "token"].iter().any(|w| k.contains(w)) {
                                out.push(s.clone());
                                out.extend(s.lines().map(str::trim).filter(|l| l.len() >= 16 && !l.starts_with("-----")).map(String::from));
                            }
                        } else {
                            walk(x, out);
                        }
                    }
                }
                Value::Array(a) => a.iter().for_each(|x| walk(x, out)),
                _ => {}
            }
        }
        walk(&v, &mut out);
    } else if text.contains("-----BEGIN") {
        let body: String = text.lines().filter(|l| !l.starts_with("-----")).map(str::trim).collect();
        out.push(body);
        out.extend(text.lines().map(str::trim).filter(|l| l.len() >= 16 && !l.starts_with("-----")).map(String::from));
    }
    out
}

/// Houdt de bekende waarden van één bron (veld) bij; een lege lijst verwijdert de bron.
pub fn register(owner: &str, raw_values: &[String]) {
    let mut all: Vec<String> = raw_values.iter().flat_map(|v| variants(v)).collect();
    all.sort();
    all.dedup();
    let mut k = KNOWN.write().unwrap_or_else(|e| e.into_inner());
    if all.is_empty() {
        k.remove(owner);
    } else {
        k.insert(owner.to_string(), all);
    }
}

pub fn unregister(owner: &str) {
    KNOWN.write().unwrap_or_else(|e| e.into_inner()).remove(owner);
}

/// Alleen de bekende waarden (patronen doet `logging::redact`).
pub fn replace_known(text: &str) -> String {
    let k = KNOWN.read().unwrap_or_else(|e| e.into_inner());
    if k.is_empty() {
        return text.to_string();
    }
    let mut vals: Vec<&String> = k.values().flatten().collect();
    vals.sort_by_key(|v| std::cmp::Reverse(v.len()));
    let mut out = text.to_string();
    for v in vals {
        if out.contains(v.as_str()) {
            out = out.replace(v.as_str(), MASK);
        }
    }
    out
}

/// Alle bekende waarden (met varianten), voor de lekcontrole.
pub fn known_values() -> Vec<String> {
    KNOWN.read().unwrap_or_else(|e| e.into_inner()).values().flatten().cloned().collect()
}

pub fn scrub(text: &str) -> String {
    crate::logging::redact(text)
}

pub fn scrub_value(v: &mut Value) {
    match v {
        Value::String(s) => *s = scrub(s),
        Value::Array(a) => a.iter_mut().for_each(scrub_value),
        Value::Object(m) => m.values_mut().for_each(scrub_value),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // De registry is globaal: tests gebruiken unieke waarden en ruimen alleen hun eigen bron op.
    #[test]
    fn replaces_known_values_in_every_common_shape() {
        register("t1", &["Zq9-secret/value+1234".to_string()]);
        assert_eq!(scrub("the value Zq9-secret/value+1234 ok"), "the value •••• ok");
        register("t1b", &["Jq7\"quote\\back-5678".to_string()]);
        assert!(!scrub("{\"k\":\"Jq7\\\"quote\\\\back-5678\"}").contains("Jq7"), "JSON-escaped form");
        unregister("t1b");
        assert!(!scrub("?t=Zq9-secret%2Fvalue%2B1234&x=1").contains("Zq9"), "URL-encoded form");
        unregister("t1");
        assert_eq!(scrub("the value Zq9-secret/value+1234"), "the value Zq9-secret/value+1234");
    }

    #[test]
    fn short_values_and_ordinary_words_are_left_alone() {
        register("t2", &["1234".to_string(), "abc".to_string()]);
        assert_eq!(scrub("code 1234 and abc"), "code 1234 and abc");
        unregister("t2");
    }

    #[test]
    fn overlapping_values_are_replaced_longest_first_and_inside_longer_text() {
        register("t3", &["overlapKEY-12345".to_string(), "overlapKEY-12345-extra-tail".to_string()]);
        assert_eq!(scrub("a overlapKEY-12345-extra-tail b"), "a •••• b");
        assert_eq!(scrub("prefixoverlapKEY-12345suffix"), "prefix••••suffix", "also inside other text");
        assert_eq!(scrub("overlapKEY-12345 and overlapKEY-12345"), "•••• and ••••", "every occurrence");
        unregister("t3");
    }

    #[test]
    fn service_account_json_and_pem_keys_are_scrubbed_line_by_line() {
        let json = br#"{"type":"service_account","client_email":"bot@p.iam.gserviceaccount.com","private_key":"-----BEGIN PRIVATE KEY-----\nMIIEvQIBADANBgkqhkiG9w0BAQEFAASCBKcwggSjAgEAAoIBAQC7\nabcdefghijklmnopqrstuvwxyz0123456789ABCD\n-----END PRIVATE KEY-----\n"}"#;
        register("t4", &values_from_file(json));
        assert!(!scrub("line: MIIEvQIBADANBgkqhkiG9w0BAQEFAASCBKcwggSjAgEAAoIBAQC7").contains("MIIEv"), "one line of the key alone");
        assert!(!scrub("abcdefghijklmnopqrstuvwxyz0123456789ABCD").contains("abcdefgh"));
        assert!(scrub("mail bot@p.iam.gserviceaccount.com").contains("bot@p"), "the client email is not secret");
        unregister("t4");
        let pem = b"-----BEGIN PRIVATE KEY-----\nMIGTAgEAMBMGByqGSM49AgEGCCqGSM49AwEHBHkwdwIBAQQg\n-----END PRIVATE KEY-----\n";
        assert!(values_from_file(pem).iter().any(|v| v == "MIGTAgEAMBMGByqGSM49AgEGCCqGSM49AwEHBHkwdwIBAQQg"));
        assert!(values_from_file(&[0xFE, 0xED, 0xFE, 0xED, 0xFF]).is_empty(), "binary keystores cannot appear as text");
    }

    #[test]
    fn json_values_are_scrubbed_recursively_and_patterns_still_apply() {
        register("t5", &["recursiveSECRET-98765".to_string()]);
        let mut v = serde_json::json!({"a":["x recursiveSECRET-98765"],"b":{"c":"ghp_abcdefghijklmnopqrstuvwxyz0123"},"n":5});
        scrub_value(&mut v);
        let s = v.to_string();
        assert!(!s.contains("recursiveSECRET") && !s.contains("ghp_abc") && s.contains("\"n\":5"), "{s}");
        unregister("t5");
    }
}
