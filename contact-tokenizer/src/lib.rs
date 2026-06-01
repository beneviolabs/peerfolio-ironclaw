#![cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]

#[cfg(target_arch = "wasm32")]
wit_bindgen::generate!({
    world: "sandboxed-tool",
    path: "wit/tool.wit",
});

use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use std::collections::BTreeSet;

type HmacSha256 = Hmac<Sha256>;

const IDENTIFIER_TYPE: &str = "phone_number";
const MIN_E164_DIGITS: usize = 8;
const MAX_E164_DIGITS: usize = 15;
const DEFAULT_COUNTRY_CODE: &str = "1";
const MAX_VCF_TEXT_BYTES: usize = 10 * 1024 * 1024;

#[cfg(target_arch = "wasm32")]
struct PeerfolioContactTokenizer;

#[cfg(target_arch = "wasm32")]
impl exports::near::agent::tool::Guest for PeerfolioContactTokenizer {
    fn execute(req: exports::near::agent::tool::Request) -> exports::near::agent::tool::Response {
        match execute_inner(&req.params) {
            Ok(result) => exports::near::agent::tool::Response {
                output: Some(result),
                error: None,
            },
            Err(error) => exports::near::agent::tool::Response {
                output: None,
                error: Some(error),
            },
        }
    }

    fn schema() -> String {
        SCHEMA.to_string()
    }

    fn description() -> String {
        "Extract phone numbers from pasted VCF text, including multi-contact iOS exports, \
         and return Peerfolio contact tokens. The tool ignores non-phone contact fields \
         and does not call Peerfolio APIs."
            .to_string()
    }
}

#[cfg(target_arch = "wasm32")]
export!(PeerfolioContactTokenizer);

#[derive(Debug, Deserialize)]
struct TokenizeVcfContactsParams {
    vcf_text: String,
    #[serde(default = "default_token_version")]
    token_version: u16,
    #[serde(default = "default_country_code")]
    default_country_code: String,
}

#[derive(Debug, Deserialize, Serialize, PartialEq, Eq)]
struct TokenizeVcfContactsOutput {
    token_version: u16,
    identifier_type: String,
    contact_tokens: Vec<String>,
    extracted_phone_count: usize,
    normalized_phone_count: usize,
    deduplicated_count: usize,
    invalid_phone_count: usize,
}

fn default_token_version() -> u16 {
    1
}

fn default_country_code() -> String {
    DEFAULT_COUNTRY_CODE.to_string()
}

fn execute_inner(params: &str) -> Result<String, String> {
    let params: TokenizeVcfContactsParams =
        serde_json::from_str(params).map_err(|error| format!("Invalid parameters: {error}"))?;
    let secret = contact_graph_secret()?;
    tokenize_vcf_contacts(params, secret)
}

fn contact_graph_secret() -> Result<&'static str, String> {
    option_env!("PEERFOLIO_CONTACT_GRAPH_SECRET").ok_or_else(|| {
        "Missing build-time PEERFOLIO_CONTACT_GRAPH_SECRET for contact tokenization".to_string()
    })
}

fn tokenize_vcf_contacts(
    params: TokenizeVcfContactsParams,
    contact_graph_secret: &str,
) -> Result<String, String> {
    if params.vcf_text.trim().is_empty() {
        return Err("'vcf_text' must not be empty".to_string());
    }
    if params.vcf_text.len() > MAX_VCF_TEXT_BYTES {
        return Err("'vcf_text' must be 10MB or smaller".to_string());
    }
    if contact_graph_secret.is_empty() {
        return Err("contact graph secret must not be empty".to_string());
    }
    if !params
        .default_country_code
        .chars()
        .all(|c| c.is_ascii_digit())
    {
        return Err("'default_country_code' must contain digits only".to_string());
    }

    let extracted = extract_tel_values(&params.vcf_text);
    let mut normalized = Vec::new();
    let mut invalid_phone_count = 0;

    for phone in &extracted {
        match normalize_phone_number(phone, &params.default_country_code) {
            Some(value) => normalized.push(value),
            None => invalid_phone_count += 1,
        }
    }

    let unique_normalized: BTreeSet<String> = normalized.iter().cloned().collect();
    let mut contact_tokens = Vec::with_capacity(unique_normalized.len());
    for phone in unique_normalized {
        contact_tokens.push(contact_token(contact_graph_secret, &phone)?);
    }

    let output = TokenizeVcfContactsOutput {
        token_version: params.token_version,
        identifier_type: IDENTIFIER_TYPE.to_string(),
        contact_tokens,
        extracted_phone_count: extracted.len(),
        normalized_phone_count: normalized.len(),
        deduplicated_count: normalized
            .len()
            .saturating_sub(unique_normalized_len(&normalized)),
        invalid_phone_count,
    };

    serde_json::to_string(&output).map_err(|error| format!("Serialization error: {error}"))
}

fn unique_normalized_len(normalized: &[String]) -> usize {
    normalized.iter().collect::<BTreeSet<_>>().len()
}

fn extract_tel_values(vcf_text: &str) -> Vec<String> {
    unfold_vcf_lines(vcf_text)
        .into_iter()
        .filter_map(|line| {
            let (name, value) = line.split_once(':')?;
            let property_name = name.rsplit('.').next().unwrap_or(name);
            let property_name = property_name.split(';').next().unwrap_or(property_name);
            if property_name.eq_ignore_ascii_case("TEL") {
                Some(unescape_vcf_value(value.trim()))
            } else {
                None
            }
        })
        .filter(|value| !value.trim().is_empty())
        .collect()
}

fn unfold_vcf_lines(vcf_text: &str) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();

    for raw_line in vcf_text.replace("\r\n", "\n").replace('\r', "\n").lines() {
        if raw_line.starts_with(' ') || raw_line.starts_with('\t') {
            if let Some(last) = lines.last_mut() {
                last.push_str(raw_line.trim_start());
            }
        } else {
            lines.push(raw_line.to_string());
        }
    }

    lines
}

fn unescape_vcf_value(value: &str) -> String {
    value
        .replace("\\n", "")
        .replace("\\N", "")
        .replace("\\;", ";")
        .replace("\\,", ",")
        .replace("\\\\", "\\")
}

fn normalize_phone_number(phone: &str, default_country_code: &str) -> Option<String> {
    let phone_without_extension = strip_extension(phone);
    let trimmed = phone_without_extension.trim();

    if trimmed.starts_with('+') {
        let digits = digits_only(trimmed);
        return e164_from_digits(&digits);
    }

    let mut digits = digits_only(trimmed);
    if digits.starts_with("011") {
        digits = digits[3..].to_string();
        return e164_from_digits(&digits);
    }

    if default_country_code == "1" && digits.len() == 11 && digits.starts_with('1') {
        if !is_valid_nanp_national_number(&digits[1..]) {
            return None;
        }
        return e164_from_digits(&digits);
    }

    if digits.len() == 10 {
        if default_country_code == "1" && !is_valid_nanp_national_number(&digits) {
            return None;
        }
        digits = format!("{default_country_code}{digits}");
    }

    e164_from_digits(&digits)
}

fn strip_extension(phone: &str) -> &str {
    let lower = phone.to_ascii_lowercase();
    for marker in [" ext.", " ext ", " x", ";ext=", "#"] {
        if let Some(index) = lower.find(marker) {
            return &phone[..index];
        }
    }
    phone
}

fn digits_only(value: &str) -> String {
    value.chars().filter(|c| c.is_ascii_digit()).collect()
}

fn is_valid_nanp_national_number(digits: &str) -> bool {
    let bytes = digits.as_bytes();
    digits.len() == 10 && (b'2'..=b'9').contains(&bytes[0]) && (b'2'..=b'9').contains(&bytes[3])
}

fn e164_from_digits(digits: &str) -> Option<String> {
    if digits.len() < MIN_E164_DIGITS
        || digits.len() > MAX_E164_DIGITS
        || digits.starts_with('0')
        || !digits.chars().all(|c| c.is_ascii_digit())
    {
        return None;
    }

    Some(format!("+{digits}"))
}

fn contact_token(secret: &str, normalized_phone_number: &str) -> Result<String, String> {
    let mut mac = HmacSha256::new_from_slice(secret.as_bytes())
        .map_err(|error| format!("Invalid contact graph secret: {error}"))?;
    mac.update(normalized_phone_number.as_bytes());
    Ok(hex_encode(&mac.finalize().into_bytes()))
}

fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);

    for byte in bytes {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }

    output
}

const SCHEMA: &str = r#"{
  "type": "object",
  "description": "Extract phone numbers from pasted VCF text, including multi-contact iOS exports, and return Peerfolio contact tokens. Does not upload to Peerfolio.",
  "properties": {
    "vcf_text": {
      "type": "string",
      "description": "The pasted text contents of a .vcf contacts export. May contain many BEGIN:VCARD blocks. Maximum size: 10MB."
    },
    "token_version": {
      "type": "integer",
      "minimum": 1,
      "default": 1
    },
    "default_country_code": {
      "type": "string",
      "description": "Digits-only country code used for local 10-digit phone numbers.",
      "default": "1"
    }
  },
  "required": ["vcf_text"],
  "additionalProperties": false
}"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_tel_values_from_ios_vcf() {
        let vcf = "BEGIN:VCARD\r\nVERSION:3.0\r\nFN:Jane Example\r\nitem1.TEL;type=CELL;type=VOICE:(415) 555-0100\r\nEMAIL:jane@example.com\r\nEND:VCARD\r\n";

        assert_eq!(extract_tel_values(vcf), vec!["(415) 555-0100"]);
    }

    #[test]
    fn unfolds_wrapped_tel_values() {
        let vcf = "BEGIN:VCARD\nTEL;TYPE=CELL:+1415555\n 0100\nEND:VCARD";

        assert_eq!(extract_tel_values(vcf), vec!["+14155550100"]);
    }

    #[test]
    fn normalizes_us_phone_numbers_to_e164() {
        assert_eq!(
            normalize_phone_number("(415) 555-0100", "1"),
            Some("+14155550100".to_string())
        );
        assert_eq!(
            normalize_phone_number("1-415-555-0100", "1"),
            Some("+14155550100".to_string())
        );
        assert_eq!(
            normalize_phone_number("+44 20 7946 0958", "1"),
            Some("+442079460958".to_string())
        );
        assert_eq!(
            normalize_phone_number("011 44 20 7946 0958", "1"),
            Some("+442079460958".to_string())
        );
    }

    #[test]
    fn strips_common_extensions() {
        assert_eq!(
            normalize_phone_number("(415) 555-0100 ext. 12", "1"),
            Some("+14155550100".to_string())
        );
        assert_eq!(
            normalize_phone_number("(415) 555-0100;ext=12", "1"),
            Some("+14155550100".to_string())
        );
    }

    #[test]
    fn rejects_invalid_phone_numbers() {
        assert_eq!(normalize_phone_number("555", "1"), None);
        assert_eq!(normalize_phone_number("0000000000", "1"), None);
        assert_eq!(normalize_phone_number("+0000000000", "1"), None);
        assert_eq!(normalize_phone_number("+1234567890123456", "1"), None);
    }

    #[test]
    fn tokenizes_and_deduplicates_vcf_contacts() {
        let params = TokenizeVcfContactsParams {
            vcf_text: "BEGIN:VCARD\nTEL:(415) 555-0100\nTEL:+1 415 555 0100\nTEL:000\nEND:VCARD"
                .to_string(),
            token_version: 1,
            default_country_code: "1".to_string(),
        };

        let output = tokenize_vcf_contacts(params, "secret").unwrap();
        let decoded: TokenizeVcfContactsOutput = serde_json::from_str(&output).unwrap();

        assert_eq!(decoded.identifier_type, "phone_number");
        assert_eq!(decoded.extracted_phone_count, 3);
        assert_eq!(decoded.normalized_phone_count, 2);
        assert_eq!(decoded.deduplicated_count, 1);
        assert_eq!(decoded.invalid_phone_count, 1);
        assert_eq!(decoded.contact_tokens.len(), 1);
        assert_eq!(decoded.contact_tokens[0].len(), 64);
    }

    #[test]
    fn tokenizes_multi_contact_ios_vcf_text() {
        let params = TokenizeVcfContactsParams {
            vcf_text: "BEGIN:VCARD\nVERSION:3.0\nFN:411 & More\nTEL;type=CELL;type=VOICE;type=pref:411\nEND:VCARD\nBEGIN:VCARD\nVERSION:3.0\nFN:Gary Lidgren\nTEL;type=CELL;type=VOICE;type=pref:+16468238675\nEND:VCARD\nBEGIN:VCARD\nVERSION:3.0\nFN:Nicole Riley\nTEL;type=CELL;type=VOICE;type=pref:(425) 761-4893\nTEL;type=CELL;type=VOICE:4257614893\nEND:VCARD\nBEGIN:VCARD\nVERSION:3.0\nFN:Kevin Hoyos\nEMAIL;type=INTERNET;type=pref:KHOYO001@FIU.EDU\nTEL;type=CELL;type=VOICE;type=pref:+1 305-733-0868\nEND:VCARD".to_string(),
            token_version: 1,
            default_country_code: "1".to_string(),
        };

        let output = tokenize_vcf_contacts(params, "secret").unwrap();
        let decoded: TokenizeVcfContactsOutput = serde_json::from_str(&output).unwrap();

        assert_eq!(decoded.extracted_phone_count, 5);
        assert_eq!(decoded.normalized_phone_count, 4);
        assert_eq!(decoded.deduplicated_count, 1);
        assert_eq!(decoded.invalid_phone_count, 1);
        assert_eq!(decoded.contact_tokens.len(), 3);
    }

    #[test]
    fn rejects_vcf_text_larger_than_10mb() {
        let params = TokenizeVcfContactsParams {
            vcf_text: "x".repeat(MAX_VCF_TEXT_BYTES + 1),
            token_version: 1,
            default_country_code: "1".to_string(),
        };

        assert_eq!(
            tokenize_vcf_contacts(params, "secret"),
            Err("'vcf_text' must be 10MB or smaller".to_string())
        );
    }
}
