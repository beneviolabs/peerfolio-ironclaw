#![cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]

#[cfg(target_arch = "wasm32")]
wit_bindgen::generate!({
    world: "sandboxed-tool",
    path: "wit/tool.wit",
});

use serde::{Deserialize, Serialize};

const DEFAULT_PEERFOLIO_API_BASE_URL: &str = "https://api.peerfolio.app";
const DISCOVERY_PATH: &str = "/contact-discovery/discover";
const IDENTIFIER_TYPE: &str = "phone_number";
const DEFAULT_TIMEOUT_MS: u64 = 10_000;
const MAX_RESPONSE_BYTES: usize = 64 * 1024;

#[cfg(target_arch = "wasm32")]
struct PeerfolioContactDiscovery;

#[cfg(target_arch = "wasm32")]
impl exports::near::agent::tool::Guest for PeerfolioContactDiscovery {
    fn execute(req: exports::near::agent::tool::Request) -> exports::near::agent::tool::Response {
        match execute_inner(&req.params, &IronclawHttpClient) {
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
        "Lookup Peerfolio strategy previews for one already-tokenized contact identifier. \
         This tool never accepts raw phone numbers, VCF text, or contact files."
            .to_string()
    }
}

#[cfg(target_arch = "wasm32")]
export!(PeerfolioContactDiscovery);

#[derive(Debug, Deserialize, Serialize, PartialEq, Eq)]
struct DiscoverByContactTokenParams {
    viewer_contact_token: String,
    #[serde(default = "default_token_version")]
    token_version: u16,
    #[serde(default = "default_identifier_type")]
    identifier_type: String,
    peerfolio_api_base_url: Option<String>,
}

#[derive(Debug, Serialize)]
struct DiscoverRequestBody<'a> {
    token_version: u16,
    identifier_type: &'a str,
    viewer_contact_token: &'a str,
}

#[derive(Debug, Deserialize, Serialize, PartialEq, Eq)]
struct DiscoverOutput {
    strategies: Vec<StrategyPreview>,
}

#[derive(Debug, Deserialize, Serialize, PartialEq, Eq)]
struct StrategyPreview {
    invite_link: String,
    owner_display_name: String,
    strategy_title: String,
    strategy_summary: String,
}

#[derive(Debug, PartialEq, Eq)]
struct HttpResponse {
    status: u16,
    body: Vec<u8>,
}

trait PeerfolioHttpClient {
    fn post_json(&self, url: &str, body: &str) -> Result<HttpResponse, String>;
}

#[cfg(target_arch = "wasm32")]
struct IronclawHttpClient;

#[cfg(target_arch = "wasm32")]
impl PeerfolioHttpClient for IronclawHttpClient {
    fn post_json(&self, url: &str, body: &str) -> Result<HttpResponse, String> {
        let headers_json = r#"{"content-type":"application/json","accept":"application/json"}"#;
        let response = near::agent::host::http_request(
            "POST",
            url,
            headers_json,
            Some(body.as_bytes()),
            Some(DEFAULT_TIMEOUT_MS),
        )
        .map_err(|error| format!("Peerfolio discovery API request failed: {error}"))?;

        Ok(HttpResponse {
            status: response.status,
            body: response.body,
        })
    }
}

fn default_token_version() -> u16 {
    1
}

fn default_identifier_type() -> String {
    IDENTIFIER_TYPE.to_string()
}

fn execute_inner(params: &str, client: &impl PeerfolioHttpClient) -> Result<String, String> {
    let params: DiscoverByContactTokenParams =
        serde_json::from_str(params).map_err(|error| format!("Invalid parameters: {error}"))?;
    discover_by_contact_token(params, client)
}

fn discover_by_contact_token(
    params: DiscoverByContactTokenParams,
    client: &impl PeerfolioHttpClient,
) -> Result<String, String> {
    validate_params(&params)?;

    let url = discovery_url(params.peerfolio_api_base_url.as_deref())?;
    let body = serde_json::to_string(&DiscoverRequestBody {
        token_version: params.token_version,
        identifier_type: IDENTIFIER_TYPE,
        viewer_contact_token: params.viewer_contact_token.trim(),
    })
    .map_err(|error| format!("Serialization error: {error}"))?;

    let response = client.post_json(&url, &body)?;
    decode_discovery_response(response)
}

fn validate_params(params: &DiscoverByContactTokenParams) -> Result<(), String> {
    if params.token_version == 0 {
        return Err("'token_version' must be at least 1".to_string());
    }

    if params.identifier_type != IDENTIFIER_TYPE {
        return Err("'identifier_type' must be phone_number".to_string());
    }

    if !is_valid_contact_token(&params.viewer_contact_token) {
        return Err(
            "'viewer_contact_token' must be a 64-character lowercase hex string".to_string(),
        );
    }

    Ok(())
}

fn is_valid_contact_token(token: &str) -> bool {
    let token = token.trim();
    token.len() == 64
        && token
            .chars()
            .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
}

fn discovery_url(api_base_url: Option<&str>) -> Result<String, String> {
    let base_url = api_base_url
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(DEFAULT_PEERFOLIO_API_BASE_URL);

    if !(base_url.starts_with("https://") || base_url.starts_with("http://")) {
        return Err("'peerfolio_api_base_url' must start with http:// or https://".to_string());
    }
    if base_url.contains('?') || base_url.contains('#') {
        return Err(
            "'peerfolio_api_base_url' must not include a query string or fragment".to_string(),
        );
    }

    Ok(format!(
        "{}{}",
        base_url.trim_end_matches('/'),
        DISCOVERY_PATH
    ))
}

fn decode_discovery_response(response: HttpResponse) -> Result<String, String> {
    if response.body.len() > MAX_RESPONSE_BYTES {
        return Err("Peerfolio discovery API response was too large".to_string());
    }

    match response.status {
        200 => {
            let output: DiscoverOutput = serde_json::from_slice(&response.body)
                .map_err(|error| format!("Invalid Peerfolio discovery API response: {error}"))?;
            serde_json::to_string(&output).map_err(|error| format!("Serialization error: {error}"))
        }
        400 | 422 => Err("Peerfolio discovery API rejected the contact token".to_string()),
        429 => Err("Peerfolio discovery API rate limited this lookup".to_string()),
        status => Err(format!(
            "Peerfolio discovery API returned unexpected status {status}"
        )),
    }
}

const SCHEMA: &str = r#"{
  "type": "object",
  "description": "Lookup Peerfolio strategy previews for one already-tokenized contact identifier. Does not accept raw phone numbers or contact files.",
  "properties": {
    "viewer_contact_token": {
      "type": "string",
      "description": "64-character lowercase hex HMAC-SHA256 contact token generated by the tokenizer tool.",
      "pattern": "^[0-9a-f]{64}$"
    },
    "token_version": {
      "type": "integer",
      "minimum": 1,
      "default": 1
    },
    "identifier_type": {
      "type": "string",
      "enum": ["phone_number"],
      "default": "phone_number"
    },
    "peerfolio_api_base_url": {
      "type": "string",
      "description": "Optional Peerfolio API base URL for staging or local development. Defaults to https://api.peerfolio.app."
    }
  },
  "required": ["viewer_contact_token"],
  "additionalProperties": false
}"#;

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    const TOKEN: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

    #[derive(Default)]
    struct MockHttpClient {
        requests: RefCell<Vec<(String, String)>>,
        response: RefCell<Option<Result<HttpResponse, String>>>,
    }

    impl MockHttpClient {
        fn with_response(response: HttpResponse) -> Self {
            Self {
                requests: RefCell::new(Vec::new()),
                response: RefCell::new(Some(Ok(response))),
            }
        }

        fn requested_body(&self) -> String {
            self.requests.borrow()[0].1.clone()
        }

        fn requested_url(&self) -> String {
            self.requests.borrow()[0].0.clone()
        }
    }

    impl PeerfolioHttpClient for MockHttpClient {
        fn post_json(&self, url: &str, body: &str) -> Result<HttpResponse, String> {
            self.requests
                .borrow_mut()
                .push((url.to_string(), body.to_string()));
            self.response
                .borrow_mut()
                .take()
                .unwrap_or_else(|| Err("missing mock response".to_string()))
        }
    }

    #[test]
    fn builds_default_discovery_url() {
        assert_eq!(
            discovery_url(None).unwrap(),
            "https://api.peerfolio.app/contact-discovery/discover"
        );
    }

    #[test]
    fn builds_override_discovery_url_without_double_slash() {
        assert_eq!(
            discovery_url(Some("https://staging-api.peerfolio.app/")).unwrap(),
            "https://staging-api.peerfolio.app/contact-discovery/discover"
        );
    }

    #[test]
    fn rejects_invalid_contact_tokens() {
        assert!(!is_valid_contact_token("abc"));
        assert!(!is_valid_contact_token(
            "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"
        ));
        assert!(!is_valid_contact_token(
            "gggggggggggggggggggggggggggggggggggggggggggggggggggggggggggggggg"
        ));
        assert!(is_valid_contact_token(TOKEN));
    }

    #[test]
    fn posts_tokenized_identifier_to_peerfolio() {
        let client = MockHttpClient::with_response(HttpResponse {
            status: 200,
            body: br#"{"strategies":[]}"#.to_vec(),
        });
        let params = DiscoverByContactTokenParams {
            viewer_contact_token: TOKEN.to_string(),
            token_version: 1,
            identifier_type: "phone_number".to_string(),
            peerfolio_api_base_url: None,
        };

        let output = discover_by_contact_token(params, &client).unwrap();

        assert_eq!(output, r#"{"strategies":[]}"#);
        assert_eq!(
            client.requested_url(),
            "https://api.peerfolio.app/contact-discovery/discover"
        );
        assert_eq!(
            client.requested_body(),
            format!(
                r#"{{"token_version":1,"identifier_type":"phone_number","viewer_contact_token":"{TOKEN}"}}"#
            )
        );
    }

    #[test]
    fn returns_safe_strategy_previews() {
        let client = MockHttpClient::with_response(HttpResponse {
            status: 200,
            body: br#"{"strategies":[{"invite_link":"https://peerfolio.app/invite/example","owner_display_name":"Ada","strategy_title":"Long-term crypto mix","strategy_summary":"A concise public summary"}]}"#.to_vec(),
        });
        let params = DiscoverByContactTokenParams {
            viewer_contact_token: TOKEN.to_string(),
            token_version: 1,
            identifier_type: "phone_number".to_string(),
            peerfolio_api_base_url: Some("https://staging-api.peerfolio.app".to_string()),
        };

        let output = discover_by_contact_token(params, &client).unwrap();
        let decoded: DiscoverOutput = serde_json::from_str(&output).unwrap();

        assert_eq!(decoded.strategies.len(), 1);
        assert_eq!(decoded.strategies[0].owner_display_name, "Ada");
        assert_eq!(
            client.requested_url(),
            "https://staging-api.peerfolio.app/contact-discovery/discover"
        );
    }

    #[test]
    fn maps_rate_limit_status_to_tool_error() {
        let response = HttpResponse {
            status: 429,
            body: br#"{"error":"rate_limited"}"#.to_vec(),
        };

        assert_eq!(
            decode_discovery_response(response),
            Err("Peerfolio discovery API rate limited this lookup".to_string())
        );
    }

    #[test]
    fn rejects_unexpected_response_shape() {
        let response = HttpResponse {
            status: 200,
            body: br#"{"owner_user_id":"should-not-be-here"}"#.to_vec(),
        };

        assert!(decode_discovery_response(response)
            .unwrap_err()
            .starts_with("Invalid Peerfolio discovery API response"));
    }
}
