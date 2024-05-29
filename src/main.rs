use iced::{executor, mouse, window};
use iced::widget::canvas::{stroke, Cache, Geometry, LineCap, Path, Stroke, Event, event};
use iced::widget::{canvas, container};
use iced::{
    Application, Color, Command, Element, Length, Point, Rectangle, Renderer, Settings,
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

const CENTER_BUTTON_REGION : CircularRegion = { CircularRegion {
    inner_radius: 0.0,
    outer_radius: CENTER_BUTTON_RADIUS
} };

const CLOCK_FACE_REGION : CircularRegion = { CircularRegion {
    inner_radius: CENTER_BUTTON_RADIUS,
    outer_radius: CLOCK_FACE_RADIUS,
} };

pub fn main() -> iced::Result {
    Clock::run(Settings {
        antialiasing: true,
        window: window::Settings {
            resizable: false,
            decorations: false,
            ..window::Settings::default()
        },
        ..Settings::default()
    })
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
    FaceClick(DateTime<Local>),
    OuterClick(DateTime<Local>),
}

impl Application for Clock {
    type Executor = executor::Default;
    type Message = ClockMessage;
    type Theme = Theme;
    type Flags = ();

    fn new(_flags: ()) -> (Self, Command<ClockMessage>) {
        (
            Clock {
                now: Local::now(),
                clock: Default::default(),
            },
            window::change_mode(window::Id::MAIN, window::Mode::Fullscreen)
        )
    }
    
    fn title(&self) -> String {
        String::from("Clock")
    }

    fn update(&mut self, message: ClockMessage) -> Command<ClockMessage> {
        match message {
            ClockMessage::Tick(local_time) => {
                let now = local_time;

                if now != self.now {
                    self.now = now;
                    self.clock.clear();
                }
            }
            ClockMessage::CenterClick => {std::process::exit(0)}
            ClockMessage::FaceClick(time) => {println!("Face Click @{:?}", time)}
            ClockMessage::OuterClick(time) => {println!("Outer click @{:?}", time)}
        }

        Command::none()
    }

    fn view(&self) -> Element<ClockMessage> {
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

impl canvas::Program<ClockMessage> for Clock {
    type State = ();

    fn update(
        &self,
        _state: &mut Self::State,
        event: Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> (event::Status, Option<ClockMessage>) {
        match event {
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                if let Some(position) = cursor.position() {
                    let radius = bounds.width.min(bounds.height) / 2.0;
                    let cursor_radius = bounds.center().distance(position) / radius;

                    if CENTER_BUTTON_REGION.contains(cursor_radius) {
                        (event::Status::Captured, Some(ClockMessage::CenterClick))
                    } else if CLOCK_FACE_REGION.contains(cursor_radius) {
                        let _hour = unit_from_position(bounds.center(), position, 12);
                        (event::Status::Captured, Some(ClockMessage::FaceClick(Local::now())))
                    } else {
                        let _hour = unit_from_position(bounds.center(), position, 12);
                        (event::Status::Captured, Some(ClockMessage::OuterClick(Local::now())))
                    }
                } else {
                    (event::Status::Ignored, None)
                }
            }
            _ => (event::Status::Ignored, None),
        }
    }

    fn draw(
        &self,
        _state: &Self::State,
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
                frame.rotate(hand_rotation(self.now.naive_local().time().hour() as u8, 12));
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
                frame.rotate(hand_rotation(self.now.naive_local().time().minute() as u8, 60));
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

        vec![clock]
    }

    fn mouse_interaction(
        &self,
        _state: &Self::State,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> mouse::Interaction {
        match cursor.position() {
            Some(position) => {
                let radius = bounds.width.min(bounds.height) / 2.0;
                let cursor_radius = bounds.center().distance(position) / radius;

                if CENTER_BUTTON_REGION.contains(cursor_radius) {
                    mouse::Interaction::Crosshair
                } else if CLOCK_FACE_REGION.contains(cursor_radius) {
                    mouse::Interaction::Grabbing
                } else {
                    mouse::Interaction::default()
                }
            },
            None => mouse::Interaction::default(),
        }
    }
}

// Calculate the unit (hour, minute, second) from a position relative to the center
// Zero is at top dead center
fn unit_from_position(center: Point, position: Point, total: u8) -> f32 {
    let relative_x = position.x - center.x;
    let relative_y = -(position.y - center.y);
    println!("Delta X = {}, Delta Y = {}", relative_x, relative_y);
    let div = relative_y / relative_x;
    let mut angle = div.atan();
    if relative_x < 0.0 {
        angle += PI;
    }
    println!("Angle in radians {}", angle);
    let angle = ((2.5 * PI) - angle) % (2.0 * PI);
    println!("Corrected angle in radians {}", angle);
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