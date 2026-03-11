use oauth2::{
    AuthUrl, AuthorizationCode, ClientId, ClientSecret, CsrfToken, PkceCodeChallenge,
    PkceCodeVerifier, RedirectUrl, Scope, TokenResponse, TokenUrl,
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
pub struct GoogleAuth {
    client_id: String,
    client_secret: String,
}

impl GoogleAuth {
    pub fn new() -> Option<Self> {
        // Try to load credentials from environment or config file
        let client_id = std::env::var("GOOGLE_CLIENT_ID").ok()?;
        let client_secret = std::env::var("GOOGLE_CLIENT_SECRET").ok()?;

        Some(Self {
            client_id,
            client_secret,
        })
    }

    fn get_token_path() -> PathBuf {
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("clock")
            .join("google_tokens.json")
    }

    fn create_client(&self) -> BasicClient {
        BasicClient::new(
            ClientId::new(self.client_id.clone()),
            Some(ClientSecret::new(self.client_secret.clone())),
            AuthUrl::new(GOOGLE_AUTH_URL.to_string()).unwrap(),
            Some(TokenUrl::new(GOOGLE_TOKEN_URL.to_string()).unwrap()),
        )
        .set_redirect_uri(
            RedirectUrl::new(format!("http://localhost:{}", REDIRECT_PORT)).unwrap(),
        )
    }

    pub fn start_login(&self) -> Result<(String, PkceCodeVerifier), String> {
        let client = self.create_client();

        let (pkce_challenge, pkce_verifier) = PkceCodeChallenge::new_random_sha256();

        let (auth_url, _csrf_token) = client
            .authorize_url(CsrfToken::new_random)
            .add_scope(Scope::new("openid".to_string()))
            .add_scope(Scope::new("email".to_string()))
            .add_scope(Scope::new("profile".to_string()))
            .add_scope(Scope::new("https://www.googleapis.com/auth/calendar.readonly".to_string()))
            .set_pkce_challenge(pkce_challenge)
            .url();

        Ok((auth_url.to_string(), pkce_verifier))
    }

    pub fn wait_for_callback(&self, pkce_verifier: PkceCodeVerifier) -> Result<String, String> {
        let listener = TcpListener::bind(format!("127.0.0.1:{}", REDIRECT_PORT))
            .map_err(|e| format!("Failed to bind to port {}: {}", REDIRECT_PORT, e))?;

        println!("Waiting for OAuth callback on port {}...", REDIRECT_PORT);

        let (mut stream, _) = listener
            .accept()
            .map_err(|e| format!("Failed to accept connection: {}", e))?;

        let mut reader = BufReader::new(&stream);
        let mut request_line = String::new();
        reader
            .read_line(&mut request_line)
            .map_err(|e| format!("Failed to read request: {}", e))?;

        // Parse the authorization code from the callback URL
        let code = request_line
            .split_whitespace()
            .nth(1)
            .and_then(|path| {
                path.split('?')
                    .nth(1)
                    .and_then(|query| {
                        query.split('&').find_map(|param| {
                            let mut parts = param.split('=');
                            if parts.next() == Some("code") {
                                parts.next().map(|s| s.to_string())
                            } else {
                                None
                            }
                        })
                    })
            })
            .ok_or("Failed to extract authorization code")?;

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

    pub fn load_tokens(&self) -> Option<String> {
        let path = Self::get_token_path();
        let content = fs::read_to_string(path).ok()?;
        let tokens: StoredTokens = serde_json::from_str(&content).ok()?;
        Some(tokens.access_token)
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
