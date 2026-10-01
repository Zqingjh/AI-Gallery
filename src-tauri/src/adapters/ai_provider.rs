//! 可插拔的 AI Provider 适配器。
//!
//! 适配器只处理已由 service 白名单化后的文本和分类体系；不读取工作区、SQLite 或日志。

use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use url::{Host, Url};

use crate::domain::{AiProviderConfig, AiProviderKind, ClassifySuggestion};

const MAX_RESPONSE_BYTES: usize = 512 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ProviderClassificationRequest {
    pub(crate) content: ProviderClassificationContent,
    pub(crate) dimensions: Vec<ProviderDimension>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ProviderClassificationContent {
    pub(crate) title: Option<String>,
    pub(crate) prompt_zh: Option<String>,
    pub(crate) prompt_en: Option<String>,
    pub(crate) negative_prompt: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ProviderDimension {
    pub(crate) id: i64,
    pub(crate) name: String,
    pub(crate) allows_multiple: bool,
    pub(crate) can_suggest_new: bool,
    pub(crate) categories: Vec<ProviderCategory>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ProviderCategory {
    pub(crate) id: i64,
    pub(crate) name: String,
    pub(crate) aliases: Vec<String>,
    pub(crate) description: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AiProviderAdapterError {
    InvalidConfiguration,
    CredentialRequired,
    RequestFailed,
    RequestRejected(u16),
    ResponseInvalid,
}

pub(crate) trait AiProviderAdapter {
    fn test_connection(
        &self,
        config: &AiProviderConfig,
        credential: Option<&str>,
    ) -> Result<(), AiProviderAdapterError>;

    fn classify(
        &self,
        config: &AiProviderConfig,
        credential: Option<&str>,
        request: &ProviderClassificationRequest,
    ) -> Result<Vec<ClassifySuggestion>, AiProviderAdapterError>;
}

#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct ProviderAdapterRegistry;

impl ProviderAdapterRegistry {
    pub(crate) fn test_connection(
        &self,
        config: &AiProviderConfig,
        credential: Option<&str>,
    ) -> Result<(), AiProviderAdapterError> {
        match config.kind {
            AiProviderKind::OpenaiCompatible => {
                OpenaiCompatibleAdapter.test_connection(config, credential)
            }
            AiProviderKind::Gemini => GeminiAdapter.test_connection(config, credential),
            AiProviderKind::Ollama => OllamaAdapter.test_connection(config, credential),
        }
    }

    pub(crate) fn classify(
        &self,
        config: &AiProviderConfig,
        credential: Option<&str>,
        request: &ProviderClassificationRequest,
    ) -> Result<Vec<ClassifySuggestion>, AiProviderAdapterError> {
        match config.kind {
            AiProviderKind::OpenaiCompatible => {
                OpenaiCompatibleAdapter.classify(config, credential, request)
            }
            AiProviderKind::Gemini => GeminiAdapter.classify(config, credential, request),
            AiProviderKind::Ollama => OllamaAdapter.classify(config, credential, request),
        }
    }
}

struct OpenaiCompatibleAdapter;
struct GeminiAdapter;
struct OllamaAdapter;

impl AiProviderAdapter for OpenaiCompatibleAdapter {
    fn test_connection(
        &self,
        config: &AiProviderConfig,
        credential: Option<&str>,
    ) -> Result<(), AiProviderAdapterError> {
        let credential = require_credential(credential)?;
        let response = agent(config)?
            .get(&join_endpoint(&config.endpoint, "models")?)
            .header("Authorization", format!("Bearer {credential}"))
            .call()
            .map_err(|_| AiProviderAdapterError::RequestFailed)?;
        read_small_body(response.into_body()).map(|_| ())
    }

    fn classify(
        &self,
        config: &AiProviderConfig,
        credential: Option<&str>,
        request: &ProviderClassificationRequest,
    ) -> Result<Vec<ClassifySuggestion>, AiProviderAdapterError> {
        let credential = require_credential(credential)?;
        let body = openai_classification_body(config, request)?;
        let value = response_json(
            agent(config)?
                .post(&join_endpoint(&config.endpoint, "chat/completions")?)
                .header("Authorization", format!("Bearer {credential}"))
                .send_json(&body),
        )?;
        let content = value
            .pointer("/choices/0/message/content")
            .ok_or(AiProviderAdapterError::ResponseInvalid)?;
        parse_structured_content(content)
    }
}

impl AiProviderAdapter for GeminiAdapter {
    fn test_connection(
        &self,
        config: &AiProviderConfig,
        credential: Option<&str>,
    ) -> Result<(), AiProviderAdapterError> {
        let credential = require_credential(credential)?;
        let url = format!(
            "{}?key={credential}",
            join_endpoint(&config.endpoint, "models")?
        );
        let response = agent(config)?
            .get(&url)
            .call()
            .map_err(|_| AiProviderAdapterError::RequestFailed)?;
        read_small_body(response.into_body()).map(|_| ())
    }

    fn classify(
        &self,
        config: &AiProviderConfig,
        credential: Option<&str>,
        request: &ProviderClassificationRequest,
    ) -> Result<Vec<ClassifySuggestion>, AiProviderAdapterError> {
        let credential = require_credential(credential)?;
        let url = format!(
            "{}?key={credential}",
            join_endpoint(
                &config.endpoint,
                &format!("models/{}:generateContent", config.model)
            )?
        );
        let body = json!({
            "systemInstruction": { "parts": [{ "text": system_instruction() }] },
            "contents": [{ "role": "user", "parts": [{ "text": serde_json::to_string(request).map_err(|_| AiProviderAdapterError::ResponseInvalid)? }] }],
            "generationConfig": { "responseMimeType": "application/json", "temperature": 0.1 }
        });
        let value = response_json(agent(config)?.post(&url).send_json(&body))?;
        let content = value
            .pointer("/candidates/0/content/parts/0/text")
            .ok_or(AiProviderAdapterError::ResponseInvalid)?;
        parse_structured_content(content)
    }
}

impl AiProviderAdapter for OllamaAdapter {
    fn test_connection(
        &self,
        config: &AiProviderConfig,
        _credential: Option<&str>,
    ) -> Result<(), AiProviderAdapterError> {
        let response = agent(config)?
            .get(&join_endpoint(&config.endpoint, "api/tags")?)
            .call()
            .map_err(|_| AiProviderAdapterError::RequestFailed)?;
        read_small_body(response.into_body()).map(|_| ())
    }

    fn classify(
        &self,
        config: &AiProviderConfig,
        _credential: Option<&str>,
        request: &ProviderClassificationRequest,
    ) -> Result<Vec<ClassifySuggestion>, AiProviderAdapterError> {
        let body = json!({
            "model": config.model,
            "stream": false,
            "format": "json",
            "messages": [
                { "role": "system", "content": system_instruction() },
                { "role": "user", "content": serde_json::to_string(request).map_err(|_| AiProviderAdapterError::ResponseInvalid)? }
            ],
            "options": { "temperature": 0.1 }
        });
        let value = response_json(
            agent(config)?
                .post(&join_endpoint(&config.endpoint, "api/chat")?)
                .send_json(&body),
        )?;
        let content = value
            .pointer("/message/content")
            .ok_or(AiProviderAdapterError::ResponseInvalid)?;
        parse_structured_content(content)
    }
}

/// 组装 OpenAI 兼容的分类请求。官方 DeepSeek V4 默认开启思考模式，
/// 而本功能只需要短 JSON；显式关闭思考并限制输出，避免占满用户配置的请求超时。
fn openai_classification_body(
    config: &AiProviderConfig,
    request: &ProviderClassificationRequest,
) -> Result<Value, AiProviderAdapterError> {
    let content =
        serde_json::to_string(request).map_err(|_| AiProviderAdapterError::ResponseInvalid)?;
    let mut body = json!({
        "model": config.model,
        "messages": [
            { "role": "system", "content": system_instruction() },
            { "role": "user", "content": content }
        ],
        "response_format": { "type": "json_object" },
        "temperature": 0.1
    });
    if is_official_deepseek_endpoint(&config.endpoint) {
        body["thinking"] = json!({ "type": "disabled" });
        body["max_tokens"] = json!(2048);
    }
    Ok(body)
}

fn is_official_deepseek_endpoint(endpoint: &str) -> bool {
    Url::parse(endpoint)
        .ok()
        .and_then(|url| url.host_str().map(str::to_owned))
        .is_some_and(|host| host.eq_ignore_ascii_case("api.deepseek.com"))
}

fn agent(config: &AiProviderConfig) -> Result<ureq::Agent, AiProviderAdapterError> {
    validate_endpoint(config)?;
    let timeout = Duration::from_millis(u64::from(config.timeout_ms));
    Ok(ureq::Agent::config_builder()
        .timeout_global(Some(timeout))
        // Provider 配置可能来自可迁移工作区，禁止重定向以免越过首跳的地址边界。
        .max_redirects(0)
        .build()
        .into())
}

fn validate_endpoint(config: &AiProviderConfig) -> Result<(), AiProviderAdapterError> {
    validate_provider_configuration(config.kind, &config.endpoint, &config.model)
}

pub(crate) fn validate_provider_configuration(
    kind: AiProviderKind,
    endpoint: &str,
    model: &str,
) -> Result<(), AiProviderAdapterError> {
    let endpoint = endpoint.trim();
    if endpoint.is_empty() || endpoint.len() > 2_000 || !valid_model_name(model) {
        return Err(AiProviderAdapterError::InvalidConfiguration);
    }
    let url = Url::parse(endpoint).map_err(|_| AiProviderAdapterError::InvalidConfiguration)?;
    if url.query().is_some()
        || url.fragment().is_some()
        || !url.username().is_empty()
        || url.password().is_some()
    {
        Err(AiProviderAdapterError::InvalidConfiguration)
    } else if matches!(kind, AiProviderKind::Ollama) {
        validate_ollama_endpoint(&url)
    } else {
        validate_remote_endpoint(&url)
    }
}

fn join_endpoint(endpoint: &str, suffix: &str) -> Result<String, AiProviderAdapterError> {
    let suffix = suffix.trim_start_matches('/');
    if suffix.is_empty() || suffix.contains("..") || suffix.contains(['\n', '\r', '?', '#', '\\']) {
        return Err(AiProviderAdapterError::InvalidConfiguration);
    }
    let mut base =
        Url::parse(endpoint).map_err(|_| AiProviderAdapterError::InvalidConfiguration)?;
    if !base.path().ends_with('/') {
        let path = format!("{}/", base.path());
        base.set_path(&path);
    }
    base.join(suffix)
        .map(|url| url.into())
        .map_err(|_| AiProviderAdapterError::InvalidConfiguration)
}

fn require_credential(credential: Option<&str>) -> Result<&str, AiProviderAdapterError> {
    credential
        .filter(|value| !value.trim().is_empty())
        .ok_or(AiProviderAdapterError::CredentialRequired)
}

fn response_json(
    response: Result<ureq::http::Response<ureq::Body>, ureq::Error>,
) -> Result<Value, AiProviderAdapterError> {
    let response = response.map_err(|error| match error {
        ureq::Error::StatusCode(status) => AiProviderAdapterError::RequestRejected(status),
        _ => AiProviderAdapterError::RequestFailed,
    })?;
    let bytes = read_small_body(response.into_body())?;
    serde_json::from_slice(&bytes).map_err(|_| AiProviderAdapterError::ResponseInvalid)
}

fn read_small_body(mut body: ureq::Body) -> Result<Vec<u8>, AiProviderAdapterError> {
    body.with_config()
        .limit(MAX_RESPONSE_BYTES as u64)
        .read_to_vec()
        .map_err(|_| AiProviderAdapterError::ResponseInvalid)
}

fn validate_remote_endpoint(url: &Url) -> Result<(), AiProviderAdapterError> {
    let host = url
        .host()
        .ok_or(AiProviderAdapterError::InvalidConfiguration)?;
    let default_https_port = url.port_or_known_default() == Some(443);
    let public_domain = matches!(host, Host::Domain(domain) if !is_local_domain(domain));
    if url.scheme() == "https" && default_https_port && public_domain {
        Ok(())
    } else {
        Err(AiProviderAdapterError::InvalidConfiguration)
    }
}

fn validate_ollama_endpoint(url: &Url) -> Result<(), AiProviderAdapterError> {
    let host = url
        .host()
        .ok_or(AiProviderAdapterError::InvalidConfiguration)?;
    let is_loopback = match host {
        Host::Ipv4(address) => address.is_loopback(),
        Host::Ipv6(address) => address.is_loopback(),
        Host::Domain(domain) => domain.eq_ignore_ascii_case("localhost"),
    };
    if url.scheme() == "http" && is_loopback {
        Ok(())
    } else {
        Err(AiProviderAdapterError::InvalidConfiguration)
    }
}

fn is_local_domain(domain: &str) -> bool {
    let domain = domain.trim_end_matches('.').to_ascii_lowercase();
    domain == "localhost" || domain.ends_with(".localhost") || domain.ends_with(".local")
}

fn valid_model_name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 200
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':'))
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct StructuredSuggestions {
    suggestions: Vec<ClassifySuggestion>,
}

fn parse_structured_content(
    content: &Value,
) -> Result<Vec<ClassifySuggestion>, AiProviderAdapterError> {
    let value = match content {
        Value::String(value) => serde_json::from_str(strip_json_code_fence(value)),
        Value::Object(_) => Ok(content.clone()),
        _ => Err(serde_json::Error::io(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "非结构化响应",
        ))),
    }
    .map_err(|_| AiProviderAdapterError::ResponseInvalid)?;
    let response: StructuredSuggestions =
        serde_json::from_value(value).map_err(|_| AiProviderAdapterError::ResponseInvalid)?;
    if response.suggestions.len() > 20 {
        return Err(AiProviderAdapterError::ResponseInvalid);
    }
    for suggestion in &response.suggestions {
        let has_target = suggestion.category_id.is_some()
            || suggestion
                .suggested_category_name
                .as_ref()
                .is_some_and(|name| !name.trim().is_empty());
        if !has_target
            || suggestion.dimension_id <= 0
            || suggestion
                .confidence
                .is_some_and(|value| !value.is_finite() || !(0.0..=1.0).contains(&value))
            || suggestion.reason.chars().count() > 500
            || suggestion
                .suggested_category_name
                .as_ref()
                .is_some_and(|name| name.chars().count() > 200)
        {
            return Err(AiProviderAdapterError::ResponseInvalid);
        }
    }
    Ok(response.suggestions)
}

/// 部分兼容模型会在 JSON 外包一层 Markdown 代码块；只去除完整包裹的围栏，
/// 后续仍使用严格的 JSON 与建议字段校验。
fn strip_json_code_fence(value: &str) -> &str {
    let value = value.trim();
    let Some(value) = value.strip_prefix("```json") else {
        return value;
    };
    let Some(value) = value.strip_suffix("```") else {
        return value;
    };
    value.trim()
}

fn system_instruction() -> &'static str {
    "你是分类建议器。只输出 JSON：{\"suggestions\":[{\"dimensionId\":数字,\"categoryId\":数字或null,\"suggestedCategoryName\":字符串或null,\"confidence\":0到1,\"reason\":不超过500字}]}; 优先使用给定 categoryId；只在允许新增的维度提出新名称。"
}

#[cfg(test)]
mod tests {
    use std::{
        io::{Read, Write},
        net::TcpListener,
        thread,
    };

    use serde_json::json;

    use super::{
        AiProviderAdapterError, AiProviderConfig, AiProviderKind, MAX_RESPONSE_BYTES,
        ProviderClassificationRequest, openai_classification_body, parse_structured_content,
        read_small_body, validate_endpoint,
    };
    use crate::domain::AiProviderCapabilities;

    fn provider(kind: AiProviderKind, endpoint: &str) -> AiProviderConfig {
        AiProviderConfig {
            id: 1,
            kind,
            display_name: "测试".to_owned(),
            endpoint: endpoint.to_owned(),
            model: "model-1".to_owned(),
            capabilities: AiProviderCapabilities {
                classification: true,
            },
            timeout_ms: 1_000,
            credential_id: None,
            is_enabled: true,
            created_at: 1,
            updated_at: 1,
        }
    }

    #[test]
    fn parses_only_limited_structured_suggestions() {
        let result = parse_structured_content(&json!(
            "{\"suggestions\":[{\"dimensionId\":1,\"categoryId\":2,\"suggestedCategoryName\":null,\"confidence\":0.8,\"reason\":\"匹配\"}]}"
        ));
        assert!(result.is_ok());
    }

    #[test]
    fn accepts_a_json_response_wrapped_by_a_markdown_code_fence() {
        let result = parse_structured_content(&json!(
            "```json\n{\"suggestions\":[{\"dimensionId\":1,\"categoryId\":2,\"suggestedCategoryName\":null,\"confidence\":0.8,\"reason\":\"匹配\"}]}\n```"
        ));
        assert!(result.is_ok());
    }

    #[test]
    fn deepseek_classification_disables_thinking_and_bounds_json_output() {
        let config = provider(AiProviderKind::OpenaiCompatible, "https://api.deepseek.com");
        let body = openai_classification_body(
            &config,
            &ProviderClassificationRequest {
                content: Default::default(),
                dimensions: vec![],
            },
        )
        .expect("应能组装分类请求");

        assert_eq!(body["thinking"]["type"], "disabled");
        assert_eq!(body["max_tokens"], 2048);
        assert!(
            body["messages"][1]["content"].is_string(),
            "OpenAI 兼容接口的消息 content 必须是字符串"
        );
    }

    #[test]
    fn rejects_unexpected_response_fields() {
        let result =
            parse_structured_content(&json!("{\"suggestions\":[],\"rawPrompt\":\"不应接受\"}"));
        assert_eq!(result, Err(AiProviderAdapterError::ResponseInvalid));
    }

    #[test]
    fn remote_provider_rejects_private_hosts_credentials_queries_and_non_standard_ports() {
        for endpoint in [
            "https://127.0.0.1",
            "https://[::1]",
            "https://user@example.com",
            "https://example.com?key=value",
            "https://example.com:8443",
            "http://example.com",
            "https://example.local",
        ] {
            assert_eq!(
                validate_endpoint(&provider(AiProviderKind::OpenaiCompatible, endpoint)),
                Err(AiProviderAdapterError::InvalidConfiguration),
                "应拒绝 {endpoint}"
            );
        }
        assert!(
            validate_endpoint(&provider(
                AiProviderKind::OpenaiCompatible,
                "https://api.example.com/v1"
            ))
            .is_ok()
        );
    }

    #[test]
    fn ollama_accepts_only_http_loopback() {
        assert!(
            validate_endpoint(&provider(AiProviderKind::Ollama, "http://127.0.0.1:11434")).is_ok()
        );
        assert!(
            validate_endpoint(&provider(AiProviderKind::Ollama, "http://localhost:11434")).is_ok()
        );
        assert_eq!(
            validate_endpoint(&provider(
                AiProviderKind::Ollama,
                "http://192.168.1.2:11434"
            )),
            Err(AiProviderAdapterError::InvalidConfiguration)
        );
    }

    #[test]
    fn rejects_response_larger_than_limit_while_reading() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("应创建本地测试服务");
        let address = listener.local_addr().expect("应读取本地测试地址");
        let body = vec![b'x'; MAX_RESPONSE_BYTES + 1];
        let worker = thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("应接收测试请求");
            let mut request = [0_u8; 1_024];
            let _ = stream.read(&mut request).expect("应读取测试请求");
            let header = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            stream.write_all(header.as_bytes()).expect("应写入响应头");
            stream.write_all(&body).expect("应写入受限响应体");
        });

        let response = ureq::get(&format!("http://{address}"))
            .call()
            .expect("应获得测试响应");
        assert_eq!(
            read_small_body(response.into_body()),
            Err(AiProviderAdapterError::ResponseInvalid)
        );
        worker.join().expect("测试服务应退出");
    }
}
