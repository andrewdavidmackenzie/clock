use iced::{mouse, window, Task};
use iced::widget::canvas::{stroke, Cache, Geometry, LineCap, Path, Stroke};
use iced::widget::{canvas, container, image};
use iced::{
    Color, Element, Length, Point, Rectangle, Renderer,
    Subscription, Theme, Vector,
};
use chrono::prelude::*;
use chrono::Local;
use std::f32::consts::PI;

mod google_auth;
use google_auth::{CalendarEvent, GoogleAuth, UserInfo};

/// Fetch avatar image from URL asynchronously
async fn fetch_avatar(url: String) -> Option<image::Handle> {
    tokio::task::spawn_blocking(move || {
        let client = reqwest::blocking::Client::new();
        match client.get(&url).send() {
            Ok(response) if response.status().is_success() => {
                match response.bytes() {
                    Ok(bytes) => Some(image::Handle::from_bytes(bytes.to_vec())),
                    Err(_) => None,
                }
            }
            _ => None,
        }
    }).await.ok().flatten()
}

/// Fetch upcoming calendar events asynchronously
async fn fetch_events(auth: GoogleAuth) -> Vec<CalendarEvent> {
    tokio::task::spawn_blocking(move || {
        auth.get_valid_access_token()
            .and_then(|token| auth.get_upcoming_events(&token).ok())
            .unwrap_or_default()
    }).await.unwrap_or_default()
}

/// Convert a time to angle on a 12-hour clock (radians from 12 o'clock, clockwise)
fn time_to_angle(hour: u32, minute: u32) -> f32 {
    let hour_12 = (hour % 12) as f32;
    let fraction = hour_12 + (minute as f32 / 60.0);
    2.0 * PI * fraction / 12.0
}

/// Parse ISO8601 datetime string to local DateTime
fn parse_event_time(time_str: &str) -> Option<DateTime<Local>> {
    // Try parsing with timezone
    if let Ok(dt) = DateTime::parse_from_rfc3339(time_str) {
        return Some(dt.with_timezone(&Local));
    }
    // Try parsing date only (all-day events)
    if let Ok(date) = chrono::NaiveDate::parse_from_str(time_str, "%Y-%m-%d") {
        return Some(Local.from_local_datetime(&date.and_hms_opt(0, 0, 0)?).single()?);
    }
    None
}

/// Generate a color for an event based on its index
fn event_color(index: usize) -> Color {
    let colors = [
        Color::from_rgb8(76, 175, 80),   // Green
        Color::from_rgb8(255, 152, 0),   // Orange
        Color::from_rgb8(156, 39, 176),  // Purple
        Color::from_rgb8(233, 30, 99),   // Pink
        Color::from_rgb8(0, 188, 212),   // Cyan
        Color::from_rgb8(255, 235, 59),  // Yellow
    ];
    colors[index % colors.len()]
}

const CENTER_BUTTON_RADIUS: f32 = 0.06;
const EXIT_BUTTON_WIDTH: f32 = 120.0;
const EXIT_BUTTON_HEIGHT: f32 = 36.0;
const EXIT_BUTTON_Y_OFFSET: f32 = 40.0;
const LOGIN_BUTTON_WIDTH: f32 = 180.0;
const LOGIN_BUTTON_HEIGHT: f32 = 36.0;
const LOGIN_BUTTON_Y_OFFSET: f32 = -5.0;
const MODAL_WIDTH: f32 = 280.0;
const MODAL_HEIGHT: f32 = 240.0;
const HOUR_HAND_RADIUS: f32 = 0.6;
const MINUTE_HAND_RADIUS: f32 = 0.78;
const SECOND_HAND_RADIUS: f32 = 0.83;
const CLOCK_FACE_RADIUS: f32 = 0.88;

const TICK_OUTER_RADIUS: f32 = 0.85;
const HOUR_TICK_INNER_RADIUS: f32 = 0.76;
const QUARTER_TICK_INNER_RADIUS: f32 = 0.71;

// Event arc constants - drawn outside the clock face (thin band)
const EVENT_ARC_INNER_RADIUS: f32 = 0.90;
const EVENT_ARC_OUTER_RADIUS: f32 = 0.98;

const CENTER_BUTTON_REGION : CircularRegion = { CircularRegion {
    inner_radius: 0.0,
    outer_radius: CENTER_BUTTON_RADIUS
} };

const CLOCK_FACE_REGION : CircularRegion = { CircularRegion {
    inner_radius: CENTER_BUTTON_RADIUS,
    outer_radius: CLOCK_FACE_RADIUS,
} };

/// Calculate the top-left origin of the exit button given the center point
fn exit_button_origin(center: Point) -> Point {
    Point::new(
        center.x - EXIT_BUTTON_WIDTH / 2.0,
        center.y - EXIT_BUTTON_HEIGHT / 2.0 + EXIT_BUTTON_Y_OFFSET,
    )
}

/// Check if a position is within the exit button bounds
fn exit_button_contains(center: Point, position: Point) -> bool {
    let origin = exit_button_origin(center);
    position.x >= origin.x
        && position.x <= origin.x + EXIT_BUTTON_WIDTH
        && position.y >= origin.y
        && position.y <= origin.y + EXIT_BUTTON_HEIGHT
}

/// Calculate the top-left origin of the login button given the center point
fn login_button_origin(center: Point) -> Point {
    Point::new(
        center.x - LOGIN_BUTTON_WIDTH / 2.0,
        center.y - LOGIN_BUTTON_HEIGHT / 2.0 + LOGIN_BUTTON_Y_OFFSET,
    )
}

/// Check if a position is within the login button bounds
fn login_button_contains(center: Point, position: Point) -> bool {
    let origin = login_button_origin(center);
    position.x >= origin.x
        && position.x <= origin.x + LOGIN_BUTTON_WIDTH
        && position.y >= origin.y
        && position.y <= origin.y + LOGIN_BUTTON_HEIGHT
}

fn main() -> iced::Result {
    let window_settings = window::Settings {
        resizable: false,
        decorations: false,
        fullscreen: true,
        ..window::Settings::default()
    };

    iced::application(Clock::new, Clock::update, Clock::view)
        .subscription(Clock::subscription)
        .antialiasing(true)
        .window(window_settings)
        .run()
}


struct Clock {
    now: DateTime<Local>,
    clock: Cache,
    menu_open: bool,
    google_auth: Option<GoogleAuth>,
    user_info: Option<UserInfo>,
    avatar: Option<image::Handle>,
    upcoming_events: Vec<CalendarEvent>,
    login_in_progress: bool,
}

/// Messages handled by the [Clock] Application
#[derive(Debug, Clone)]
enum ClockMessage {
    Tick(DateTime<Local>),
    CenterClick,
    ExitClick,
    LoginClick,
    LogoutClick,
    LoginComplete(Result<UserInfo, String>),
    SessionRestored(Option<UserInfo>),
    AvatarLoaded(Option<image::Handle>),
    EventsLoaded(Vec<CalendarEvent>),
    Click {
        start_region: ClickRegion,
        end_region: ClickRegion,
        start_time: f32,
        end_time: f32,
    },
}

fn hours_and_minutes(time_float: f32) -> (u8, u8) {
    let hours = time_float as u8;
    let minutes = ((time_float - hours as f32) * 60.0) as u8;
    // Display 12 instead of 0 for 12 o'clock
    (if hours == 0 { 12 } else { hours }, minutes)
}

/// Determine if clicked time should be AM or PM based on next occurrence after current time
fn next_occurrence_period(time_float: f32, current_time: &DateTime<Local>) -> &'static str {
    let clicked_hour = time_float as u8; // 0-11 internally
    let clicked_minutes = ((time_float - clicked_hour as f32) * 60.0) as u8;

    let current_hour = current_time.hour() as u8; // 0-23
    let current_minutes = current_time.minute() as u8;

    // Convert to minutes since midnight for comparison
    let current_total_mins = current_hour as u16 * 60 + current_minutes as u16;

    // AM candidate: clicked_hour (0-11) represents 12AM, 1AM, ..., 11AM
    let am_total_mins = clicked_hour as u16 * 60 + clicked_minutes as u16;
    // PM candidate: clicked_hour + 12 (12-23) represents 12PM, 1PM, ..., 11PM
    let pm_total_mins = (clicked_hour as u16 + 12) * 60 + clicked_minutes as u16;

    // Find which comes next after current time
    if pm_total_mins > current_total_mins {
        "PM"
    } else if am_total_mins > current_total_mins {
        "AM"
    } else {
        // Both have passed today, so AM tomorrow is next
        "AM"
    }
}

impl Clock {
    fn new() -> (Self, Task<ClockMessage>) {
        let google_auth = GoogleAuth::new();

        // Create the task to restore session asynchronously (don't block first frame)
        let restore_task = if let Some(auth) = google_auth.clone() {
            Task::perform(
                async move {
                    tokio::task::spawn_blocking(move || {
                        auth.get_valid_access_token().and_then(|token| {
                            match auth.get_user_info(&token) {
                                Ok(info) => {
                                    // Also fetch and print the next calendar event
                                    if let Ok(Some(event)) = auth.get_next_calendar_event(&token) {
                                        if let Some(summary) = &event.summary {
                                            let time = event.start
                                                .as_ref()
                                                .and_then(|s| s.date_time.as_ref().or(s.date.as_ref()))
                                                .map(|s| s.as_str())
                                                .unwrap_or("unknown time");
                                            println!("Next calendar event: {} at {}", summary, time);
                                        }
                                    }
                                    Some(info)
                                }
                                Err(_) => None,
                            }
                        })
                    }).await.unwrap_or(None)
                },
                ClockMessage::SessionRestored,
            )
        } else {
            Task::none()
        };

        (
            Clock {
                now: Local::now(),
                clock: Default::default(),
                menu_open: false,
                google_auth,
                user_info: None,
                avatar: None,
                upcoming_events: Vec::new(),
                login_in_progress: false,
            },
            restore_task
        )
    }
    
    fn update(&mut self, message: ClockMessage) -> Task<ClockMessage> {
        match message {
            ClockMessage::Tick(local_time) => {
                let now = local_time;

                if now != self.now {
                    self.now = now;
                    self.clock.clear();
                }
            }
            ClockMessage::CenterClick => {
                self.menu_open = !self.menu_open;
                self.clock.clear();
            }
            ClockMessage::ExitClick => {
                return iced::exit();
            }
            ClockMessage::LoginClick => {
                if let Some(auth) = &self.google_auth {
                    self.login_in_progress = true;
                    self.clock.clear();

                    let auth = auth.clone();
                    return Task::perform(
                        async move {
                            // This runs the OAuth flow in a blocking manner
                            tokio::task::spawn_blocking(move || {
                                match auth.start_login() {
                                    Ok((auth_url, pkce_verifier, csrf_token)) => {
                                        // Open browser for user to authenticate
                                        if webbrowser::open(&auth_url).is_err() {
                                            return Err("Failed to open browser".to_string());
                                        }

                                        // Wait for callback and exchange code for token
                                        match auth.wait_for_callback(pkce_verifier, csrf_token) {
                                            Ok(access_token) => {
                                                // Get user info
                                                match auth.get_user_info(&access_token) {
                                                    Ok(user_info) => {
                                                        // Print next calendar event
                                                        if let Ok(Some(event)) = auth.get_next_calendar_event(&access_token) {
                                                            if let Some(summary) = &event.summary {
                                                                let time = event.start
                                                                    .as_ref()
                                                                    .and_then(|s| s.date_time.as_ref().or(s.date.as_ref()))
                                                                    .map(|s| s.as_str())
                                                                    .unwrap_or("unknown time");
                                                                println!("Next calendar event: {} at {}", summary, time);
                                                            }
                                                        }
                                                        Ok(user_info)
                                                    }
                                                    Err(e) => Err(e),
                                                }
                                            }
                                            Err(e) => Err(e),
                                        }
                                    }
                                    Err(e) => Err(e),
                                }
                            }).await.unwrap_or_else(|e| Err(format!("Task failed: {:?}", e)))
                        },
                        ClockMessage::LoginComplete,
                    );
                }
            }
            ClockMessage::LogoutClick => {
                if let Some(auth) = &self.google_auth {
                    if let Err(e) = auth.clear_tokens() {
                        eprintln!("Warning: Failed to clear tokens: {}", e);
                    }
                }
                self.user_info = None;
                self.avatar = None;
                self.upcoming_events.clear();
                self.clock.clear();
            }
            ClockMessage::LoginComplete(result) => {
                self.login_in_progress = false;
                match result {
                    Ok(user_info) => {
                        println!("Logged in as: {}", user_info.name);
                        let avatar_url = user_info.picture.clone();
                        self.user_info = Some(user_info);
                        self.menu_open = false; // Close modal on successful login
                        self.clock.clear();

                        let mut tasks = Vec::new();
                        // Fetch avatar if available
                        if let Some(url) = avatar_url {
                            tasks.push(Task::perform(fetch_avatar(url), ClockMessage::AvatarLoaded));
                        }
                        // Fetch upcoming events
                        if let Some(auth) = self.google_auth.clone() {
                            tasks.push(Task::perform(fetch_events(auth), ClockMessage::EventsLoaded));
                        }
                        if !tasks.is_empty() {
                            return Task::batch(tasks);
                        }
                    }
                    Err(e) => {
                        eprintln!("Login failed: {}", e);
                        self.clock.clear();
                    }
                }
            }
            ClockMessage::SessionRestored(user_info) => {
                if let Some(info) = user_info {
                    println!("Session restored for: {}", info.name);
                    let avatar_url = info.picture.clone();
                    self.user_info = Some(info);
                    self.clock.clear();

                    let mut tasks = Vec::new();
                    // Fetch avatar if available
                    if let Some(url) = avatar_url {
                        tasks.push(Task::perform(fetch_avatar(url), ClockMessage::AvatarLoaded));
                    }
                    // Fetch upcoming events
                    if let Some(auth) = self.google_auth.clone() {
                        tasks.push(Task::perform(fetch_events(auth), ClockMessage::EventsLoaded));
                    }
                    if !tasks.is_empty() {
                        return Task::batch(tasks);
                    }
                }
            }
            ClockMessage::AvatarLoaded(handle) => {
                if let Some(h) = handle {
                    self.avatar = Some(h);
                    self.clock.clear();
                }
            }
            ClockMessage::EventsLoaded(events) => {
                println!("Loaded {} upcoming events", events.len());
                for event in &events {
                    if let Some(summary) = &event.summary {
                        println!("  - {}", summary);
                    }
                }
                self.upcoming_events = events;
                self.clock.clear();
            }
            ClockMessage::Click { start_region, end_region, start_time, end_time } => {
                let (start_h, start_m) = hours_and_minutes(start_time);
                let (end_h, end_m) = hours_and_minutes(end_time);
                let start_period = next_occurrence_period(start_time, &self.now);
                let end_period = next_occurrence_period(end_time, &self.now);
                let drag_type = match (start_region, end_region) {
                    (ClickRegion::Face, ClickRegion::Face) => "Face",
                    (ClickRegion::Outer, ClickRegion::Outer) => "Outer",
                    (ClickRegion::Face, ClickRegion::Outer) => "Drag-Out",
                    (ClickRegion::Outer, ClickRegion::Face) => "Drag-In",
                };
                println!("{} {:02}:{:02} {} - {:02}:{:02} {}",
                    drag_type, start_h, start_m, start_period, end_h, end_m, end_period);
            }
        }

        Task::none()
    }

    fn view(&self) -> Element<'_, ClockMessage> {
        let canvas = canvas(self)
            .width(Length::Fill)
            .height(Length::Fill);

        container(canvas)
            .width(Length::Fill)
            .height(Length::Fill)
            .padding(20)
            .into()
    }

    fn subscription(&self) -> Subscription<ClockMessage> {
        iced::time::every(std::time::Duration::from_secs(1)).map(|_| {
            ClockMessage::Tick(Local::now())
        })
    }
}

// A Circular region for click detection. We detect clicks
// - within a circle (from 0.0 out to the circle radius)
// - from one radius out to another radius (a donut)
// - from a radius out to infinity
// These are defined as fractions of the radius of the displayed clock, so they can be constants
// and will be multiplied by the clock radius at runtime - as it scales to fit window
struct CircularRegion {
    inner_radius: f32,
    outer_radius: f32,
}

impl CircularRegion {
    fn contains(&self, position: f32) -> bool {
        (position >= self.inner_radius) && (position < self.outer_radius)
    }
}

/// State for the canvas program to track dragging
#[derive(Default)]
struct ClockState {
    /// Current cursor position for tooltip display
    cursor_info: Option<CursorInfo>,
    /// When mouse is pressed, stores drag start info
    dragging: Option<DragState>,
    /// Track if exit button is being pressed (for release-to-activate pattern)
    exit_button_pressed: bool,
    /// Track if login/logout button is being pressed
    login_button_pressed: bool,
    /// Event being hovered over (name and cursor position)
    hovered_event: Option<(String, Point)>,
}

#[derive(Clone, Copy)]
struct CursorInfo {
    position: Point,
    time_float: f32,
}

#[derive(Clone, Copy)]
struct DragState {
    start_region: ClickRegion,
    start_time: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum ClickRegion {
    Face,
    Outer,
}

impl canvas::Program<ClockMessage> for Clock {
    type State = ClockState;

    fn update(
        &self,
        state: &mut Self::State,
        event: &iced::Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Option<canvas::Action<ClockMessage>> {
        match event {
            iced::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                if let Some(position) = cursor.position_in(bounds) {
                    // Use frame-relative center (position_in returns frame-relative coords)
                    let center = Point::new(bounds.width / 2.0, bounds.height / 2.0);
                    let radius = bounds.width.min(bounds.height) / 2.0;
                    let cursor_radius = center.distance(position) / radius;

                    // Check if menu is open and handle button clicks
                    if self.menu_open {
                        if exit_button_contains(center, position) {
                            // Track press, exit triggers on release (allows drag-away to cancel)
                            state.exit_button_pressed = true;
                            return Some(canvas::Action::request_redraw());
                        }
                        // Only arm login button if auth is configured and not in progress
                        if login_button_contains(center, position)
                            && self.google_auth.is_some()
                            && !self.login_in_progress
                        {
                            // Track press for login/logout button
                            state.login_button_pressed = true;
                            return Some(canvas::Action::request_redraw());
                        }
                        // Click outside buttons closes menu
                        return Some(canvas::Action::publish(ClockMessage::CenterClick));
                    }

                    if CENTER_BUTTON_REGION.contains(cursor_radius) {
                        Some(canvas::Action::publish(ClockMessage::CenterClick))
                    } else {
                        let time_float = unit_from_position(center, position, 12);
                        let start_region = if CLOCK_FACE_REGION.contains(cursor_radius) {
                            ClickRegion::Face
                        } else {
                            ClickRegion::Outer
                        };
                        state.dragging = Some(DragState {
                            start_region,
                            start_time: time_float,
                        });
                        state.cursor_info = Some(CursorInfo { position, time_float });
                        Some(canvas::Action::request_redraw())
                    }
                } else {
                    None
                }
            }
            iced::Event::Mouse(mouse::Event::CursorMoved { .. }) => {
                // Suppress hover/tooltip when menu is open
                if self.menu_open {
                    state.cursor_info = None;
                    state.dragging = None;
                    state.hovered_event = None;
                    return Some(canvas::Action::request_redraw());
                }

                if let Some(position) = cursor.position_in(bounds) {
                    // Use frame-relative center (position_in returns frame-relative coords)
                    let center = Point::new(bounds.width / 2.0, bounds.height / 2.0);
                    let radius = bounds.width.min(bounds.height) / 2.0;
                    let cursor_radius = center.distance(position) / radius;
                    let time_float = unit_from_position(center, position, 12);

                    // Check if hovering over an event arc
                    state.hovered_event = None;
                    if cursor_radius >= EVENT_ARC_INNER_RADIUS && cursor_radius <= EVENT_ARC_OUTER_RADIUS {
                        let cursor_angle = time_to_angle(
                            (time_float as u32) % 12,
                            ((time_float.fract()) * 60.0) as u32,
                        );

                        // Check each event
                        let now = self.now;
                        for event in &self.upcoming_events {
                            let start_time = event.start.as_ref().and_then(|s| {
                                s.date_time.as_ref().or(s.date.as_ref()).and_then(|t| parse_event_time(t))
                            });
                            let end_time = event.end.as_ref().and_then(|e| {
                                e.date_time.as_ref().or(e.date.as_ref()).and_then(|t| parse_event_time(t))
                            });

                            if let (Some(start), Some(end)) = (start_time, end_time) {
                                let now_plus_12 = now + chrono::Duration::hours(12);
                                if start > now_plus_12 || end < now {
                                    continue;
                                }

                                let display_start = if start < now { now } else { start };
                                let display_end = if end > now_plus_12 { now_plus_12 } else { end };

                                let start_angle = time_to_angle(display_start.hour(), display_start.minute());
                                let end_angle = time_to_angle(display_end.hour(), display_end.minute());

                                let (start_a, end_a) = if end_angle < start_angle {
                                    (start_angle, end_angle + 2.0 * PI)
                                } else {
                                    (start_angle, end_angle)
                                };

                                // Check if cursor angle is within this event's arc
                                let cursor_a = if cursor_angle < start_a { cursor_angle + 2.0 * PI } else { cursor_angle };
                                if cursor_a >= start_a && cursor_a <= end_a {
                                    if let Some(name) = &event.summary {
                                        state.hovered_event = Some((name.clone(), position));
                                        break;
                                    }
                                }
                            }
                        }
                    }

                    // Keep tracking during drag, or show tooltip outside center button
                    if state.dragging.is_some() || !CENTER_BUTTON_REGION.contains(cursor_radius) {
                        state.cursor_info = Some(CursorInfo {
                            position,
                            time_float,
                        });
                        Some(canvas::Action::request_redraw())
                    } else {
                        state.cursor_info = None;
                        Some(canvas::Action::request_redraw())
                    }
                } else {
                    state.cursor_info = None;
                    state.hovered_event = None;
                    Some(canvas::Action::request_redraw())
                }
            }
            iced::Event::Mouse(mouse::Event::CursorLeft) => {
                state.cursor_info = None;
                state.hovered_event = None;
                state.exit_button_pressed = false;
                state.login_button_pressed = false;
                Some(canvas::Action::request_redraw())
            }
            iced::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                // Handle button releases when menu is open
                if self.menu_open {
                    state.dragging = None;
                    if state.exit_button_pressed {
                        state.exit_button_pressed = false;
                        // Check if still inside button on release
                        if let Some(position) = cursor.position_in(bounds) {
                            let center = Point::new(bounds.width / 2.0, bounds.height / 2.0);
                            if exit_button_contains(center, position) {
                                return Some(canvas::Action::publish(ClockMessage::ExitClick));
                            }
                        }
                    }
                    if state.login_button_pressed {
                        state.login_button_pressed = false;
                        // Check if still inside button on release and auth is configured
                        if let Some(position) = cursor.position_in(bounds) {
                            let center = Point::new(bounds.width / 2.0, bounds.height / 2.0);
                            if login_button_contains(center, position) && self.google_auth.is_some() {
                                // Determine if this is login or logout based on current state
                                let message = if self.user_info.is_some() {
                                    ClockMessage::LogoutClick
                                } else {
                                    ClockMessage::LoginClick
                                };
                                return Some(canvas::Action::publish(message));
                            }
                        }
                    }
                    return None;
                }

                if let Some(drag_state) = state.dragging.take() {
                    if let Some(position) = cursor.position_in(bounds) {
                        // Use frame-relative center
                        let center = Point::new(bounds.width / 2.0, bounds.height / 2.0);
                        let radius = bounds.width.min(bounds.height) / 2.0;
                        let cursor_radius = center.distance(position) / radius;

                        let end_region = if CLOCK_FACE_REGION.contains(cursor_radius) {
                            ClickRegion::Face
                        } else {
                            ClickRegion::Outer
                        };
                        let message = ClockMessage::Click {
                            start_region: drag_state.start_region,
                            end_region,
                            start_time: drag_state.start_time,
                            end_time: unit_from_position(center, position, 12),
                        };
                        Some(canvas::Action::publish(message))
                    } else {
                        None
                    }
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    fn draw(
        &self,
        state: &Self::State,
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<Geometry> {
        let clock = self.clock.draw(renderer, bounds.size(), |frame| {
            let center = frame.center();
            frame.translate(Vector::new(center.x, center.y));

            let radius = frame.width().min(frame.height()) / 2.0;

            let background = Path::circle(Point::ORIGIN, radius * CLOCK_FACE_RADIUS);
            frame.fill(&background, Color::from_rgb8(0x12, 0x93, 0xD8));

            // Draw hour ticks around the clock face
            let tick_stroke = Stroke {
                width: radius / 40.0,
                style: stroke::Style::Solid(Color::WHITE),
                line_cap: LineCap::Round,
                ..Stroke::default()
            };

            for hour in 0..12 {
                let inner_radius = if hour % 3 == 0 {
                    QUARTER_TICK_INNER_RADIUS
                } else {
                    HOUR_TICK_INNER_RADIUS
                };

                frame.with_save(|frame| {
                    frame.rotate(2.0 * PI * hour as f32 / 12.0);
                    let tick = Path::line(
                        Point::new(0.0, -(inner_radius * radius)),
                        Point::new(0.0, -(TICK_OUTER_RADIUS * radius)),
                    );
                    frame.stroke(&tick, tick_stroke);
                });
            }

            // Draw event arcs around the perimeter
            let now = self.now;
            for (index, event) in self.upcoming_events.iter().enumerate() {
                // Parse start and end times
                let start_time = event.start.as_ref().and_then(|s| {
                    s.date_time.as_ref().or(s.date.as_ref()).and_then(|t| parse_event_time(t))
                });
                let end_time = event.end.as_ref().and_then(|e| {
                    e.date_time.as_ref().or(e.date.as_ref()).and_then(|t| parse_event_time(t))
                });

                if let (Some(start), Some(end)) = (start_time, end_time) {
                    // Only draw events that are within the visible 12-hour window
                    let now_plus_12 = now + chrono::Duration::hours(12);
                    if start > now_plus_12 || end < now {
                        continue;
                    }

                    // Clamp start to now if event already started
                    let display_start = if start < now { now } else { start };
                    let display_end = if end > now_plus_12 { now_plus_12 } else { end };

                    let start_angle = time_to_angle(display_start.hour(), display_start.minute());
                    let end_angle = time_to_angle(display_end.hour(), display_end.minute());

                    // Handle wrap-around (e.g., 11pm to 1am)
                    let (start_a, end_a) = if end_angle < start_angle {
                        (start_angle, end_angle + 2.0 * PI)
                    } else {
                        (start_angle, end_angle)
                    };

                    // Draw arc using path builder
                    let color = event_color(index);
                    let inner_r = radius * EVENT_ARC_INNER_RADIUS;
                    let outer_r = radius * EVENT_ARC_OUTER_RADIUS;

                    // Build arc path as a filled region between two arcs
                    let arc_path = Path::new(|builder| {
                        // Start at inner arc beginning
                        let start_inner = Point::new(
                            inner_r * start_a.sin(),
                            -inner_r * start_a.cos(),
                        );
                        builder.move_to(start_inner);

                        // Arc along inner radius
                        let steps = ((end_a - start_a) * 20.0) as usize + 1;
                        for i in 0..=steps {
                            let angle = start_a + (end_a - start_a) * (i as f32 / steps as f32);
                            let point = Point::new(
                                inner_r * angle.sin(),
                                -inner_r * angle.cos(),
                            );
                            builder.line_to(point);
                        }

                        // Line to outer arc end
                        let end_outer = Point::new(
                            outer_r * end_a.sin(),
                            -outer_r * end_a.cos(),
                        );
                        builder.line_to(end_outer);

                        // Arc back along outer radius
                        for i in (0..=steps).rev() {
                            let angle = start_a + (end_a - start_a) * (i as f32 / steps as f32);
                            let point = Point::new(
                                outer_r * angle.sin(),
                                -outer_r * angle.cos(),
                            );
                            builder.line_to(point);
                        }

                        builder.close();
                    });

                    frame.fill(&arc_path, color);

                    // Draw event name curved along the arc
                    if let Some(name) = &event.summary {
                        let text_radius = radius * (EVENT_ARC_INNER_RADIUS + EVENT_ARC_OUTER_RADIUS) / 2.0;
                        let arc_span = end_a - start_a;
                        let font_size = 12.0;
                        let char_width = font_size * 0.6; // Approximate character width

                        // Calculate how many characters fit
                        let arc_length = arc_span * text_radius;
                        let max_chars = (arc_length / char_width) as usize;

                        // Truncate name if needed
                        let display_name: String = if name.len() > max_chars && max_chars > 1 {
                            format!("{}…", &name[..max_chars.saturating_sub(1).min(name.len())])
                        } else {
                            name.clone()
                        };

                        // Calculate angle span for text
                        let text_len = display_name.chars().count();
                        let total_text_angle = (text_len as f32 * char_width) / text_radius;
                        let text_start_angle = (start_a + end_a) / 2.0 - total_text_angle / 2.0;

                        // Draw each character along the arc
                        for (i, ch) in display_name.chars().enumerate() {
                            let char_angle = text_start_angle + (i as f32 + 0.5) * char_width / text_radius;
                            let char_pos = Point::new(
                                text_radius * char_angle.sin(),
                                -text_radius * char_angle.cos(),
                            );

                            frame.with_save(|frame| {
                                frame.translate(Vector::new(char_pos.x, char_pos.y));
                                frame.rotate(char_angle);
                                frame.fill_text(canvas::Text {
                                    content: ch.to_string(),
                                    position: Point::ORIGIN,
                                    color: Color::WHITE,
                                    size: iced::Pixels(font_size),
                                    align_x: iced::alignment::Horizontal::Center.into(),
                                    align_y: iced::alignment::Vertical::Center.into(),
                                    ..canvas::Text::default()
                                });
                            });
                        }
                    }
                }
            }

            let hour_hand =
                Path::line(Point::ORIGIN, Point::new(0.0, - (HOUR_HAND_RADIUS * radius)));

            let hour_width = || -> Stroke {
                Stroke {
                    width: radius / 15.0,
                    style: stroke::Style::Solid(Color::WHITE),
                    line_cap: LineCap::Round,
                    ..Stroke::default()
                }
            };

            frame.with_save(|frame| {
                let time = self.now.naive_local().time();
                let hour = (time.hour() % 12) as f32;
                let minute = time.minute() as f32;
                let second = time.second() as f32;
                let hour_with_minutes = hour + (minute / 60.0) + (second / 3_600.0);
                frame.rotate(2.0 * PI * hour_with_minutes / 12.0);
                frame.stroke(&hour_hand, hour_width());
            });

            let minute_hand =
                Path::line(Point::ORIGIN, Point::new(0.0, -(MINUTE_HAND_RADIUS * radius)));

            let minute_width = || -> Stroke {
                Stroke {
                    width: radius / 30.0,
                    style: stroke::Style::Solid(Color::WHITE),
                    line_cap: LineCap::Round,
                    ..Stroke::default()
                }
            };

            frame.with_save(|frame| {
                let time = self.now.naive_local().time();
                let minute_with_seconds = time.minute() as f32 + (time.second() as f32 / 60.0);
                frame.rotate(2.0 * PI * minute_with_seconds / 60.0);
                frame.stroke(&minute_hand, minute_width());
            });

            let second_hand =
                Path::line(Point::ORIGIN, Point::new(0.0, -(SECOND_HAND_RADIUS * radius)));

            let second_width = || -> Stroke {
                Stroke {
                    width: radius / 200.0,
                    style: stroke::Style::Solid(Color::WHITE),
                    line_cap: LineCap::Round,
                    ..Stroke::default()
                }
            };

            frame.with_save(|frame| {
                frame.rotate(hand_rotation(self.now.naive_local().time().second() as u8, 60));
                frame.stroke(&second_hand, second_width());
            });

            let center = Path::circle(Point::ORIGIN, radius * CENTER_BUTTON_RADIUS);
            frame.fill(&center, Color::from_rgb8(0x92, 0x93, 0xD8));
        });

        let mut geometries = vec![clock];

        // Draw tooltip when cursor is over face or outer regions (but not when hovering an event)
        if state.hovered_event.is_none() {
        if let Some(cursor_info) = &state.cursor_info {
            let (hours, minutes) = hours_and_minutes(cursor_info.time_float);
            let period = next_occurrence_period(cursor_info.time_float, &self.now);
            let time_text = format!("{:02}:{:02} {}", hours, minutes, period);

            let tooltip = canvas::Cache::default().draw(renderer, bounds.size(), |frame| {
                let font_size = 16.0;
                let padding = 4.0;
                let text_width = 92.0; // Width for "HH:MM AM" tooltip (wider for SW rendering)
                let text_height = font_size;

                // Position tooltip near cursor with offset
                let tooltip_x = cursor_info.position.x + 15.0;
                let tooltip_y = cursor_info.position.y - 10.0;

                // Draw rounded rectangle background
                let bg_rect = Path::rounded_rectangle(
                    Point::new(tooltip_x - padding, tooltip_y - padding),
                    iced::Size::new(text_width + padding * 2.0, text_height + padding * 2.0),
                    4.0.into(),
                );
                frame.fill(&bg_rect, Color::from_rgba8(0, 0, 0, 0.8));

                frame.fill_text(canvas::Text {
                    content: time_text,
                    position: Point::new(tooltip_x, tooltip_y),
                    color: Color::WHITE,
                    size: iced::Pixels(font_size),
                    ..canvas::Text::default()
                });
            });
            geometries.push(tooltip);
        }
        }

        // Draw event hover tooltip
        if let Some((event_name, position)) = &state.hovered_event {
            let event_tooltip = canvas::Cache::default().draw(renderer, bounds.size(), |frame| {
                let font_size = 16.0;
                let padding = 8.0;
                let char_width = font_size * 0.6;
                let text_width = (event_name.len() as f32 * char_width).max(100.0);
                let text_height = font_size;

                // Position tooltip near cursor with offset
                let tooltip_x = position.x + 15.0;
                let tooltip_y = position.y - 10.0;

                // Draw rounded rectangle background
                let bg_rect = Path::rounded_rectangle(
                    Point::new(tooltip_x - padding, tooltip_y - padding),
                    iced::Size::new(text_width + padding * 2.0, text_height + padding * 2.0),
                    6.0.into(),
                );
                frame.fill(&bg_rect, Color::from_rgba8(0, 0, 0, 0.9));

                frame.fill_text(canvas::Text {
                    content: event_name.clone(),
                    position: Point::new(tooltip_x, tooltip_y),
                    color: Color::WHITE,
                    size: iced::Pixels(font_size),
                    ..canvas::Text::default()
                });
            });
            geometries.push(event_tooltip);
        }

        // Draw menu popup if open
        if self.menu_open {
            let user_info = self.user_info.clone();
            let avatar = self.avatar.clone();
            let login_in_progress = self.login_in_progress;
            let has_google_auth = self.google_auth.is_some();

            let menu = canvas::Cache::default().draw(renderer, bounds.size(), |frame| {
                let center = frame.center();

                // Modal dimensions
                let modal_x = center.x - MODAL_WIDTH / 2.0;
                let modal_y = center.y - MODAL_HEIGHT / 2.0;

                // Draw modal background with rounded corners
                let modal_bg = Path::rounded_rectangle(
                    Point::new(modal_x, modal_y),
                    iced::Size::new(MODAL_WIDTH, MODAL_HEIGHT),
                    12.0.into(),
                );
                frame.fill(&modal_bg, Color::from_rgba8(40, 40, 40, 0.9));

                // Draw border
                frame.stroke(&modal_bg, Stroke {
                    width: 2.0,
                    style: stroke::Style::Solid(Color::from_rgb8(100, 100, 100)),
                    ..Stroke::default()
                });

                // Login/Logout section
                let login_origin = login_button_origin(center);

                if let Some(info) = &user_info {
                    // User is logged in - show avatar, name and logout button
                    let avatar_size = 56.0;
                    let avatar_x = center.x - avatar_size / 2.0;
                    let avatar_y = login_origin.y - 95.0;

                    // Draw avatar if available, otherwise draw placeholder circle
                    if let Some(ref handle) = avatar {
                        frame.draw_image(
                            Rectangle::new(
                                Point::new(avatar_x, avatar_y),
                                iced::Size::new(avatar_size, avatar_size),
                            ),
                            handle,
                        );
                    } else {
                        // Placeholder circle for avatar
                        let avatar_circle = Path::circle(
                            Point::new(center.x, avatar_y + avatar_size / 2.0),
                            avatar_size / 2.0,
                        );
                        frame.fill(&avatar_circle, Color::from_rgb8(80, 80, 80));
                    }

                    // Draw name below avatar
                    frame.fill_text(canvas::Text {
                        content: info.name.clone(),
                        position: Point::new(center.x - 60.0, login_origin.y - 30.0),
                        color: Color::WHITE,
                        size: iced::Pixels(14.0),
                        ..canvas::Text::default()
                    });

                    // Draw Logout button
                    let logout_bg = Path::rounded_rectangle(
                        login_origin,
                        iced::Size::new(LOGIN_BUTTON_WIDTH, LOGIN_BUTTON_HEIGHT),
                        6.0.into(),
                    );
                    frame.fill(&logout_bg, Color::from_rgb8(100, 100, 100));

                    frame.fill_text(canvas::Text {
                        content: String::from("Logout"),
                        position: Point::new(center.x - 28.0, login_origin.y + 10.0),
                        color: Color::WHITE,
                        size: iced::Pixels(18.0),
                        ..canvas::Text::default()
                    });
                } else if has_google_auth {
                    // User is not logged in - show login button
                    let button_color = if login_in_progress {
                        Color::from_rgb8(80, 80, 80) // Dimmed while in progress
                    } else {
                        Color::from_rgb8(66, 133, 244) // Google blue
                    };

                    let login_bg = Path::rounded_rectangle(
                        login_origin,
                        iced::Size::new(LOGIN_BUTTON_WIDTH, LOGIN_BUTTON_HEIGHT),
                        6.0.into(),
                    );
                    frame.fill(&login_bg, button_color);

                    let button_text = if login_in_progress {
                        "Logging in..."
                    } else {
                        "Login with Google"
                    };

                    frame.fill_text(canvas::Text {
                        content: String::from(button_text),
                        position: Point::new(center.x - 65.0, login_origin.y + 10.0),
                        color: Color::WHITE,
                        size: iced::Pixels(16.0),
                        ..canvas::Text::default()
                    });
                } else {
                    // No Google auth configured
                    frame.fill_text(canvas::Text {
                        content: String::from("Google auth not configured"),
                        position: Point::new(center.x - 95.0, login_origin.y + 5.0),
                        color: Color::from_rgb8(150, 150, 150),
                        size: iced::Pixels(12.0),
                        ..canvas::Text::default()
                    });
                }

                // Exit button
                let exit_origin = exit_button_origin(center);

                // Draw Exit button background
                let exit_bg = Path::rounded_rectangle(
                    exit_origin,
                    iced::Size::new(EXIT_BUTTON_WIDTH, EXIT_BUTTON_HEIGHT),
                    6.0.into(),
                );
                frame.fill(&exit_bg, Color::from_rgb8(180, 60, 60));

                // Draw Exit button text
                frame.fill_text(canvas::Text {
                    content: String::from("Exit"),
                    position: Point::new(center.x - 18.0, exit_origin.y + 10.0),
                    color: Color::WHITE,
                    size: iced::Pixels(18.0),
                    ..canvas::Text::default()
                });
            });
            geometries.push(menu);
        }

        geometries
    }

    fn mouse_interaction(
        &self,
        state: &Self::State,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> mouse::Interaction {
        match cursor.position_in(bounds) {
            Some(position) => {
                // Use frame-relative center
                let center = Point::new(bounds.width / 2.0, bounds.height / 2.0);
                let radius = bounds.width.min(bounds.height) / 2.0;
                let cursor_radius = center.distance(position) / radius;

                // Check if hovering over buttons when menu is open
                if self.menu_open {
                    if exit_button_contains(center, position) {
                        return mouse::Interaction::Pointer;
                    }
                    if login_button_contains(center, position) && self.google_auth.is_some() && !self.login_in_progress {
                        return mouse::Interaction::Pointer;
                    }
                    return mouse::Interaction::default();
                }

                if state.dragging.is_some() {
                    // Arrow/pointer while dragging (takes priority)
                    mouse::Interaction::Pointer
                } else if CENTER_BUTTON_REGION.contains(cursor_radius) {
                    mouse::Interaction::Crosshair
                } else {
                    // Crosshair when hovering over face or outer areas
                    mouse::Interaction::Crosshair
                }
            },
            None => mouse::Interaction::default(),
        }
    }
}

// Calculate the unit (hour, minute, second) from a position relative to the center
// Zero is at the top dead center
fn unit_from_position(center: Point, position: Point, total: u8) -> f32 {
    let relative_x = position.x - center.x;
    let relative_y = -(position.y - center.y);
    let div = relative_y / relative_x;
    let mut angle = div.atan();
    if relative_x < 0.0 {
        angle += PI;
    }
    let angle = ((2.5 * PI) - angle) % (2.0 * PI);
    let rotation_percent = angle / (2.0 * PI);
    (total as f32 * rotation_percent * 1000.0).round() / 1000.0
}

// Calculate an angle (in radians) from a count over a total possible
// e.g. 30 (minutes) over a total of 60 (minutes) is 50% of 360 degrees, or 180 degrees
fn hand_rotation(count: u8, total: u8) -> f32 {
    let rotation_percent = count as f32 / total as f32;
    2.0 * PI * rotation_percent
}

#[cfg(test)]
mod test {
    use iced::Point;
    use super::unit_from_position;

    #[test]
    fn test_unit_0_clock() {
        assert_eq!(unit_from_position(Point::new(100.0,100.0),
                                      Point::new(100.0,0.0),
                                      12), 0.0);
    }

    #[test]
    fn test_unit_3_clock() {
        assert_eq!(unit_from_position(Point::new(100.0,100.0),
                                      Point::new(200.0,100.0),
                                      12), 3.0);
    }

    #[test]
    fn test_unit_4_clock() {
        assert_eq!(unit_from_position(Point::new(100.0,100.0),
                                      Point::new(180.2,146.3),
                                      12), 4.0);
    }

    #[test]
    fn test_unit_6_clock() {
        assert_eq!(unit_from_position(Point::new(100.0,100.0),
                                      Point::new(100.0,200.0),
                                      12), 6.0);
    }

    #[test]
    fn test_unit_7_clock() {
        assert_eq!(unit_from_position(Point::new(100.0,100.0),
                                      Point::new(53.8, 180.0),
                                      12), 7.0);
    }

    #[test]
    fn test_unit_9_clock() {
        assert_eq!(unit_from_position(Point::new(100.0,100.0),
                                      Point::new(0.0,100.0),
                                      12), 9.0);
    }
}