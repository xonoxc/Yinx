use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use crossterm::event::{KeyCode, KeyModifiers};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{
        Block, Borders, Cell, Clear, List, ListItem, ListState, Paragraph, Row, Table, Tabs, Wrap,
    },
    Frame,
};
use serde::{Deserialize, Serialize};

use yinx_core::request::{
    Header, Headers, Method, Request, RequestBody, RequestBuilder, RequestError, RequestUrl,
};

use crate::editor::{EditorError, EditorFormat};
use crate::input::InputBuffer;
use crate::theme::Theme;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RequestTab {
    Headers,
    Body,
    Auth,
    Params,
}

impl RequestTab {
    pub fn all() -> Vec<RequestTab> {
        vec![
            RequestTab::Headers,
            RequestTab::Body,
            RequestTab::Auth,
            RequestTab::Params,
        ]
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            RequestTab::Headers => "Headers",
            RequestTab::Body => "Body",
            RequestTab::Auth => "Auth",
            RequestTab::Params => "Params",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FocusedField {
    Method,
    Url,
    Tabs,
    TabContent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BodyType {
    Raw,
    Json,
    Form,
}

impl BodyType {
    pub fn as_str(&self) -> &'static str {
        match self {
            BodyType::Raw => "Raw",
            BodyType::Json => "JSON",
            BodyType::Form => "Form",
        }
    }

    pub fn all() -> Vec<BodyType> {
        vec![BodyType::Raw, BodyType::Json, BodyType::Form]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthType {
    None,
    Basic,
    Bearer,
    ApiKey,
}

impl AuthType {
    pub fn as_str(&self) -> &'static str {
        match self {
            AuthType::None => "None",
            AuthType::Basic => "Basic",
            AuthType::Bearer => "Bearer",
            AuthType::ApiKey => "API Key",
        }
    }

    pub fn all() -> Vec<AuthType> {
        vec![
            AuthType::None,
            AuthType::Basic,
            AuthType::Bearer,
            AuthType::ApiKey,
        ]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditableField {
    Url,
    Body,
    Headers,
    HeaderName(usize),
    HeaderValue(usize),
    AuthType,
    AuthUsername,
    AuthPassword,
    AuthToken,
    AuthKeyName,
    AuthKeyValue,
    ParamKey(usize),
    ParamValue(usize),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequestPaneEditSpec {
    pub field: EditableField,
    pub format: EditorFormat,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct SerializableHeader {
    name: String,
    value: String,
}

pub struct RequestPane {
    method: Method,
    url_buffer: InputBuffer,
    url_scroll_offset: usize,
    selected_tab: usize,
    headers: Vec<(InputBuffer, InputBuffer)>,
    header_selected: usize,
    header_field_focus: HeaderField,
    body_content: InputBuffer,
    body_type: BodyType,
    body_type_selected: usize,
    auth_type: AuthType,
    auth_username: InputBuffer,
    auth_password: InputBuffer,
    auth_token: InputBuffer,
    auth_key_name: InputBuffer,
    auth_key_value: InputBuffer,
    auth_field_focus: AuthField,
    params: Vec<(InputBuffer, InputBuffer)>,
    param_selected: usize,
    param_field_focus: ParamField,
    focused_field: FocusedField,
    method_popup_visible: bool,
    method_list_state: ListState,
    url_history: Vec<String>,
    url_autocomplete_visible: bool,
    url_autocomplete_selected: usize,
    search_visible: bool,
    compact: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HeaderField {
    Name,
    Value,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AuthField {
    Type,
    Username,
    Password,
    Token,
    KeyName,
    KeyValue,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ParamField {
    Key,
    Value,
}

impl RequestPane {
    pub fn new() -> Self {
        let mut method_list_state = ListState::default();
        method_list_state.select(Some(0));

        Self {
            method: Method::Get,
            url_buffer: InputBuffer::new(),
            url_scroll_offset: 0,
            selected_tab: 0,
            headers: vec![(InputBuffer::new(), InputBuffer::new())],
            header_selected: 0,
            header_field_focus: HeaderField::Name,
            body_content: InputBuffer::new(),
            body_type: BodyType::Raw,
            body_type_selected: 0,
            auth_type: AuthType::None,
            auth_username: InputBuffer::new(),
            auth_password: InputBuffer::new(),
            auth_token: InputBuffer::new(),
            auth_key_name: InputBuffer::new(),
            auth_key_value: InputBuffer::new(),
            auth_field_focus: AuthField::Type,
            params: vec![(InputBuffer::new(), InputBuffer::new())],
            param_selected: 0,
            param_field_focus: ParamField::Key,
            focused_field: FocusedField::Url,
            method_popup_visible: false,
            method_list_state,
            url_history: Vec::new(),
            url_autocomplete_visible: false,
            url_autocomplete_selected: 0,
            search_visible: false,
            compact: false,
        }
    }

    pub fn with_method(mut self, method: Method) -> Self {
        self.method = method;
        self
    }

    pub fn with_url(mut self, url: &str) -> Self {
        self.url_buffer = InputBuffer::with_content(url);
        self
    }

    pub fn with_headers(mut self, headers: Headers) -> Self {
        self.headers.clear();
        for header in headers.iter() {
            self.headers.push((
                InputBuffer::with_content(&header.name),
                InputBuffer::with_content(&header.value),
            ));
        }
        if self.headers.is_empty() {
            self.headers.push((InputBuffer::new(), InputBuffer::new()));
        }
        self
    }

    pub fn with_body(mut self, body: RequestBody) -> Self {
        match &body {
            RequestBody::Raw(s) => {
                self.body_type = BodyType::Raw;
                self.body_content = InputBuffer::with_content(s);
            }
            RequestBody::Json(v) => {
                self.body_type = BodyType::Json;
                self.body_content =
                    InputBuffer::with_content(&serde_json::to_string_pretty(v).unwrap_or_default());
            }
            RequestBody::Form(_pairs) => {
                self.body_type = BodyType::Form;
                self.body_content = InputBuffer::new();
            }
            _ => {
                self.body_type = BodyType::Raw;
                self.body_content = InputBuffer::new();
            }
        }
        self
    }

    pub fn with_url_history(mut self, history: Vec<String>) -> Self {
        self.url_history = history;
        self
    }

    pub fn set_request(&mut self, request: yinx_core::request::Request) {
        self.method = request.method;
        self.url_buffer = InputBuffer::with_content(request.url.as_str());
        self.headers.clear();
        for h in request.headers.iter() {
            self.headers.push((
                InputBuffer::with_content(&h.name),
                InputBuffer::with_content(&h.value),
            ));
        }
        if self.headers.is_empty() {
            self.headers.push((InputBuffer::new(), InputBuffer::new()));
        }
        match &request.body {
            RequestBody::Raw(s) => {
                self.body_type = BodyType::Raw;
                self.body_content = InputBuffer::with_content(s);
            }
            RequestBody::Json(v) => {
                self.body_type = BodyType::Json;
                self.body_content =
                    InputBuffer::with_content(&serde_json::to_string_pretty(v).unwrap_or_default());
            }
            RequestBody::Form(_) => {
                self.body_type = BodyType::Form;
                self.body_content = InputBuffer::new();
            }
            RequestBody::Multipart(_) => {
                self.body_type = BodyType::Form;
                self.body_content = InputBuffer::new();
            }
            RequestBody::Binary(data) => {
                self.body_type = BodyType::Raw;
                self.body_content = InputBuffer::with_content(&String::from_utf8_lossy(data));
            }
            RequestBody::None => {
                self.body_type = BodyType::Raw;
                self.body_content = InputBuffer::new();
            }
        }
    }

    pub fn method(&self) -> Method {
        self.method
    }

    pub fn url(&self) -> String {
        self.url_buffer.as_str().to_string()
    }

    fn populated_headers(&self) -> usize {
        self.headers
            .iter()
            .filter(|(name, value)| !name.as_str().is_empty() || !value.as_str().is_empty())
            .count()
    }

    fn populated_params(&self) -> usize {
        self.params
            .iter()
            .filter(|(name, value)| !name.as_str().is_empty() || !value.as_str().is_empty())
            .count()
    }

    fn tab_titles(&self) -> Vec<String> {
        vec![
            format!("HEADERS {}", self.populated_headers()),
            format!("BODY {}", self.body_type.as_str()),
            format!("AUTH {}", self.auth_type.as_str().to_uppercase()),
            format!("PARAMS {}", self.populated_params()),
        ]
    }

    pub fn headers(&self) -> Headers {
        let mut headers = Headers::new();
        for (name_buf, value_buf) in &self.headers {
            let name = name_buf.as_str().trim();
            let value = value_buf.as_str().trim();
            if !name.is_empty() {
                let _ = headers.set(name, value);
            }
        }
        headers
    }

    pub fn body(&self) -> RequestBody {
        match self.body_type {
            BodyType::Raw => RequestBody::Raw(self.body_content.as_str().to_string()),
            BodyType::Json => serde_json::from_str(self.body_content.as_str())
                .map(RequestBody::Json)
                .unwrap_or(RequestBody::Raw(self.body_content.as_str().to_string())),
            BodyType::Form => RequestBody::Form(
                self.params
                    .iter()
                    .filter_map(|(k, v)| {
                        let key = k.as_str().trim();
                        if key.is_empty() {
                            None
                        } else {
                            Some((key.to_string(), v.as_str().to_string()))
                        }
                    })
                    .collect(),
            ),
        }
    }

    pub fn to_request(&self, timeout_secs: u64) -> Result<Request, RequestError> {
        let url = self.url_with_query_params();
        let body = self.effective_body();
        let mut headers = self.headers();

        match self.auth_type {
            AuthType::None => {}
            AuthType::Basic => {
                let username = self.auth_username.as_str().trim();
                let password = self.auth_password.as_str().trim();
                if !username.is_empty() || !password.is_empty() {
                    let encoded = BASE64.encode(format!("{username}:{password}"));
                    headers.set("Authorization", &format!("Basic {encoded}"))?;
                }
            }
            AuthType::Bearer => {
                let token = self.auth_token.as_str().trim();
                if !token.is_empty() {
                    headers.set("Authorization", &format!("Bearer {token}"))?;
                }
            }
            AuthType::ApiKey => {
                let name = self.auth_key_name.as_str().trim();
                let value = self.auth_key_value.as_str().trim();
                if !name.is_empty() {
                    headers.set(name, value)?;
                }
            }
        }

        if !body.is_empty() && !headers.contains("content-type") {
            if let Some(content_type) = body.content_type() {
                headers.set("Content-Type", content_type)?;
            }
        }

        RequestBuilder::new()
            .method(self.method)
            .url(url)
            .headers(headers)
            .body(body)
            .timeout_secs(timeout_secs)
            .build()
    }

    pub fn set_compact(&mut self, compact: bool) {
        self.compact = compact;
    }

    pub fn is_compact(&self) -> bool {
        self.compact
    }

    pub fn url_scroll_offset(&self) -> usize {
        self.url_scroll_offset
    }

    pub fn update_url_scroll(&mut self, viewport_width: usize) {
        let cursor = self.url_buffer.cursor_pos;
        let url_len = self.url_buffer.len();

        if cursor < self.url_scroll_offset {
            self.url_scroll_offset = cursor;
        } else {
            let right_edge = self.url_scroll_offset + viewport_width;
            if cursor > right_edge && viewport_width > 0 {
                self.url_scroll_offset = cursor.saturating_sub(viewport_width.saturating_sub(5));
            }
        }

        if url_len <= viewport_width {
            self.url_scroll_offset = 0;
        }

        self.url_scroll_offset = self
            .url_scroll_offset
            .min(url_len.saturating_sub(viewport_width));
    }

    pub fn set_focused_field(&mut self, field: FocusedField) {
        self.focused_field = field;
    }

    pub fn focused_field(&self) -> FocusedField {
        self.focused_field
    }

    pub fn paste_text(&mut self, text: &str) -> bool {
        if text.is_empty() {
            return false;
        }

        match self.focused_editable_field() {
            EditableField::Url => {
                self.url_buffer.insert_str(&normalize_single_line(text));
                self.url_autocomplete_visible = !self.url_buffer.as_str().is_empty();
                self.url_autocomplete_selected = 0;
                true
            }
            EditableField::Body => {
                self.body_content.insert_str(text);
                true
            }
            EditableField::HeaderName(index) => self
                .headers
                .get_mut(index)
                .map(|(name, _)| name.insert_str(&normalize_single_line(text)))
                .is_some(),
            EditableField::HeaderValue(index) => self
                .headers
                .get_mut(index)
                .map(|(_, value)| value.insert_str(&normalize_single_line(text)))
                .is_some(),
            EditableField::AuthUsername => {
                self.auth_username.insert_str(&normalize_single_line(text));
                true
            }
            EditableField::AuthPassword => {
                self.auth_password.insert_str(&normalize_single_line(text));
                true
            }
            EditableField::AuthToken => {
                self.auth_token.insert_str(&normalize_single_line(text));
                true
            }
            EditableField::AuthKeyName => {
                self.auth_key_name.insert_str(&normalize_single_line(text));
                true
            }
            EditableField::AuthKeyValue => {
                self.auth_key_value.insert_str(&normalize_single_line(text));
                true
            }
            EditableField::ParamKey(index) => self
                .params
                .get_mut(index)
                .map(|(key, _)| key.insert_str(&normalize_single_line(text)))
                .is_some(),
            EditableField::ParamValue(index) => self
                .params
                .get_mut(index)
                .map(|(_, value)| value.insert_str(&normalize_single_line(text)))
                .is_some(),
            EditableField::Headers | EditableField::AuthType => false,
        }
    }

    fn url_with_query_params(&self) -> String {
        let url = self.url();
        if url.trim().is_empty() || self.body_type == BodyType::Form {
            return url;
        }

        let params: Vec<(String, String)> = self
            .params
            .iter()
            .filter_map(|(key, value)| {
                let key = key.as_str().trim();
                if key.is_empty() {
                    None
                } else {
                    Some((key.to_string(), value.as_str().to_string()))
                }
            })
            .collect();

        if params.is_empty() {
            return url;
        }

        match url::Url::parse(&url) {
            Ok(mut parsed) => {
                {
                    let mut query = parsed.query_pairs_mut();
                    for (key, value) in params {
                        query.append_pair(&key, &value);
                    }
                }
                parsed.to_string()
            }
            Err(_) => url,
        }
    }

    fn effective_body(&self) -> RequestBody {
        match self.body() {
            RequestBody::Raw(content) if content.is_empty() => RequestBody::None,
            RequestBody::Form(fields) if fields.is_empty() => RequestBody::None,
            other => other,
        }
    }

    pub fn editor_spec_for_focused_field(&self) -> Result<RequestPaneEditSpec, EditorError> {
        self.editor_spec_for(self.focused_editable_field())
    }

    pub fn focused_editable_field(&self) -> EditableField {
        match self.focused_field {
            FocusedField::Url | FocusedField::Method => EditableField::Url,
            FocusedField::Tabs => match RequestTab::all()[self.selected_tab] {
                RequestTab::Headers => EditableField::Headers,
                RequestTab::Body => EditableField::Body,
                RequestTab::Auth => self.current_auth_editable_field(),
                RequestTab::Params => EditableField::ParamKey(self.param_selected),
            },
            FocusedField::TabContent => match RequestTab::all()[self.selected_tab] {
                RequestTab::Headers => match self.header_field_focus {
                    HeaderField::Name => EditableField::HeaderName(self.header_selected),
                    HeaderField::Value => EditableField::HeaderValue(self.header_selected),
                },
                RequestTab::Body => EditableField::Body,
                RequestTab::Auth => self.current_auth_editable_field(),
                RequestTab::Params => match self.param_field_focus {
                    ParamField::Key => EditableField::ParamKey(self.param_selected),
                    ParamField::Value => EditableField::ParamValue(self.param_selected),
                },
            },
        }
    }

    pub fn editor_spec_for(
        &self,
        field: EditableField,
    ) -> Result<RequestPaneEditSpec, EditorError> {
        let spec = match field {
            EditableField::Url => RequestPaneEditSpec {
                field,
                format: EditorFormat::Text,
                content: self.url(),
            },
            EditableField::Body => match self.body_type {
                BodyType::Json => RequestPaneEditSpec {
                    field,
                    format: EditorFormat::Json,
                    content: self.body_content.as_str().to_string(),
                },
                BodyType::Form => RequestPaneEditSpec {
                    field,
                    format: EditorFormat::Yaml,
                    content: serde_yaml::to_string(&self.serializable_params())
                        .map_err(|err| EditorError::Validation(err.to_string()))?,
                },
                BodyType::Raw => RequestPaneEditSpec {
                    field,
                    format: EditorFormat::Text,
                    content: self.body_content.as_str().to_string(),
                },
            },
            EditableField::Headers => RequestPaneEditSpec {
                field,
                format: EditorFormat::Yaml,
                content: serde_yaml::to_string(&self.serializable_headers())
                    .map_err(|err| EditorError::Validation(err.to_string()))?,
            },
            EditableField::HeaderName(index) => RequestPaneEditSpec {
                field,
                format: EditorFormat::Text,
                content: self.header_buffers(index)?.0.as_str().to_string(),
            },
            EditableField::HeaderValue(index) => RequestPaneEditSpec {
                field,
                format: EditorFormat::Text,
                content: self.header_buffers(index)?.1.as_str().to_string(),
            },
            EditableField::AuthUsername => RequestPaneEditSpec {
                field,
                format: EditorFormat::Text,
                content: self.auth_username.as_str().to_string(),
            },
            EditableField::AuthType => RequestPaneEditSpec {
                field,
                format: EditorFormat::Text,
                content: self.auth_type.as_str().to_string(),
            },
            EditableField::AuthPassword => RequestPaneEditSpec {
                field,
                format: EditorFormat::Text,
                content: self.auth_password.as_str().to_string(),
            },
            EditableField::AuthToken => RequestPaneEditSpec {
                field,
                format: EditorFormat::Text,
                content: self.auth_token.as_str().to_string(),
            },
            EditableField::AuthKeyName => RequestPaneEditSpec {
                field,
                format: EditorFormat::Text,
                content: self.auth_key_name.as_str().to_string(),
            },
            EditableField::AuthKeyValue => RequestPaneEditSpec {
                field,
                format: EditorFormat::Text,
                content: self.auth_key_value.as_str().to_string(),
            },
            EditableField::ParamKey(index) => RequestPaneEditSpec {
                field,
                format: EditorFormat::Text,
                content: self.param_buffers(index)?.0.as_str().to_string(),
            },
            EditableField::ParamValue(index) => RequestPaneEditSpec {
                field,
                format: EditorFormat::Text,
                content: self.param_buffers(index)?.1.as_str().to_string(),
            },
        };

        Ok(spec)
    }

    pub fn validate_editor_content(
        &self,
        field: EditableField,
        content: &str,
    ) -> Result<(), EditorError> {
        match field {
            EditableField::Url => RequestUrl::new(normalize_single_line(content))
                .map(|_| ())
                .map_err(|err| EditorError::Validation(err.to_string())),
            EditableField::Body => match self.body_type {
                BodyType::Json => serde_json::from_str::<serde_json::Value>(content)
                    .map(|_| ())
                    .map_err(|err| EditorError::Validation(err.to_string())),
                BodyType::Form => {
                    let params: Vec<SerializableHeader> = serde_yaml::from_str(content)
                        .map_err(|err| EditorError::Validation(err.to_string()))?;
                    for param in params {
                        if param.name.trim().is_empty() {
                            return Err(EditorError::Validation(
                                "Form keys cannot be empty".to_string(),
                            ));
                        }
                    }
                    Ok(())
                }
                BodyType::Raw => Ok(()),
            },
            EditableField::Headers => {
                let headers: Vec<SerializableHeader> = serde_yaml::from_str(content)
                    .map_err(|err| EditorError::Validation(err.to_string()))?;
                for header in headers {
                    Header::new(header.name, header.value)
                        .map_err(|err| EditorError::Validation(err.to_string()))?;
                }
                Ok(())
            }
            EditableField::HeaderName(_) => Header::new(normalize_single_line(content), "")
                .map(|_| ())
                .map_err(|err| EditorError::Validation(err.to_string())),
            EditableField::AuthType => parse_auth_type(normalize_single_line(content)).map(|_| ()),
            EditableField::HeaderValue(_)
            | EditableField::AuthUsername
            | EditableField::AuthPassword
            | EditableField::AuthToken
            | EditableField::AuthKeyName
            | EditableField::AuthKeyValue
            | EditableField::ParamValue(_) => Ok(()),
            EditableField::ParamKey(_) => Ok(()),
        }
    }

    pub fn apply_edited_content(
        &mut self,
        field: EditableField,
        content: &str,
    ) -> Result<(), EditorError> {
        self.validate_editor_content(field, content)?;

        match field {
            EditableField::Url => {
                self.url_buffer = InputBuffer::with_content(normalize_single_line(content));
            }
            EditableField::Body => match self.body_type {
                BodyType::Json | BodyType::Raw => {
                    self.body_content = InputBuffer::with_content(content);
                }
                BodyType::Form => {
                    let params: Vec<SerializableHeader> = serde_yaml::from_str(content)
                        .map_err(|err| EditorError::Validation(err.to_string()))?;
                    self.params = params
                        .into_iter()
                        .map(|param| {
                            (
                                InputBuffer::with_content(&param.name),
                                InputBuffer::with_content(&param.value),
                            )
                        })
                        .collect();
                    if self.params.is_empty() {
                        self.params.push((InputBuffer::new(), InputBuffer::new()));
                    }
                    self.param_selected = self.param_selected.min(self.params.len() - 1);
                    self.body_content = InputBuffer::with_content(content);
                }
            },
            EditableField::Headers => {
                let headers: Vec<SerializableHeader> = serde_yaml::from_str(content)
                    .map_err(|err| EditorError::Validation(err.to_string()))?;
                self.headers = headers
                    .into_iter()
                    .map(|header| {
                        (
                            InputBuffer::with_content(&header.name),
                            InputBuffer::with_content(&header.value),
                        )
                    })
                    .collect();
                if self.headers.is_empty() {
                    self.headers.push((InputBuffer::new(), InputBuffer::new()));
                }
                self.header_selected = self.header_selected.min(self.headers.len() - 1);
            }
            EditableField::HeaderName(index) => {
                self.header_buffers_mut(index)?.0 =
                    InputBuffer::with_content(normalize_single_line(content));
            }
            EditableField::HeaderValue(index) => {
                self.header_buffers_mut(index)?.1 =
                    InputBuffer::with_content(normalize_single_line(content));
            }
            EditableField::AuthType => {
                self.auth_type = parse_auth_type(normalize_single_line(content))?;
            }
            EditableField::AuthUsername => {
                self.auth_username = InputBuffer::with_content(normalize_single_line(content));
            }
            EditableField::AuthPassword => {
                self.auth_password = InputBuffer::with_content(normalize_single_line(content));
            }
            EditableField::AuthToken => {
                self.auth_token = InputBuffer::with_content(normalize_single_line(content));
            }
            EditableField::AuthKeyName => {
                self.auth_key_name = InputBuffer::with_content(normalize_single_line(content));
            }
            EditableField::AuthKeyValue => {
                self.auth_key_value = InputBuffer::with_content(normalize_single_line(content));
            }
            EditableField::ParamKey(index) => {
                self.param_buffers_mut(index)?.0 =
                    InputBuffer::with_content(normalize_single_line(content));
            }
            EditableField::ParamValue(index) => {
                self.param_buffers_mut(index)?.1 =
                    InputBuffer::with_content(normalize_single_line(content));
            }
        }

        Ok(())
    }

    pub fn handle_key(&mut self, key_code: KeyCode, modifiers: KeyModifiers) -> bool {
        if self.method_popup_visible {
            return self.handle_method_popup_key(key_code);
        }

        if self.url_autocomplete_visible {
            return self.handle_autocomplete_key(key_code);
        }

        // Ctrl+F opens search in active tab content
        if modifiers.contains(KeyModifiers::CONTROL) && key_code == KeyCode::Char('f') {
            if self.focused_field == FocusedField::TabContent {
                self.search_visible = true;
                return true;
            }
        }

        match self.focused_field {
            FocusedField::Method => self.handle_method_key(key_code),
            FocusedField::Url => self.handle_url_key(key_code),
            FocusedField::Tabs => self.handle_tabs_key(key_code),
            FocusedField::TabContent => self.handle_tab_content_key(key_code),
        }
    }

    fn handle_method_key(&mut self, key_code: KeyCode) -> bool {
        match key_code {
            KeyCode::Enter => {
                self.method_popup_visible = true;
                let current_idx = Method::all()
                    .iter()
                    .position(|m| *m == self.method)
                    .unwrap_or(0);
                self.method_list_state.select(Some(current_idx));
                true
            }
            KeyCode::Right => {
                self.focused_field = FocusedField::Url;
                true
            }
            KeyCode::Down | KeyCode::Tab => {
                self.focused_field = FocusedField::Url;
                true
            }
            _ => false,
        }
    }

    fn handle_url_key(&mut self, key_code: KeyCode) -> bool {
        match key_code {
            KeyCode::Left => {
                if self.url_buffer.cursor_pos == 0 {
                    self.focused_field = FocusedField::Method;
                } else {
                    self.url_buffer.move_cursor_left();
                }
                true
            }
            KeyCode::Right => {
                self.url_buffer.move_cursor_right();
                true
            }
            KeyCode::Backspace => {
                self.url_buffer.delete_char();
                true
            }
            KeyCode::Enter => {
                self.focused_field = FocusedField::Tabs;
                true
            }
            KeyCode::Tab | KeyCode::Down => {
                self.focused_field = FocusedField::Tabs;
                true
            }
            KeyCode::Up => {
                if self.url_buffer.cursor_pos == 0 && self.url_buffer.is_empty() {
                    self.focused_field = FocusedField::Method;
                }
                true
            }
            KeyCode::Char(c) => {
                self.url_buffer.insert_char(c);
                if !self.url_buffer.as_str().is_empty() && !self.url_autocomplete_visible {
                    self.url_autocomplete_visible = true;
                    self.url_autocomplete_selected = 0;
                }
                true
            }
            _ => false,
        }
    }

    fn handle_tabs_key(&mut self, key_code: KeyCode) -> bool {
        match key_code {
            KeyCode::Left => {
                if self.selected_tab == 0 {
                    self.focused_field = FocusedField::Url;
                } else {
                    self.selected_tab = self.selected_tab.saturating_sub(1);
                }
                true
            }
            KeyCode::Right => {
                let tabs = RequestTab::all();
                if self.selected_tab >= tabs.len() - 1 {
                    self.focused_field = FocusedField::TabContent;
                } else {
                    self.selected_tab = (self.selected_tab + 1).min(tabs.len() - 1);
                }
                true
            }
            KeyCode::Down | KeyCode::Enter => {
                self.focused_field = FocusedField::TabContent;
                true
            }
            KeyCode::Up => {
                self.focused_field = FocusedField::Url;
                true
            }
            _ => false,
        }
    }

    fn handle_tab_content_key(&mut self, key_code: KeyCode) -> bool {
        let tab = RequestTab::all()[self.selected_tab];
        match tab {
            RequestTab::Headers => self.handle_headers_key(key_code),
            RequestTab::Body => self.handle_body_key(key_code),
            RequestTab::Auth => self.handle_auth_key(key_code),
            RequestTab::Params => self.handle_params_key(key_code),
        }
    }

    fn handle_headers_key(&mut self, key_code: KeyCode) -> bool {
        match key_code {
            KeyCode::Up => {
                if self.header_selected == 0 {
                    self.focused_field = FocusedField::Tabs;
                } else {
                    self.header_selected = self.header_selected.saturating_sub(1);
                }
                true
            }
            KeyCode::Down => {
                if self.header_selected >= self.headers.len() - 1 {
                    let (k, v) = self.headers.last().unwrap();
                    if k.as_str().is_empty() && v.as_str().is_empty() {
                        return true;
                    }
                }
                self.header_selected = (self.header_selected + 1).min(self.headers.len() - 1);
                true
            }
            KeyCode::Left => {
                self.header_field_focus = HeaderField::Name;
                true
            }
            KeyCode::Right => {
                self.header_field_focus = HeaderField::Value;
                true
            }
            KeyCode::Char('a') => {
                self.headers.push((InputBuffer::new(), InputBuffer::new()));
                self.header_selected = self.headers.len() - 1;
                true
            }
            KeyCode::Char('d') => {
                if self.headers.len() > 1 && self.header_selected < self.headers.len() {
                    self.headers.remove(self.header_selected);
                    if self.header_selected >= self.headers.len() {
                        self.header_selected = self.headers.len() - 1;
                    }
                }
                true
            }
            KeyCode::Backspace => {
                let (name_buf, value_buf) = &mut self.headers[self.header_selected];
                match self.header_field_focus {
                    HeaderField::Name => {
                        name_buf.delete_char();
                    }
                    HeaderField::Value => {
                        value_buf.delete_char();
                    }
                }
                true
            }
            KeyCode::Char(c) => {
                let (name_buf, value_buf) = &mut self.headers[self.header_selected];
                match self.header_field_focus {
                    HeaderField::Name => name_buf.insert_char(c),
                    HeaderField::Value => value_buf.insert_char(c),
                }
                true
            }
            _ => true,
        }
    }

    fn handle_body_key(&mut self, key_code: KeyCode) -> bool {
        match key_code {
            KeyCode::Up => {
                self.focused_field = FocusedField::Tabs;
                true
            }
            KeyCode::Left => {
                self.body_content.move_cursor_left();
                true
            }
            KeyCode::Right => {
                self.body_content.move_cursor_right();
                true
            }
            KeyCode::Backspace => {
                self.body_content.delete_char();
                true
            }
            KeyCode::Char('t') => {
                self.body_type_selected = (self.body_type_selected + 1) % BodyType::all().len();
                self.body_type = BodyType::all()[self.body_type_selected];
                true
            }
            KeyCode::Char(c) => {
                self.body_content.insert_char(c);
                true
            }
            _ => true,
        }
    }

    fn handle_auth_key(&mut self, key_code: KeyCode) -> bool {
        match key_code {
            KeyCode::Up => {
                self.focused_field = FocusedField::Tabs;
                true
            }
            KeyCode::Left => {
                self.auth_field_focus = AuthField::Type;
                true
            }
            KeyCode::Right => {
                match self.auth_type {
                    AuthType::Basic => self.auth_field_focus = AuthField::Username,
                    AuthType::Bearer => self.auth_field_focus = AuthField::Token,
                    AuthType::ApiKey => self.auth_field_focus = AuthField::KeyName,
                    AuthType::None => self.auth_field_focus = AuthField::Type,
                }
                true
            }
            KeyCode::Down => {
                match self.auth_type {
                    AuthType::Basic => {
                        if self.auth_field_focus == AuthField::Username {
                            self.auth_field_focus = AuthField::Password;
                        }
                    }
                    AuthType::ApiKey => {
                        if self.auth_field_focus == AuthField::KeyName {
                            self.auth_field_focus = AuthField::KeyValue;
                        }
                    }
                    _ => {}
                }
                true
            }
            KeyCode::Char('t') => {
                let types = AuthType::all();
                let current = types.iter().position(|t| *t == self.auth_type).unwrap_or(0);
                self.auth_type = types[(current + 1) % types.len()];
                true
            }
            KeyCode::Backspace => {
                match self.auth_field_focus {
                    AuthField::Username => {
                        self.auth_username.delete_char();
                    }
                    AuthField::Password => {
                        self.auth_password.delete_char();
                    }
                    AuthField::Token => {
                        self.auth_token.delete_char();
                    }
                    AuthField::KeyName => {
                        self.auth_key_name.delete_char();
                    }
                    AuthField::KeyValue => {
                        self.auth_key_value.delete_char();
                    }
                    _ => {}
                }
                true
            }
            KeyCode::Char(c) => {
                match self.auth_field_focus {
                    AuthField::Username => self.auth_username.insert_char(c),
                    AuthField::Password => self.auth_password.insert_char(c),
                    AuthField::Token => self.auth_token.insert_char(c),
                    AuthField::KeyName => self.auth_key_name.insert_char(c),
                    AuthField::KeyValue => self.auth_key_value.insert_char(c),
                    _ => {}
                }
                true
            }
            _ => true,
        }
    }

    fn handle_params_key(&mut self, key_code: KeyCode) -> bool {
        match key_code {
            KeyCode::Up => {
                if self.param_selected == 0 {
                    self.focused_field = FocusedField::Tabs;
                } else {
                    self.param_selected = self.param_selected.saturating_sub(1);
                }
                true
            }
            KeyCode::Down => {
                if self.param_selected >= self.params.len() - 1 {
                    let (k, v) = self.params.last().unwrap();
                    if k.as_str().is_empty() && v.as_str().is_empty() {
                        return true;
                    }
                }
                self.param_selected = (self.param_selected + 1).min(self.params.len() - 1);
                true
            }
            KeyCode::Left => {
                self.param_field_focus = ParamField::Key;
                true
            }
            KeyCode::Right => {
                self.param_field_focus = ParamField::Value;
                true
            }
            KeyCode::Char('a') => {
                self.params.push((InputBuffer::new(), InputBuffer::new()));
                self.param_selected = self.params.len() - 1;
                true
            }
            KeyCode::Char('d') => {
                if self.params.len() > 1 && self.param_selected < self.params.len() {
                    self.params.remove(self.param_selected);
                    if self.param_selected >= self.params.len() {
                        self.param_selected = self.params.len() - 1;
                    }
                }
                true
            }
            KeyCode::Backspace => {
                let (key_buf, value_buf) = &mut self.params[self.param_selected];
                match self.param_field_focus {
                    ParamField::Key => {
                        key_buf.delete_char();
                    }
                    ParamField::Value => {
                        value_buf.delete_char();
                    }
                }
                true
            }
            KeyCode::Char(c) => {
                let (key_buf, value_buf) = &mut self.params[self.param_selected];
                match self.param_field_focus {
                    ParamField::Key => key_buf.insert_char(c),
                    ParamField::Value => value_buf.insert_char(c),
                }
                true
            }
            _ => true,
        }
    }

    fn handle_method_popup_key(&mut self, key_code: KeyCode) -> bool {
        let methods = Method::all();
        match key_code {
            KeyCode::Up => {
                let idx = self.method_list_state.selected().unwrap_or(0);
                let new_idx = idx.saturating_sub(1);
                self.method_list_state.select(Some(new_idx));
            }
            KeyCode::Down => {
                let idx = self.method_list_state.selected().unwrap_or(0);
                let new_idx = (idx + 1).min(methods.len() - 1);
                self.method_list_state.select(Some(new_idx));
            }
            KeyCode::Enter => {
                if let Some(idx) = self.method_list_state.selected() {
                    self.method = methods[idx];
                }
                self.method_popup_visible = false;
            }
            KeyCode::Esc => {
                self.method_popup_visible = false;
            }
            _ => {}
        }
        true
    }

    fn handle_autocomplete_key(&mut self, key_code: KeyCode) -> bool {
        match key_code {
            KeyCode::Up => {
                if self.url_autocomplete_selected > 0 {
                    self.url_autocomplete_selected -= 1;
                }
            }
            KeyCode::Down => {
                let matches = self.get_url_matches();
                if self.url_autocomplete_selected < matches.len().saturating_sub(1) {
                    self.url_autocomplete_selected += 1;
                }
            }
            KeyCode::Enter => {
                let matches = self.get_url_matches();
                if let Some(url) = matches.get(self.url_autocomplete_selected) {
                    self.url_buffer = InputBuffer::with_content(url);
                }
                self.url_autocomplete_visible = false;
            }
            KeyCode::Esc => {
                self.url_autocomplete_visible = false;
            }
            KeyCode::Char(c) => {
                // Allow typing while autocomplete is visible
                self.url_buffer.insert_char(c);
                self.url_autocomplete_selected = 0;
                if self.get_url_matches().is_empty() {
                    self.url_autocomplete_visible = false;
                }
            }
            KeyCode::Backspace => {
                self.url_buffer.delete_char();
                self.url_autocomplete_selected = 0;
                if self.get_url_matches().is_empty() {
                    self.url_autocomplete_visible = false;
                }
            }
            _ => {}
        }
        true
    }

    fn get_url_matches(&self) -> Vec<String> {
        let input = self.url_buffer.as_str().to_lowercase();
        self.url_history
            .iter()
            .filter(|h| h.to_lowercase().starts_with(&input))
            .cloned()
            .collect()
    }

    fn current_auth_editable_field(&self) -> EditableField {
        match self.auth_field_focus {
            AuthField::Type => EditableField::AuthType,
            AuthField::Token => EditableField::AuthToken,
            AuthField::Username => EditableField::AuthUsername,
            AuthField::Password => EditableField::AuthPassword,
            AuthField::KeyName => EditableField::AuthKeyName,
            AuthField::KeyValue => EditableField::AuthKeyValue,
        }
    }

    fn serializable_headers(&self) -> Vec<SerializableHeader> {
        self.headers
            .iter()
            .filter_map(|(name, value)| {
                let name = name.as_str().trim();
                if name.is_empty() {
                    None
                } else {
                    Some(SerializableHeader {
                        name: name.to_string(),
                        value: value.as_str().to_string(),
                    })
                }
            })
            .collect()
    }

    fn serializable_params(&self) -> Vec<SerializableHeader> {
        self.params
            .iter()
            .filter_map(|(name, value)| {
                let name = name.as_str().trim();
                if name.is_empty() {
                    None
                } else {
                    Some(SerializableHeader {
                        name: name.to_string(),
                        value: value.as_str().to_string(),
                    })
                }
            })
            .collect()
    }

    fn header_buffers(&self, index: usize) -> Result<&(InputBuffer, InputBuffer), EditorError> {
        self.headers.get(index).ok_or_else(|| {
            EditorError::Validation(format!("Header index {index} is out of bounds"))
        })
    }

    fn header_buffers_mut(
        &mut self,
        index: usize,
    ) -> Result<&mut (InputBuffer, InputBuffer), EditorError> {
        self.headers.get_mut(index).ok_or_else(|| {
            EditorError::Validation(format!("Header index {index} is out of bounds"))
        })
    }

    fn param_buffers(&self, index: usize) -> Result<&(InputBuffer, InputBuffer), EditorError> {
        self.params
            .get(index)
            .ok_or_else(|| EditorError::Validation(format!("Param index {index} is out of bounds")))
    }

    fn param_buffers_mut(
        &mut self,
        index: usize,
    ) -> Result<&mut (InputBuffer, InputBuffer), EditorError> {
        self.params
            .get_mut(index)
            .ok_or_else(|| EditorError::Validation(format!("Param index {index} is out of bounds")))
    }

    pub fn render_compact(
        &mut self,
        frame: &mut Frame,
        area: Rect,
        theme: &Theme,
        is_active: bool,
    ) {
        if area.width < 10 || area.height < 5 {
            return;
        }

        let bg = theme.pane_bg(is_active);
        let inner = area;

        // Background fill
        frame.render_widget(
            Block::default().style(Style::default().bg(bg).fg(theme.foreground.as_color())),
            area,
        );

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints(vec![
                Constraint::Length(3),
                Constraint::Length(1),
                Constraint::Min(0),
            ])
            .split(inner);

        self.render_compact_method_url(frame, chunks[0], theme, is_active);
        self.render_compact_tabs(frame, chunks[1], theme, is_active);
        self.render_compact_selected_tab(frame, chunks[2], theme, is_active);

        if self.method_popup_visible {
            self.render_method_popup(frame, area, theme);
        }

        if self.url_autocomplete_visible {
            self.render_autocomplete(frame, chunks[0], theme);
        }
    }

    fn render_compact_method_url(
        &self,
        frame: &mut Frame,
        area: Rect,
        theme: &Theme,
        is_active: bool,
    ) {
        let method_focused = is_active && self.focused_field == FocusedField::Method;
        let url_focused = is_active && self.focused_field == FocusedField::Url;

        let method_color = self.method_theme_color(theme);
        let bar_bg = if is_active && (method_focused || url_focused) {
            theme.bg_element()
        } else {
            theme.pane_bg(is_active)
        };

        let url_display = if self.url_buffer.as_str().is_empty() {
            "Paste or type a URL..."
        } else {
            self.url_buffer.as_str()
        };

        let url_style = if self.url_buffer.as_str().is_empty() {
            theme.placeholder_color()
        } else if url_focused {
            theme.foreground.as_color()
        } else {
            theme.typography_level(2).0
        };

        let bar_content = Line::from(vec![
            Span::styled(
                format!(" {} ", self.method.as_str()),
                Style::default()
                    .fg(method_color)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("  ", Style::default().fg(theme.text_muted())),
            Span::styled(url_display, Style::default().fg(url_style)),
        ]);

        let bar_para = Paragraph::new(bar_content)
            .style(Style::default().bg(bar_bg).fg(theme.foreground.as_color()))
            .wrap(Wrap { trim: true });
        frame.render_widget(bar_para, area);

        if url_focused {
            let mut cursor_x = 0u16;
            let method_part = format!(" {}  ", self.method.as_str());
            let url_prefix = &self.url_buffer.as_str()[..self.url_buffer.cursor_pos];
            cursor_x = cursor_x.saturating_add(method_part.chars().count() as u16);
            cursor_x = cursor_x.saturating_add(url_prefix.chars().count() as u16);
            frame.set_cursor_position(ratatui::prelude::Position::new(
                area.x + cursor_x.min(area.width.saturating_sub(1)),
                area.y,
            ));
        }
    }

    fn method_theme_color(&self, theme: &Theme) -> ratatui::style::Color {
        match self.method {
            yinx_core::request::Method::Get => theme.semantic.success.as_color(),
            yinx_core::request::Method::Post => theme.semantic.info.as_color(),
            yinx_core::request::Method::Put => theme.semantic.warning.as_color(),
            yinx_core::request::Method::Patch => theme.semantic.warning.as_color(),
            yinx_core::request::Method::Delete => theme.semantic.error.as_color(),
            yinx_core::request::Method::Head => theme.semantic.info.as_color(),
            yinx_core::request::Method::Options => theme.semantic.info.as_color(),
        }
    }

    fn render_compact_tabs(&self, frame: &mut Frame, area: Rect, theme: &Theme, is_active: bool) {
        let mut spans: Vec<Span> = Vec::new();
        let titles = self.tab_titles();

        for (i, title) in titles.iter().enumerate() {
            if i > 0 {
                spans.push(Span::raw("  "));
            }
            if i == self.selected_tab {
                spans.push(Span::styled(
                    format!(" {} ", title),
                    Style::default()
                        .fg(theme.highlight.selected_fg.as_color())
                        .bg(theme.highlight.selected_bg.as_color())
                        .add_modifier(Modifier::BOLD),
                ));
            } else {
                let is_tabs_focused = is_active && self.focused_field == FocusedField::Tabs;
                spans.push(Span::styled(
                    format!(" {} ", title),
                    Style::default()
                        .fg(if is_tabs_focused || is_active {
                            theme.typography_level(1).0
                        } else {
                            theme.typography_level(3).0
                        })
                        .bg(theme.subtle_bg()),
                ));
            }
        }

        let line = Line::from(spans);
        let paragraph = Paragraph::new(line).style(Style::default().bg(theme.pane_bg(is_active)));
        frame.render_widget(paragraph, area);
    }

    fn render_compact_selected_tab(
        &mut self,
        frame: &mut Frame,
        area: Rect,
        theme: &Theme,
        is_active: bool,
    ) {
        let tab = RequestTab::all()[self.selected_tab];
        match tab {
            RequestTab::Headers => self.render_compact_headers(frame, area, theme, is_active),
            RequestTab::Body => self.render_compact_body(frame, area, theme, is_active),
            RequestTab::Auth => self.render_auth_tab(frame, area, theme, is_active),
            RequestTab::Params => self.render_compact_params(frame, area, theme, is_active),
        }
    }

    fn render_compact_headers(
        &self,
        frame: &mut Frame,
        area: Rect,
        theme: &Theme,
        is_active: bool,
    ) {
        let rows: Vec<Row> = self
            .headers
            .iter()
            .enumerate()
            .map(|(i, (name, value))| {
                let is_selected = is_active
                    && self.focused_field == FocusedField::TabContent
                    && self.header_selected == i;
                let style = if is_selected {
                    Style::default()
                        .bg(theme.highlight.selected_bg.as_color())
                        .fg(theme.highlight.selected_fg.as_color())
                } else {
                    Style::default().fg(theme.foreground.as_color())
                };

                Row::new(vec![
                    Cell::from(name.as_str().to_string()).style(style),
                    Cell::from(value.as_str().to_string()).style(style),
                ])
                .style(style)
            })
            .collect();

        let table = Table::new(
            rows,
            &[Constraint::Percentage(40), Constraint::Percentage(60)],
        )
        .header(
            Row::new(vec![Cell::from("Key"), Cell::from("Value")]).style(
                Style::default()
                    .bg(theme.bg_element())
                    .fg(theme.typography_level(1).0)
                    .add_modifier(Modifier::BOLD),
            ),
        )
        .block(
            Block::default().style(
                Style::default()
                    .bg(theme.pane_bg(is_active))
                    .fg(theme.foreground.as_color()),
            ),
        );

        let mut state = ratatui::widgets::TableState::default();
        if is_active && self.focused_field == FocusedField::TabContent {
            state.select(Some(self.header_selected));
        }

        frame.render_stateful_widget(table, area, &mut state);
    }

    fn render_compact_body(&self, frame: &mut Frame, area: Rect, theme: &Theme, is_active: bool) {
        let body_lines = if self.body_content.as_str().is_empty() {
            vec![Line::from(Span::styled(
                "Compose a request body here…",
                Style::default().fg(theme.placeholder_color()),
            ))]
        } else {
            self.body_content
                .as_str()
                .lines()
                .map(|line| Line::from(line.to_string()))
                .collect::<Vec<_>>()
        };

        let body_para = Paragraph::new(body_lines)
            .style(
                Style::default()
                    .fg(theme.foreground.as_color())
                    .bg(theme.pane_bg(is_active)),
            )
            .wrap(Wrap { trim: false });
        frame.render_widget(body_para, area);

        if is_active && self.focused_field == FocusedField::TabContent {
            let x_offset = self.body_content.as_str()[..self.body_content.cursor_pos]
                .chars()
                .count() as u16;
            frame.set_cursor_position(ratatui::prelude::Position::new(
                area.x + x_offset.min(area.width.saturating_sub(1)),
                area.y,
            ));
        }
    }

    fn render_compact_params(&self, frame: &mut Frame, area: Rect, theme: &Theme, is_active: bool) {
        let rows: Vec<Row> = self
            .params
            .iter()
            .enumerate()
            .map(|(i, (key, value))| {
                let is_selected = is_active
                    && self.focused_field == FocusedField::TabContent
                    && self.param_selected == i;
                let style = if is_selected {
                    Style::default()
                        .bg(theme.highlight.selected_bg.as_color())
                        .fg(theme.highlight.selected_fg.as_color())
                } else {
                    Style::default().fg(theme.foreground.as_color())
                };

                Row::new(vec![
                    Cell::from(key.as_str().to_string()).style(style),
                    Cell::from(value.as_str().to_string()).style(style),
                ])
                .style(style)
            })
            .collect();

        let table = Table::new(
            rows,
            &[Constraint::Percentage(40), Constraint::Percentage(60)],
        )
        .header(
            Row::new(vec![Cell::from("Key"), Cell::from("Value")]).style(
                Style::default()
                    .bg(theme.bg_element())
                    .fg(theme.typography_level(1).0)
                    .add_modifier(Modifier::BOLD),
            ),
        )
        .block(
            Block::default().style(
                Style::default()
                    .bg(theme.pane_bg(is_active))
                    .fg(theme.foreground.as_color()),
            ),
        );

        let mut state = ratatui::widgets::TableState::default();
        if is_active && self.focused_field == FocusedField::TabContent {
            state.select(Some(self.param_selected));
        }

        frame.render_stateful_widget(table, area, &mut state);
    }

    pub fn render(&mut self, frame: &mut Frame, area: Rect, theme: &Theme, is_active: bool) {
        let inner = area;

        // Background fill
        frame.render_widget(
            Block::default().style(
                Style::default()
                    .bg(theme.pane_bg(is_active))
                    .fg(theme.foreground.as_color()),
            ),
            area,
        );

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints(vec![
                Constraint::Length(3),
                Constraint::Length(3),
                Constraint::Min(0),
            ])
            .split(inner);

        self.render_method_url(frame, chunks[0], theme, is_active);
        self.render_tabs(frame, chunks[1], theme, is_active);
        self.render_tab_content(frame, chunks[2], theme, is_active);

        if self.method_popup_visible {
            self.render_method_popup(frame, area, theme);
        }

        if self.url_autocomplete_visible {
            self.render_autocomplete(frame, chunks[0], theme);
        }
    }

    fn render_method_url(&self, frame: &mut Frame, area: Rect, theme: &Theme, is_active: bool) {
        let method_focused = is_active && self.focused_field == FocusedField::Method;
        let url_focused = is_active && self.focused_field == FocusedField::Url;

        let method_color = self.method_theme_color(theme);
        let bar_bg = if is_active && (method_focused || url_focused) {
            theme.bg_element()
        } else {
            theme.pane_bg(is_active)
        };

        let url_display = if self.url_buffer.as_str().is_empty() {
            "Paste or type a URL..."
        } else {
            self.url_buffer.as_str()
        };

        let url_style = if self.url_buffer.as_str().is_empty() {
            theme.placeholder_color()
        } else if url_focused {
            theme.foreground.as_color()
        } else {
            theme.text_muted()
        };

        let bar_content = Line::from(vec![
            Span::styled(
                format!(" {} ", self.method.as_str()),
                Style::default()
                    .fg(method_color)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("  ", Style::default().fg(theme.text_muted())),
            Span::styled(url_display, Style::default().fg(url_style)),
        ]);

        let bar_para = Paragraph::new(bar_content)
            .style(Style::default().bg(bar_bg).fg(theme.foreground.as_color()))
            .wrap(Wrap { trim: true });
        frame.render_widget(bar_para, area);

        if url_focused {
            let mut cursor_x = 0u16;
            let method_part = format!(" {}  ", self.method.as_str());
            let url_prefix = &self.url_buffer.as_str()[..self.url_buffer.cursor_pos];
            cursor_x = cursor_x.saturating_add(method_part.chars().count() as u16);
            cursor_x = cursor_x.saturating_add(url_prefix.chars().count() as u16);
            frame.set_cursor_position(ratatui::prelude::Position::new(
                area.x + cursor_x.min(area.width.saturating_sub(1)),
                area.y,
            ));
        }
    }

    fn render_tabs(&self, frame: &mut Frame, area: Rect, theme: &Theme, is_active: bool) {
        let titles: Vec<Line> = self.tab_titles().into_iter().map(Line::from).collect();

        let tabs_widget = Tabs::new(titles)
            .select(self.selected_tab)
            .block(Block::default().border_style(Style::default().fg(
                if is_active && self.focused_field == FocusedField::Tabs {
                    theme.border.active_color.as_color()
                } else {
                    theme.border.color.as_color()
                },
            )))
            .style(
                Style::default()
                    .bg(theme.subtle_bg())
                    .fg(theme.foreground.as_color()),
            )
            .highlight_style(
                Style::default()
                    .fg(theme.highlight.selected_fg.as_color())
                    .bg(theme.highlight.selected_bg.as_color())
                    .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
            );
        frame.render_widget(tabs_widget, area);
    }

    fn render_tab_content(&self, frame: &mut Frame, area: Rect, theme: &Theme, is_active: bool) {
        let tab = RequestTab::all()[self.selected_tab];

        match tab {
            RequestTab::Headers => self.render_headers_tab(frame, area, theme, is_active),
            RequestTab::Body => self.render_body_tab(frame, area, theme, is_active),
            RequestTab::Auth => self.render_auth_tab(frame, area, theme, is_active),
            RequestTab::Params => self.render_params_tab(frame, area, theme, is_active),
        }
    }

    fn render_headers_tab(&self, frame: &mut Frame, area: Rect, theme: &Theme, is_active: bool) {
        let header = Row::new(vec![Cell::from("Name"), Cell::from("Value")]).style(
            Style::default()
                .fg(theme.pane.title.as_color())
                .add_modifier(Modifier::BOLD),
        );

        let rows: Vec<Row> = self
            .headers
            .iter()
            .enumerate()
            .map(|(i, (name, value))| {
                let is_selected = is_active
                    && self.focused_field == FocusedField::TabContent
                    && self.header_selected == i;
                let style = if is_selected {
                    Style::default()
                        .bg(theme.highlight.selected_bg.as_color())
                        .fg(theme.highlight.selected_fg.as_color())
                } else {
                    Style::default().fg(theme.foreground.as_color())
                };

                let name_style = if is_selected && self.header_field_focus == HeaderField::Name {
                    Style::default()
                        .bg(theme.highlight.selected_bg.as_color())
                        .fg(theme.highlight.selected_fg.as_color())
                        .add_modifier(Modifier::BOLD)
                } else {
                    style
                };

                let value_style = if is_selected && self.header_field_focus == HeaderField::Value {
                    Style::default()
                        .bg(theme.highlight.selected_bg.as_color())
                        .fg(theme.highlight.selected_fg.as_color())
                        .add_modifier(Modifier::BOLD)
                } else {
                    style
                };

                Row::new(vec![
                    Cell::from(name.as_str().to_string()).style(name_style),
                    Cell::from(value.as_str().to_string()).style(value_style),
                ])
                .style(style)
            })
            .collect();

        let table = Table::new(
            rows,
            &[Constraint::Percentage(40), Constraint::Percentage(60)],
        )
        .header(header)
        .block(
            Block::default().style(
                Style::default()
                    .bg(theme.pane_bg(is_active))
                    .fg(theme.foreground.as_color()),
            ),
        )
        .row_highlight_style(
            Style::default()
                .bg(theme.highlight.selected_bg.as_color())
                .fg(theme.highlight.selected_fg.as_color()),
        );

        let mut state = ratatui::widgets::TableState::default();
        if is_active && self.focused_field == FocusedField::TabContent {
            state.select(Some(self.header_selected));
        }

        frame.render_stateful_widget(table, area, &mut state);
    }

    fn render_body_tab(&self, frame: &mut Frame, area: Rect, theme: &Theme, is_active: bool) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints(vec![Constraint::Length(3), Constraint::Min(0)])
            .split(area);

        let body_type_str = self.body_type.as_str();
        let type_style = if is_active
            && self.focused_field == FocusedField::TabContent
            && self.auth_field_focus == AuthField::Type
        {
            Style::default()
                .fg(theme.highlight.selected_fg.as_color())
                .bg(theme.highlight.selected_bg.as_color())
        } else {
            Style::default().fg(theme.foreground.as_color())
        };

        let type_para = Paragraph::new(Line::from(vec![Span::styled(
            format!("BODY TYPE {}  t cycle", body_type_str),
            type_style,
        )]))
        .style(
            Style::default()
                .bg(theme.bg_element())
                .fg(theme.typography_level(1).0),
        );

        frame.render_widget(type_para, chunks[0]);

        let content_style = if is_active && self.focused_field == FocusedField::TabContent {
            Style::default()
                .fg(theme.highlight.selected_fg.as_color())
                .bg(theme.highlight.selected_bg.as_color())
        } else {
            Style::default()
                .fg(theme.foreground.as_color())
                .bg(theme.pane_bg(is_active))
        };

        let body_para = Paragraph::new(self.body_content.as_str())
            .style(content_style)
            .wrap(Wrap { trim: false });

        frame.render_widget(body_para, chunks[1]);

        if is_active && self.focused_field == FocusedField::TabContent {
            let x_offset = self.body_content.as_str()[..self.body_content.cursor_pos]
                .chars()
                .count() as u16;
            frame.set_cursor_position(ratatui::prelude::Position::new(
                chunks[1].x + 1 + x_offset,
                chunks[1].y + 1,
            ));
        }
    }

    fn render_auth_tab(&self, frame: &mut Frame, area: Rect, theme: &Theme, is_active: bool) {
        let auth_type_str = self.auth_type.as_str();
        let is_focused = is_active && self.focused_field == FocusedField::TabContent;

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints(vec![Constraint::Length(3), Constraint::Min(0)])
            .split(area);

        let type_style = if is_focused && self.auth_field_focus == AuthField::Type {
            Style::default()
                .fg(theme.highlight.selected_fg.as_color())
                .bg(theme.highlight.selected_bg.as_color())
        } else {
            Style::default().fg(theme.foreground.as_color())
        };

        let type_para = Paragraph::new(Line::from(vec![
            Span::styled(
                "Type: ",
                Style::default()
                    .fg(theme.section_title())
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(auth_type_str, type_style),
            Span::styled("  t cycle", Style::default().fg(theme.text_muted())),
        ]))
        .style(
            Style::default()
                .bg(theme.pane_bg(is_active))
                .fg(theme.foreground.as_color()),
        );

        frame.render_widget(type_para, chunks[0]);

        match self.auth_type {
            AuthType::None => {
                let para = Paragraph::new("No authentication configured").style(
                    Style::default()
                        .fg(theme.typography_level(2).0)
                        .bg(theme.pane_bg(is_active)),
                );
                frame.render_widget(para, chunks[1]);
            }
            AuthType::Basic => {
                let inner_chunks = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints(vec![Constraint::Length(3), Constraint::Length(3)])
                    .split(chunks[1]);

                let user_style = if is_focused && self.auth_field_focus == AuthField::Username {
                    Style::default()
                        .fg(theme.highlight.selected_fg.as_color())
                        .bg(theme.highlight.selected_bg.as_color())
                } else {
                    Style::default()
                        .fg(theme.foreground.as_color())
                        .bg(theme.bg_element())
                };

                let pass_style = if is_focused && self.auth_field_focus == AuthField::Password {
                    Style::default()
                        .fg(theme.highlight.selected_fg.as_color())
                        .bg(theme.highlight.selected_bg.as_color())
                } else {
                    Style::default()
                        .fg(theme.foreground.as_color())
                        .bg(theme.bg_element())
                };

                let user_line = Line::from(vec![
                    Span::styled(
                        "Username ",
                        Style::default()
                            .fg(theme.section_title())
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(self.auth_username.as_str(), Style::default()),
                ]);
                let user_para = Paragraph::new(user_line).style(user_style);
                frame.render_widget(user_para, inner_chunks[0]);

                let pass_line = Line::from(vec![
                    Span::styled(
                        "Password ",
                        Style::default()
                            .fg(theme.section_title())
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(self.auth_password.as_str(), Style::default()),
                ]);
                let pass_para = Paragraph::new(pass_line).style(pass_style);
                frame.render_widget(pass_para, inner_chunks[1]);

                if is_focused && self.auth_field_focus == AuthField::Username {
                    let prefix = "Username ";
                    let x_offset = prefix.chars().count() as u16
                        + self.auth_username.as_str()[..self.auth_username.cursor_pos]
                            .chars()
                            .count() as u16;
                    frame.set_cursor_position(ratatui::prelude::Position::new(
                        inner_chunks[0].x + x_offset.min(inner_chunks[0].width.saturating_sub(1)),
                        inner_chunks[0].y,
                    ));
                } else if is_focused && self.auth_field_focus == AuthField::Password {
                    let prefix = "Password ";
                    let x_offset = prefix.chars().count() as u16
                        + self.auth_password.as_str()[..self.auth_password.cursor_pos]
                            .chars()
                            .count() as u16;
                    frame.set_cursor_position(ratatui::prelude::Position::new(
                        inner_chunks[1].x + x_offset.min(inner_chunks[1].width.saturating_sub(1)),
                        inner_chunks[1].y,
                    ));
                }
            }
            AuthType::Bearer => {
                let token_style = if is_focused && self.auth_field_focus == AuthField::Token {
                    Style::default()
                        .fg(theme.highlight.selected_fg.as_color())
                        .bg(theme.highlight.selected_bg.as_color())
                } else {
                    Style::default()
                        .fg(theme.foreground.as_color())
                        .bg(theme.bg_element())
                };

                let token_line = Line::from(vec![
                    Span::styled(
                        "Bearer Token ",
                        Style::default()
                            .fg(theme.section_title())
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(self.auth_token.as_str(), Style::default()),
                ]);
                let token_para = Paragraph::new(token_line).style(token_style);
                frame.render_widget(token_para, chunks[1]);

                if is_focused && self.auth_field_focus == AuthField::Token {
                    let prefix = "Bearer Token ";
                    let x_offset = prefix.chars().count() as u16
                        + self.auth_token.as_str()[..self.auth_token.cursor_pos]
                            .chars()
                            .count() as u16;
                    frame.set_cursor_position(ratatui::prelude::Position::new(
                        chunks[1].x + x_offset.min(chunks[1].width.saturating_sub(1)),
                        chunks[1].y,
                    ));
                }
            }
            AuthType::ApiKey => {
                let inner_chunks = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints(vec![Constraint::Length(3), Constraint::Length(3)])
                    .split(chunks[1]);

                let key_style = if is_focused && self.auth_field_focus == AuthField::KeyName {
                    Style::default()
                        .fg(theme.highlight.selected_fg.as_color())
                        .bg(theme.highlight.selected_bg.as_color())
                } else {
                    Style::default()
                        .fg(theme.foreground.as_color())
                        .bg(theme.bg_element())
                };

                let value_style = if is_focused && self.auth_field_focus == AuthField::KeyValue {
                    Style::default()
                        .fg(theme.highlight.selected_fg.as_color())
                        .bg(theme.highlight.selected_bg.as_color())
                } else {
                    Style::default()
                        .fg(theme.foreground.as_color())
                        .bg(theme.bg_element())
                };

                let key_line = Line::from(vec![
                    Span::styled(
                        "Key Name ",
                        Style::default()
                            .fg(theme.section_title())
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(self.auth_key_name.as_str(), Style::default()),
                ]);
                let key_para = Paragraph::new(key_line).style(key_style);
                frame.render_widget(key_para, inner_chunks[0]);

                let value_line = Line::from(vec![
                    Span::styled(
                        "Key Value ",
                        Style::default()
                            .fg(theme.section_title())
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(self.auth_key_value.as_str(), Style::default()),
                ]);
                let value_para = Paragraph::new(value_line).style(value_style);
                frame.render_widget(value_para, inner_chunks[1]);

                if is_focused && self.auth_field_focus == AuthField::KeyName {
                    let prefix = "Key Name ";
                    let x_offset = prefix.chars().count() as u16
                        + self.auth_key_name.as_str()[..self.auth_key_name.cursor_pos]
                            .chars()
                            .count() as u16;
                    frame.set_cursor_position(ratatui::prelude::Position::new(
                        inner_chunks[0].x + x_offset.min(inner_chunks[0].width.saturating_sub(1)),
                        inner_chunks[0].y,
                    ));
                } else if is_focused && self.auth_field_focus == AuthField::KeyValue {
                    let prefix = "Key Value ";
                    let x_offset = prefix.chars().count() as u16
                        + self.auth_key_value.as_str()[..self.auth_key_value.cursor_pos]
                            .chars()
                            .count() as u16;
                    frame.set_cursor_position(ratatui::prelude::Position::new(
                        inner_chunks[1].x + x_offset.min(inner_chunks[1].width.saturating_sub(1)),
                        inner_chunks[1].y,
                    ));
                }
            }
        }
    }

    fn render_params_tab(&self, frame: &mut Frame, area: Rect, theme: &Theme, is_active: bool) {
        let header = Row::new(vec![Cell::from("Key"), Cell::from("Value")]).style(
            Style::default()
                .fg(theme.pane.title.as_color())
                .add_modifier(Modifier::BOLD),
        );

        let rows: Vec<Row> = self
            .params
            .iter()
            .enumerate()
            .map(|(i, (key, value))| {
                let is_selected = is_active
                    && self.focused_field == FocusedField::TabContent
                    && self.param_selected == i;
                let style = if is_selected {
                    Style::default()
                        .bg(theme.highlight.selected_bg.as_color())
                        .fg(theme.highlight.selected_fg.as_color())
                } else {
                    Style::default().fg(theme.foreground.as_color())
                };

                let key_style = if is_selected && self.param_field_focus == ParamField::Key {
                    Style::default()
                        .bg(theme.highlight.selected_bg.as_color())
                        .fg(theme.highlight.selected_fg.as_color())
                        .add_modifier(Modifier::BOLD)
                } else {
                    style
                };

                let value_style = if is_selected && self.param_field_focus == ParamField::Value {
                    Style::default()
                        .bg(theme.highlight.selected_bg.as_color())
                        .fg(theme.highlight.selected_fg.as_color())
                        .add_modifier(Modifier::BOLD)
                } else {
                    style
                };

                Row::new(vec![
                    Cell::from(key.as_str().to_string()).style(key_style),
                    Cell::from(value.as_str().to_string()).style(value_style),
                ])
                .style(style)
            })
            .collect();

        let table = Table::new(
            rows,
            &[Constraint::Percentage(40), Constraint::Percentage(60)],
        )
        .header(header)
        .block(
            Block::default().style(
                Style::default()
                    .bg(theme.pane_bg(is_active))
                    .fg(theme.foreground.as_color()),
            ),
        )
        .row_highlight_style(
            Style::default()
                .bg(theme.highlight.selected_bg.as_color())
                .fg(theme.highlight.selected_fg.as_color()),
        );

        let mut state = ratatui::widgets::TableState::default();
        if is_active && self.focused_field == FocusedField::TabContent {
            state.select(Some(self.param_selected));
        }

        frame.render_stateful_widget(table, area, &mut state);
    }

    fn render_method_popup(&self, frame: &mut Frame, area: Rect, theme: &Theme) {
        let popup_area = centered_rect(30, 30, area);
        frame.render_widget(Clear, popup_area);

        let methods = Method::all();
        let items: Vec<ListItem> = methods.iter().map(|m| ListItem::new(m.as_str())).collect();

        let list = List::new(items)
            .style(Style::default().fg(theme.foreground.as_color()))
            .block(
                Block::default()
                    .title("Select Method")
                    .borders(Borders::ALL)
                    .border_type(theme.tui_border_type())
                    .border_style(Style::default().fg(theme.border.active_color.as_color()))
                    .style(
                        Style::default()
                            .bg(theme.pane.bg_color())
                            .fg(theme.foreground.as_color()),
                    ),
            )
            .highlight_style(
                Style::default()
                    .bg(theme.highlight.selected_bg.as_color())
                    .fg(theme.highlight.selected_fg.as_color())
                    .add_modifier(Modifier::BOLD),
            );

        let mut state = ListState::default();
        if let Some(sel) = self.method_list_state.selected() {
            state.select(Some(sel));
        }

        frame.render_stateful_widget(list, popup_area, &mut state);
    }

    fn render_autocomplete(&self, frame: &mut Frame, url_area: Rect, theme: &Theme) {
        let matches = self.get_url_matches();
        if matches.is_empty() {
            return;
        }

        let popup_area = Rect {
            x: url_area.x + 10,
            y: url_area.y + 3,
            width: url_area.width.saturating_sub(10),
            height: (matches.len() as u16 + 2).min(10),
        };

        frame.render_widget(Clear, popup_area);

        let items: Vec<ListItem> = matches
            .iter()
            .enumerate()
            .map(|(i, m)| {
                if i == self.url_autocomplete_selected {
                    ListItem::new(m.as_str()).style(
                        Style::default()
                            .bg(theme.highlight.selected_bg.as_color())
                            .fg(theme.highlight.selected_fg.as_color()),
                    )
                } else {
                    ListItem::new(m.as_str())
                }
            })
            .collect();

        let list = List::new(items)
            .style(Style::default().fg(theme.foreground.as_color()))
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(theme.border.active_color.as_color()))
                    .style(
                        Style::default()
                            .bg(theme.pane.bg_color())
                            .fg(theme.foreground.as_color()),
                    ),
            );

        frame.render_widget(list, popup_area);
    }
}

fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}

impl Default for RequestPane {
    fn default() -> Self {
        Self::new()
    }
}

fn normalize_single_line(content: &str) -> &str {
    content.trim_end_matches(['\r', '\n'])
}

fn parse_auth_type(content: &str) -> Result<AuthType, EditorError> {
    match content.trim().to_ascii_lowercase().as_str() {
        "none" => Ok(AuthType::None),
        "basic" => Ok(AuthType::Basic),
        "bearer" => Ok(AuthType::Bearer),
        "api key" | "apikey" | "api_key" => Ok(AuthType::ApiKey),
        other => Err(EditorError::Validation(format!(
            "Unknown auth type '{other}'"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_backspace_empty_url_buffer_returns_true() {
        let mut pane = RequestPane::new();
        pane.url_buffer = InputBuffer::new();
        pane.focused_field = FocusedField::Url;

        let result = pane.handle_key(KeyCode::Backspace, KeyModifiers::NONE);

        assert!(result);
        assert_eq!(pane.focused_field, FocusedField::Url);
    }

    #[test]
    fn test_left_arrow_at_start_moves_to_method() {
        let mut pane = RequestPane::new();
        pane.url_buffer = InputBuffer::with_content("hello");
        pane.url_buffer.cursor_pos = 0;
        pane.focused_field = FocusedField::Url;

        pane.handle_key(KeyCode::Left, KeyModifiers::NONE);

        assert_eq!(pane.focused_field, FocusedField::Method);
    }

    #[test]
    fn test_up_arrow_empty_url_moves_to_method() {
        let mut pane = RequestPane::new();
        pane.url_buffer = InputBuffer::new();
        pane.focused_field = FocusedField::Url;

        pane.handle_key(KeyCode::Up, KeyModifiers::NONE);

        assert_eq!(pane.focused_field, FocusedField::Method);
    }

    #[test]
    fn test_request_pane_new() {
        let pane = RequestPane::new();
        assert_eq!(pane.method(), Method::Get);
        assert!(pane.url().is_empty());
        assert_eq!(pane.focused_field(), FocusedField::Url);
    }

    #[test]
    fn test_request_pane_with_method() {
        let pane = RequestPane::new().with_method(Method::Post);
        assert_eq!(pane.method(), Method::Post);
    }

    #[test]
    fn test_request_pane_with_url() {
        let pane = RequestPane::new().with_url("https://example.com");
        assert_eq!(pane.url(), "https://example.com");
    }

    #[test]
    fn test_request_pane_headers() {
        let mut headers = Headers::new();
        let _ = headers.set("Content-Type", "application/json");
        let pane = RequestPane::new().with_headers(headers);
        let retrieved = pane.headers();
        assert_eq!(retrieved.get("Content-Type"), Some("application/json"));
    }

    #[test]
    fn test_request_pane_body() {
        let pane = RequestPane::new().with_body(RequestBody::Raw("test body".to_string()));
        let body = pane.body();
        assert_eq!(body, RequestBody::Raw("test body".to_string()));
    }

    #[test]
    fn test_request_tab_all() {
        let tabs = RequestTab::all();
        assert_eq!(tabs.len(), 4);
        assert!(matches!(tabs[0], RequestTab::Headers));
        assert!(matches!(tabs[3], RequestTab::Params));
    }

    #[test]
    fn test_request_tab_as_str() {
        assert_eq!(RequestTab::Headers.as_str(), "Headers");
        assert_eq!(RequestTab::Body.as_str(), "Body");
        assert_eq!(RequestTab::Auth.as_str(), "Auth");
        assert_eq!(RequestTab::Params.as_str(), "Params");
    }

    #[test]
    fn test_focused_field_set_get() {
        let mut pane = RequestPane::new();
        pane.set_focused_field(FocusedField::Method);
        assert_eq!(pane.focused_field(), FocusedField::Method);

        pane.set_focused_field(FocusedField::Tabs);
        assert_eq!(pane.focused_field(), FocusedField::Tabs);
    }

    #[test]
    fn test_body_type_as_str() {
        assert_eq!(BodyType::Raw.as_str(), "Raw");
        assert_eq!(BodyType::Json.as_str(), "JSON");
        assert_eq!(BodyType::Form.as_str(), "Form");
    }

    #[test]
    fn test_auth_type_as_str() {
        assert_eq!(AuthType::None.as_str(), "None");
        assert_eq!(AuthType::Basic.as_str(), "Basic");
        assert_eq!(AuthType::Bearer.as_str(), "Bearer");
        assert_eq!(AuthType::ApiKey.as_str(), "API Key");
    }

    #[test]
    fn test_auth_type_all() {
        let types = AuthType::all();
        assert_eq!(types.len(), 4);
    }

    #[test]
    fn test_request_pane_url_history() {
        let history = vec![
            "https://example.com".to_string(),
            "https://google.com".to_string(),
        ];
        let pane = RequestPane::new().with_url_history(history);
        assert_eq!(pane.url_history.len(), 2);
    }

    #[test]
    fn test_request_pane_default_headers() {
        let pane = RequestPane::new();
        assert_eq!(pane.headers.len(), 1);
    }

    #[test]
    fn test_editor_spec_uses_expected_formats() {
        let pane = RequestPane::new()
            .with_url("https://example.com")
            .with_headers({
                let mut headers = Headers::new();
                let _ = headers.set("Content-Type", "application/json");
                headers
            })
            .with_body(RequestBody::Json(serde_json::json!({"ok": true})));

        assert_eq!(
            pane.editor_spec_for(EditableField::Url).unwrap().format,
            EditorFormat::Text
        );
        assert_eq!(
            pane.editor_spec_for(EditableField::Headers).unwrap().format,
            EditorFormat::Yaml
        );
        assert_eq!(
            pane.editor_spec_for(EditableField::Body).unwrap().format,
            EditorFormat::Json
        );
    }

    #[test]
    fn test_apply_edited_url_trims_trailing_newline() {
        let mut pane = RequestPane::new();
        pane.apply_edited_content(EditableField::Url, "https://example.com/path\n")
            .unwrap();
        assert_eq!(pane.url(), "https://example.com/path");
    }

    #[test]
    fn test_apply_edited_json_body_validates() {
        let mut pane = RequestPane::new().with_body(RequestBody::Json(serde_json::json!({})));
        let result = pane.apply_edited_content(EditableField::Body, "{invalid");
        assert!(matches!(result, Err(EditorError::Validation(_))));
    }

    #[test]
    fn test_apply_edited_headers_replaces_collection() {
        let mut pane = RequestPane::new();
        pane.apply_edited_content(
            EditableField::Headers,
            "- name: Accept\n  value: application/json\n- name: X-Test\n  value: true\n",
        )
        .unwrap();

        let headers = pane.headers();
        assert_eq!(headers.get("Accept"), Some("application/json"));
        assert_eq!(headers.get("X-Test"), Some("true"));
        assert_eq!(headers.len(), 2);
    }

    #[test]
    fn test_apply_edited_header_name_validates_name() {
        let mut pane = RequestPane::new();
        let result = pane.apply_edited_content(EditableField::HeaderName(0), "Bad Header");
        assert!(matches!(result, Err(EditorError::Validation(_))));
    }

    #[test]
    fn test_cursor_context_is_preserved_after_external_edit_apply() {
        let mut pane = RequestPane::new();
        pane.selected_tab = 0;
        pane.focused_field = FocusedField::TabContent;
        pane.header_selected = 0;
        pane.header_field_focus = HeaderField::Value;

        let original_focus = pane.focused_field();
        let original_selection = pane.header_selected;
        pane.apply_edited_content(EditableField::HeaderValue(0), "application/json\n")
            .unwrap();

        assert_eq!(pane.focused_field(), original_focus);
        assert_eq!(pane.header_selected, original_selection);
    }

    #[test]
    fn test_focused_editable_field_tracks_header_value_context() {
        let mut pane = RequestPane::new();
        pane.selected_tab = 0;
        pane.focused_field = FocusedField::TabContent;
        pane.header_selected = 0;
        pane.header_field_focus = HeaderField::Value;

        assert_eq!(pane.focused_editable_field(), EditableField::HeaderValue(0));
    }

    #[test]
    fn test_request_pane_default_params() {
        let pane = RequestPane::new();
        assert_eq!(pane.params.len(), 1);
    }

    #[test]
    fn test_handle_method_key_enter() {
        let mut pane = RequestPane::new();
        pane.set_focused_field(FocusedField::Method);
        let result = pane.handle_key(KeyCode::Enter, KeyModifiers::NONE);
        assert!(result);
        assert!(pane.method_popup_visible);
    }

    #[test]
    fn test_handle_method_key_right() {
        let mut pane = RequestPane::new();
        pane.set_focused_field(FocusedField::Method);
        let result = pane.handle_key(KeyCode::Right, KeyModifiers::NONE);
        assert!(result);
        assert_eq!(pane.focused_field(), FocusedField::Url);
    }

    #[test]
    fn test_handle_url_key_left() {
        let mut pane = RequestPane::new();
        pane.set_focused_field(FocusedField::Url);
        let result = pane.handle_key(KeyCode::Left, KeyModifiers::NONE);
        assert!(result);
        assert_eq!(pane.focused_field(), FocusedField::Method);
    }

    #[test]
    fn test_handle_url_key_tab() {
        let mut pane = RequestPane::new();
        pane.set_focused_field(FocusedField::Url);
        let result = pane.handle_key(KeyCode::Tab, KeyModifiers::NONE);
        assert!(result);
        assert_eq!(pane.focused_field(), FocusedField::Tabs);
    }

    #[test]
    fn test_handle_url_typing() {
        let mut pane = RequestPane::new();
        pane.set_focused_field(FocusedField::Url);
        pane.handle_key(KeyCode::Char('h'), KeyModifiers::NONE);
        assert_eq!(pane.url(), "h");
        pane.handle_key(KeyCode::Char('i'), KeyModifiers::NONE);
        assert_eq!(pane.url(), "hi");
    }

    #[test]
    fn test_handle_url_backspace() {
        let mut pane = RequestPane::new();
        pane.set_focused_field(FocusedField::Url);
        pane.handle_key(KeyCode::Char('h'), KeyModifiers::NONE);
        pane.handle_key(KeyCode::Char('i'), KeyModifiers::NONE);
        pane.handle_key(KeyCode::Backspace, KeyModifiers::NONE);
        assert_eq!(pane.url(), "h");
    }

    #[test]
    fn test_url_focus_handles_printable_keys() {
        let mut pane = RequestPane::new();
        pane.set_focused_field(FocusedField::Url);

        assert!(pane.handle_key(KeyCode::Char('p'), KeyModifiers::NONE));
        assert_eq!(pane.url(), "p");
    }

    #[test]
    fn test_paste_text_inserts_into_url_buffer() {
        let mut pane = RequestPane::new();
        pane.set_focused_field(FocusedField::Url);

        assert!(pane.paste_text("https://example.com"));
        assert_eq!(pane.url(), "https://example.com");
        assert!(pane.url_autocomplete_visible);
    }

    #[test]
    fn test_handle_tabs_key_left_from_first() {
        let mut pane = RequestPane::new();
        pane.set_focused_field(FocusedField::Tabs);
        pane.selected_tab = 0;
        let result = pane.handle_key(KeyCode::Left, KeyModifiers::NONE);
        assert!(result);
        assert_eq!(pane.focused_field(), FocusedField::Url);
    }

    #[test]
    fn test_handle_tabs_key_right_to_content() {
        let mut pane = RequestPane::new();
        pane.set_focused_field(FocusedField::Tabs);
        pane.selected_tab = RequestTab::all().len() - 1;
        let result = pane.handle_key(KeyCode::Right, KeyModifiers::NONE);
        assert!(result);
        assert_eq!(pane.focused_field(), FocusedField::TabContent);
    }

    #[test]
    fn test_handle_tabs_key_down_to_content() {
        let mut pane = RequestPane::new();
        pane.set_focused_field(FocusedField::Tabs);
        let result = pane.handle_key(KeyCode::Down, KeyModifiers::NONE);
        assert!(result);
        assert_eq!(pane.focused_field(), FocusedField::TabContent);
    }

    #[test]
    fn test_handle_tab_content_key_up_to_tabs() {
        let mut pane = RequestPane::new();
        pane.set_focused_field(FocusedField::TabContent);
        let result = pane.handle_key(KeyCode::Up, KeyModifiers::NONE);
        assert!(result);
        assert_eq!(pane.focused_field(), FocusedField::Tabs);
    }

    #[test]
    fn test_handle_headers_add_delete() {
        let mut pane = RequestPane::new();
        pane.set_focused_field(FocusedField::TabContent);
        pane.selected_tab = 0; // Headers tab

        // Add a header
        pane.handle_key(KeyCode::Char('a'), KeyModifiers::NONE);
        assert_eq!(pane.headers.len(), 2);

        // Delete the added header
        pane.handle_key(KeyCode::Char('d'), KeyModifiers::NONE);
        assert_eq!(pane.headers.len(), 1);
    }

    #[test]
    fn test_handle_headers_edit() {
        let mut pane = RequestPane::new();
        pane.set_focused_field(FocusedField::TabContent);
        pane.selected_tab = 0; // Headers tab

        // Edit header name
        pane.handle_key(KeyCode::Char('C'), KeyModifiers::NONE);
        assert!(pane.headers[0].0.as_str().contains('C'));

        // Switch to value field
        pane.handle_key(KeyCode::Right, KeyModifiers::NONE);

        // Edit header value
        pane.handle_key(KeyCode::Char('v'), KeyModifiers::NONE);
        assert!(pane.headers[0].1.as_str().contains('v'));
    }

    #[test]
    fn test_handle_body_edit() {
        let mut pane = RequestPane::new();
        pane.set_focused_field(FocusedField::TabContent);
        pane.selected_tab = 1; // Body tab

        // Type in body
        pane.handle_key(KeyCode::Char('h'), KeyModifiers::NONE);
        pane.handle_key(KeyCode::Char('i'), KeyModifiers::NONE);
        assert!(pane.body_content.as_str().contains("hi"));
    }

    #[test]
    fn test_handle_body_type_change() {
        let mut pane = RequestPane::new();
        pane.set_focused_field(FocusedField::TabContent);
        pane.selected_tab = 1; // Body tab

        assert_eq!(pane.body_type, BodyType::Raw);
        pane.handle_key(KeyCode::Char('t'), KeyModifiers::NONE);
        assert_eq!(pane.body_type, BodyType::Json);
    }

    #[test]
    fn test_handle_auth_type_change() {
        let mut pane = RequestPane::new();
        pane.set_focused_field(FocusedField::TabContent);
        pane.selected_tab = 2; // Auth tab

        assert_eq!(pane.auth_type, AuthType::None);
        pane.handle_key(KeyCode::Char('t'), KeyModifiers::NONE);
        assert_eq!(pane.auth_type, AuthType::Basic);
    }

    #[test]
    fn test_handle_auth_basic_edit() {
        let mut pane = RequestPane::new();
        pane.set_focused_field(FocusedField::TabContent);
        pane.selected_tab = 2; // Auth tab
        pane.auth_type = AuthType::Basic;

        // Move to username field
        pane.handle_key(KeyCode::Right, KeyModifiers::NONE);
        pane.handle_key(KeyCode::Char('u'), KeyModifiers::NONE);
        assert!(pane.auth_username.as_str().contains('u'));

        // Move to password field
        pane.handle_key(KeyCode::Down, KeyModifiers::NONE);
        pane.handle_key(KeyCode::Char('p'), KeyModifiers::NONE);
        assert!(pane.auth_password.as_str().contains('p'));
    }

    #[test]
    fn test_handle_params_add_delete() {
        let mut pane = RequestPane::new();
        pane.set_focused_field(FocusedField::TabContent);
        pane.selected_tab = 3; // Params tab

        // Add a param
        pane.handle_key(KeyCode::Char('a'), KeyModifiers::NONE);
        assert_eq!(pane.params.len(), 2);

        // Delete the added param
        pane.handle_key(KeyCode::Char('d'), KeyModifiers::NONE);
        assert_eq!(pane.params.len(), 1);
    }

    #[test]
    fn test_handle_params_edit() {
        let mut pane = RequestPane::new();
        pane.set_focused_field(FocusedField::TabContent);
        pane.selected_tab = 3; // Params tab

        // Edit param key
        pane.handle_key(KeyCode::Char('k'), KeyModifiers::NONE);
        assert!(pane.params[0].0.as_str().contains('k'));

        // Switch to value field
        pane.handle_key(KeyCode::Right, KeyModifiers::NONE);

        // Edit param value
        pane.handle_key(KeyCode::Char('v'), KeyModifiers::NONE);
        assert!(pane.params[0].1.as_str().contains('v'));
    }

    #[test]
    fn test_method_popup_navigation() {
        let mut pane = RequestPane::new();
        pane.method_popup_visible = true;
        pane.method_list_state.select(Some(0));

        pane.handle_key(KeyCode::Down, KeyModifiers::NONE);
        assert_eq!(pane.method_list_state.selected(), Some(1));

        pane.handle_key(KeyCode::Up, KeyModifiers::NONE);
        assert_eq!(pane.method_list_state.selected(), Some(0));
    }

    #[test]
    fn test_method_popup_enter_selects() {
        let mut pane = RequestPane::new();
        pane.method = Method::Get;
        pane.method_popup_visible = true;
        pane.method_list_state.select(Some(1)); // POST

        pane.handle_key(KeyCode::Enter, KeyModifiers::NONE);
        assert!(!pane.method_popup_visible);
        assert_eq!(pane.method(), Method::Post);
    }

    #[test]
    fn test_method_popup_esc_cancels() {
        let mut pane = RequestPane::new();
        pane.method_popup_visible = true;

        pane.handle_key(KeyCode::Esc, KeyModifiers::NONE);
        assert!(!pane.method_popup_visible);
    }

    #[test]
    fn test_url_autocomplete() {
        let mut pane = RequestPane::new();
        pane.url_history = vec![
            "https://example.com".to_string(),
            "https://google.com".to_string(),
            "https://github.com".to_string(),
        ];
        pane.url_buffer = InputBuffer::with_content("https://e");
        pane.url_autocomplete_visible = true;

        let matches = pane.get_url_matches();
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0], "https://example.com");

        pane.handle_key(KeyCode::Enter, KeyModifiers::NONE);
        assert!(!pane.url_autocomplete_visible);
        assert_eq!(pane.url(), "https://example.com");
    }

    #[test]
    fn test_url_autocomplete_navigation() {
        let mut pane = RequestPane::new();
        pane.url_history = vec![
            "https://example.com".to_string(),
            "https://google.com".to_string(),
        ];
        pane.url_buffer = InputBuffer::with_content("https://");
        pane.url_autocomplete_visible = true;
        pane.url_autocomplete_selected = 0;

        pane.handle_key(KeyCode::Down, KeyModifiers::NONE);
        assert!(pane.url_autocomplete_selected <= 1);
    }

    #[test]
    fn test_layout_renders() {
        let pane = RequestPane::new();
        assert_eq!(pane.focused_field(), FocusedField::Url);
    }

    #[test]
    fn test_to_request_adds_query_params_and_auth() {
        let pane = RequestPane::new()
            .with_url("https://example.com/api")
            .with_method(Method::Get);
        let mut pane = pane;
        pane.auth_type = AuthType::Bearer;
        pane.auth_token = InputBuffer::with_content("token123");
        pane.params[0].0 = InputBuffer::with_content("page");
        pane.params[0].1 = InputBuffer::with_content("1");

        let request = pane.to_request(15).unwrap();

        assert_eq!(request.url.as_str(), "https://example.com/api?page=1");
        assert_eq!(
            request.headers.get("Authorization"),
            Some("Bearer token123")
        );
        assert_eq!(request.timeout_secs, 15);
    }

    #[test]
    fn test_to_request_omits_empty_raw_body() {
        let pane = RequestPane::new().with_url("https://example.com");
        let request = pane.to_request(30).unwrap();

        assert_eq!(request.body, RequestBody::None);
        assert_eq!(request.headers.get("Content-Type"), None);
    }

    // Issue 3: Method Dropdown - Task 3.1
    #[test]
    fn test_enter_with_method_popup_selects_and_closes() {
        let mut pane = RequestPane::new();
        pane.method_popup_visible = true;
        pane.method_list_state.select(Some(1)); // POST is at index 1
        pane.focused_field = FocusedField::Method;

        let result = pane.handle_key(KeyCode::Enter, KeyModifiers::NONE);

        assert!(result);
        assert!(!pane.method_popup_visible);
        assert_eq!(pane.method, Method::Post);
    }

    // Task 3.2
    #[test]
    fn test_method_popup_blocks_other_key_handling() {
        let mut pane = RequestPane::new();
        pane.method_popup_visible = true;
        pane.focused_field = FocusedField::Method;

        // Left arrow should NOT move focus while popup is open
        let result = pane.handle_key(KeyCode::Left, KeyModifiers::NONE);

        assert!(result); // consumed
        assert_eq!(pane.focused_field, FocusedField::Method); // didn't change
    }

    // Task 3.3
    #[test]
    fn test_up_down_navigates_method_list() {
        let mut pane = RequestPane::new();
        pane.method_popup_visible = true;
        pane.method_list_state.select(Some(0));

        pane.handle_key(KeyCode::Down, KeyModifiers::NONE);
        assert_eq!(pane.method_list_state.selected(), Some(1));

        pane.handle_key(KeyCode::Up, KeyModifiers::NONE);
        assert_eq!(pane.method_list_state.selected(), Some(0));
    }

    // Task 9.2
    #[test]
    fn test_ctrl_f_opens_search() {
        let mut pane = RequestPane::new();
        pane.focused_field = FocusedField::TabContent;

        let result = pane.handle_key(KeyCode::Char('f'), KeyModifiers::CONTROL);

        assert!(pane.search_visible);
        assert!(result);
    }
}
