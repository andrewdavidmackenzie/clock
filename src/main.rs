use iced::{mouse, window, Task};
use iced::widget::canvas::{stroke, Cache, Geometry, LineCap, Path, Stroke};
use iced::widget::{canvas, container};
use iced::{
    Color, Element, Length, Point, Rectangle, Renderer,
    Subscription, Theme, Vector,
};
use chrono::prelude::*;
use chrono::Local;
use std::f32::consts::PI;

const CENTER_BUTTON_RADIUS: f32 = 0.07;
const HOUR_HAND_RADIUS: f32 = 0.7;
const MINUTE_HAND_RADIUS: f32 = 0.9;
const SECOND_HAND_RADIUS: f32 = 0.95;
const CLOCK_FACE_RADIUS: f32 = 1.0;

const TICK_OUTER_RADIUS: f32 = 0.95;
const HOUR_TICK_INNER_RADIUS: f32 = 0.85;
const QUARTER_TICK_INNER_RADIUS: f32 = 0.80;

const CENTER_BUTTON_REGION : CircularRegion = { CircularRegion {
    inner_radius: 0.0,
    outer_radius: CENTER_BUTTON_RADIUS
} };

const CLOCK_FACE_REGION : CircularRegion = { CircularRegion {
    inner_radius: CENTER_BUTTON_RADIUS,
    outer_radius: CLOCK_FACE_RADIUS,
} };

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
}

/// Messages handled by the [Clock] Application
#[derive(Debug, Clone, Copy)]
enum ClockMessage {
    Tick(DateTime<Local>),
    CenterClick,
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

impl Clock {
    fn new() -> (Self, Task<ClockMessage>) {
        (
            Clock {
                now: Local::now(),
                clock: Default::default(),
            },
            Task::none()
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
            ClockMessage::CenterClick => {std::process::exit(0)}
            ClockMessage::Click { start_region, end_region, start_time, end_time } => {
                let (start_h, start_m) = hours_and_minutes(start_time);
                let (end_h, end_m) = hours_and_minutes(end_time);
                let drag_type = match (start_region, end_region) {
                    (ClickRegion::Face, ClickRegion::Face) => "Face",
                    (ClickRegion::Outer, ClickRegion::Outer) => "Outer",
                    (ClickRegion::Face, ClickRegion::Outer) => "Drag-Out",
                    (ClickRegion::Outer, ClickRegion::Face) => "Drag-In",
                };
                println!("{} {:02}:{:02} - {:02}:{:02}", drag_type, start_h, start_m, end_h, end_m);
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
                if let Some(position) = cursor.position_in(bounds) {
                    // Use frame-relative center (position_in returns frame-relative coords)
                    let center = Point::new(bounds.width / 2.0, bounds.height / 2.0);
                    let radius = bounds.width.min(bounds.height) / 2.0;
                    let cursor_radius = center.distance(position) / radius;
                    let time_float = unit_from_position(center, position, 12);

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
                    Some(canvas::Action::request_redraw())
                }
            }
            iced::Event::Mouse(mouse::Event::CursorLeft) => {
                state.cursor_info = None;
                Some(canvas::Action::request_redraw())
            }
            iced::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
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

        // Draw tooltip when cursor is over face or outer regions
        if let Some(cursor_info) = &state.cursor_info {
            let tooltip = canvas::Cache::default().draw(renderer, bounds.size(), |frame| {
                let (hours, minutes) = hours_and_minutes(cursor_info.time_float);
                let time_text = format!("{:02}:{:02}", hours, minutes);

                let font_size = 16.0;
                let padding = 4.0;
                let text_width = 42.0; // Approximate width for "HH:MM" at 16px
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