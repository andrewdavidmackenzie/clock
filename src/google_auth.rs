use oauth2::{
    AuthUrl, AuthorizationCode, ClientId, ClientSecret, CsrfToken, PkceCodeChallenge,
    PkceCodeVerifier, RedirectUrl, Scope, TokenResponse, TokenUrl, AuthType,
    basic::BasicClient,
    reqwest::http_client,
};
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::path::PathBuf;

const GOOGLE_AUTH_URL: &str = "https://accounts.google.com/o/oauth2/v2/auth";
const GOOGLE_TOKEN_URL: &str = "https://oauth2.googleapis.com/token";
const GOOGLE_USERINFO_URL: &str = "https://www.googleapis.com/oauth2/v2/userinfo";
const GOOGLE_CALENDAR_EVENTS_URL: &str = "https://www.googleapis.com/calendar/v3/calendars/primary/events";
const REDIRECT_PORT: u16 = 8085;

// OAuth credentials for the clock app
// To create your own: Google Cloud Console -> APIs & Services -> Credentials -> Create OAuth client ID -> Desktop app
const CLIENT_ID: &str = "95536384409-hrd7nebrgggunk7ccbc7nvsji4qr3vo3.apps.googleusercontent.com";
const CLIENT_SECRET: &str = "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";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserInfo {
    pub id: String,
    pub email: String,
    pub name: String,
    pub picture: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CalendarEvent {
    pub summary: Option<String>,
    pub start: Option<EventTime>,
    pub end: Option<EventTime>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventTime {
    #[serde(rename = "dateTime")]
    pub date_time: Option<String>,
    pub date: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CalendarEventsResponse {
    items: Option<Vec<CalendarEvent>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct StoredTokens {
    access_token: String,
    refresh_token: Option<String>,
}

#[derive(Debug, Clone)]
pub struct GoogleAuth;

impl GoogleAuth {
    pub fn new() -> Option<Self> {
        if CLIENT_ID.is_empty() {
            None
        } else {
            Some(Self)
        }
    }

    fn get_token_path() -> PathBuf {
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("clock")
            .join("google_tokens.json")
    }

    fn create_client(&self) -> BasicClient {
        BasicClient::new(
            ClientId::new(CLIENT_ID.to_string()),
            Some(ClientSecret::new(CLIENT_SECRET.to_string())),
            AuthUrl::new(GOOGLE_AUTH_URL.to_string()).unwrap(),
            Some(TokenUrl::new(GOOGLE_TOKEN_URL.to_string()).unwrap()),
        )
        .set_redirect_uri(
            RedirectUrl::new(format!("http://127.0.0.1:{}", REDIRECT_PORT)).unwrap(),
        )
        .set_auth_type(AuthType::RequestBody)
    }

    pub fn start_login(&self) -> Result<(String, PkceCodeVerifier, CsrfToken), String> {
        let client = self.create_client();

        let (pkce_challenge, pkce_verifier) = PkceCodeChallenge::new_random_sha256();

        let (auth_url, csrf_token) = client
            .authorize_url(CsrfToken::new_random)
            .add_scope(Scope::new("openid".to_string()))
            .add_scope(Scope::new("email".to_string()))
            .add_scope(Scope::new("profile".to_string()))
            .add_scope(Scope::new("https://www.googleapis.com/auth/calendar.readonly".to_string()))
            .add_extra_param("access_type", "offline")
            .add_extra_param("prompt", "consent")
            .set_pkce_challenge(pkce_challenge)
            .url();

        Ok((auth_url.to_string(), pkce_verifier, csrf_token))
    }

    pub fn wait_for_callback(&self, pkce_verifier: PkceCodeVerifier, expected_state: CsrfToken) -> Result<String, String> {
        use std::time::{Duration, Instant};

        let listener = TcpListener::bind(format!("127.0.0.1:{}", REDIRECT_PORT))
            .map_err(|e| format!("Failed to bind to port {}: {}", REDIRECT_PORT, e))?;

        // Set non-blocking with timeout
        listener.set_nonblocking(true)
            .map_err(|e| format!("Failed to set non-blocking: {}", e))?;

        let timeout = Duration::from_secs(120);
        let start = Instant::now();

        let mut stream = loop {
            match listener.accept() {
                Ok((stream, _)) => break stream,
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    if start.elapsed() > timeout {
                        return Err("Login timed out - no response received within 2 minutes".to_string());
                    }
                    std::thread::sleep(Duration::from_millis(100));
                }
                Err(e) => return Err(format!("Failed to accept connection: {}", e)),
            }
        };

        let mut reader = BufReader::new(&stream);
        let mut request_line = String::new();
        reader
            .read_line(&mut request_line)
            .map_err(|e| format!("Failed to read request: {}", e))?;

        // Parse query parameters from the callback URL
        let query_string = request_line
            .split_whitespace()
            .nth(1)
            .and_then(|path| path.split('?').nth(1))
            .ok_or("Failed to parse callback URL")?;

        let mut code = None;
        let mut state = None;

        for param in query_string.split('&') {
            let mut parts = param.split('=');
            match parts.next() {
                Some("code") => code = parts.next().map(|s| s.to_string()),
                Some("state") => state = parts.next().map(|s| s.to_string()),
                _ => {}
            }
        }

        let code = code.ok_or("Failed to extract authorization code")?;

        // Validate CSRF state
        let state = state.ok_or("Missing state parameter in callback")?;
        if state != *expected_state.secret() {
            return Err("Invalid state parameter - possible CSRF attack".to_string());
        }

        // Send response to browser
        let response = "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\n\r\n\
            <html><body><h1>Login successful!</h1><p>You can close this window.</p></body></html>";
        stream.write_all(response.as_bytes()).ok();

        // Exchange code for token
        let client = self.create_client();
        let token_result = client
            .exchange_code(AuthorizationCode::new(code))
            .set_pkce_verifier(pkce_verifier)
            .request(http_client)
            .map_err(|e| format!("Token exchange failed: {:?}", e))?;

        let access_token = token_result.access_token().secret().clone();
        let refresh_token = token_result.refresh_token().map(|t| t.secret().clone());

        // Save tokens
        let tokens = StoredTokens {
            access_token: access_token.clone(),
            refresh_token,
        };
        self.save_tokens(&tokens)?;

        Ok(access_token)
    }

    fn save_tokens(&self, tokens: &StoredTokens) -> Result<(), String> {
        let path = Self::get_token_path();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create config directory: {}", e))?;
        }
        let json = serde_json::to_string_pretty(tokens)
            .map_err(|e| format!("Failed to serialize tokens: {}", e))?;
        fs::write(&path, json)
            .map_err(|e| format!("Failed to write tokens: {}", e))?;
        Ok(())
    }

    fn load_stored_tokens(&self) -> Option<StoredTokens> {
        let path = Self::get_token_path();
        let content = fs::read_to_string(path).ok()?;
        serde_json::from_str(&content).ok()
    }

    /// Try to get a valid access token, refreshing if necessary
    pub fn get_valid_access_token(&self) -> Option<String> {
        let tokens = self.load_stored_tokens()?;

        // First try the stored access token
        if self.is_token_valid(&tokens.access_token) {
            return Some(tokens.access_token);
        }

        // Access token expired, try to refresh
        if let Some(refresh_token) = tokens.refresh_token {
            if let Ok(new_access_token) = self.refresh_access_token(&refresh_token) {
                return Some(new_access_token);
            }
        }

        None
    }

    fn is_token_valid(&self, access_token: &str) -> bool {
        // Quick validation by trying to fetch user info
        self.get_user_info(access_token).is_ok()
    }

    fn refresh_access_token(&self, refresh_token: &str) -> Result<String, String> {
        use oauth2::RefreshToken;

        let client = self.create_client();
        let token_result = client
            .exchange_refresh_token(&RefreshToken::new(refresh_token.to_string()))
            .request(http_client)
            .map_err(|e| format!("Token refresh failed: {:?}", e))?;

        let new_access_token = token_result.access_token().secret().clone();
        let new_refresh_token = token_result
            .refresh_token()
            .map(|t| t.secret().clone())
            .or_else(|| Some(refresh_token.to_string())); // Keep old refresh token if not returned

        // Save updated tokens
        let tokens = StoredTokens {
            access_token: new_access_token.clone(),
            refresh_token: new_refresh_token,
        };
        self.save_tokens(&tokens)?;

        Ok(new_access_token)
    }

    pub fn clear_tokens(&self) -> Result<(), String> {
        let path = Self::get_token_path();
        if path.exists() {
            fs::remove_file(&path)
                .map_err(|e| format!("Failed to remove tokens: {}", e))?;
        }
        Ok(())
    }

    pub fn get_user_info(&self, access_token: &str) -> Result<UserInfo, String> {
        let client = reqwest::blocking::Client::new();
        let response = client
            .get(GOOGLE_USERINFO_URL)
            .bearer_auth(access_token)
            .send()
            .map_err(|e| format!("Failed to get user info: {}", e))?;

        if !response.status().is_success() {
            return Err(format!("User info request failed: {}", response.status()));
        }

        response
            .json::<UserInfo>()
            .map_err(|e| format!("Failed to parse user info: {}", e))
    }

    pub fn get_next_calendar_event(&self, access_token: &str) -> Result<Option<CalendarEvent>, String> {
        let client = reqwest::blocking::Client::new();

        let now = chrono::Utc::now().to_rfc3339();

        let response = client
            .get(GOOGLE_CALENDAR_EVENTS_URL)
            .bearer_auth(access_token)
            .query(&[
                ("maxResults", "1"),
                ("orderBy", "startTime"),
                ("singleEvents", "true"),
                ("timeMin", &now),
            ])
            .send()
            .map_err(|e| format!("Failed to get calendar events: {}", e))?;

        if !response.status().is_success() {
            return Err(format!("Calendar request failed: {}", response.status()));
        }

        let events_response: CalendarEventsResponse = response
            .json()
            .map_err(|e| format!("Failed to parse calendar response: {}", e))?;

        Ok(events_response.items.and_then(|items| items.into_iter().next()))
    }
}
