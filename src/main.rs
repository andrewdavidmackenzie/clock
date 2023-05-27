use iced::{executor, mouse};
use iced::widget::canvas::{
    stroke, Cache, Cursor, Geometry, LineCap, Path, Stroke,
};
use iced::widget::{canvas, container};
use iced::{
    Application, Color, Command, Element, Length, Point, Rectangle, Settings,
    Subscription, Theme, Vector,
};
use chrono::prelude::*;
use chrono::{Local};

const CENTER_BUTTON_RADIUS: f32 = 1.0 / 15.0;
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
        ..Settings::default()
    })
}

struct Clock {
    now: chrono::DateTime<Local>,
    clock: Cache,
}

#[derive(Debug, Clone, Copy)]
enum Message {
    Tick(chrono::DateTime<Local>),
}

impl Application for Clock {
    type Executor = executor::Default;
    type Message = Message;
    type Theme = Theme;
    type Flags = ();

    fn new(_flags: ()) -> (Self, Command<Message>) {
        (
            Clock {
                now: chrono::offset::Local::now(),
                clock: Default::default(),
            },
            Command::none(),
        )
    }

    fn title(&self) -> String {
        String::from("Clock")
    }

    fn update(&mut self, message: Message) -> Command<Message> {
        match message {
            Message::Tick(local_time) => {
                let now = local_time;

                if now != self.now {
                    self.now = now;
                    self.clock.clear();
                }
            }
        }

        Command::none()
    }

    fn view(&self) -> Element<Message> {
        let canvas = canvas(self as &Self)
            .width(Length::Fill)
            .height(Length::Fill);

        container(canvas)
            .width(Length::Fill)
            .height(Length::Fill)
            .padding(20)
            .into()
    }

    fn subscription(&self) -> Subscription<Message> {
        iced::time::every(std::time::Duration::from_secs(1)).map(|_| {
            Message::Tick(chrono::offset::Local::now())
        })
    }
}

// maybe when mouse is hovering on the inside of the face, show current times in this 12h period
// and when hovering outside, show times in next 12h period, to allow you to click on a time 1h
// later or 13h later
enum ClockClick {
    Center,
    Face(chrono::DateTime<Local>)
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
        (position > self.inner_radius) && (position < self.outer_radius)
    }
}

impl<Message> canvas::Program<Message> for Clock {
    type State = ();

    fn draw(
        &self,
        _state: &Self::State,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: Cursor,
    ) -> Vec<Geometry> {
        let clock = self.clock.draw(bounds.size(), |frame| {
            let center = frame.center();
            frame.translate(Vector::new(center.x, center.y));

            let radius = frame.width().min(frame.height()) / 2.0;

            let background = Path::circle(Point::ORIGIN, radius * CLOCK_FACE_RADIUS);
            frame.fill(&background, Color::from_rgb8(0x12, 0x93, 0xD8));

            let hour_hand =
                Path::line(Point::ORIGIN, Point::new(0.0, -0.7 * radius));

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
                Path::line(Point::ORIGIN, Point::new(0.0, -0.9 * radius));

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
                Path::line(Point::ORIGIN, Point::new(0.0, - 0.95 * radius));

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
        cursor: Cursor,
    ) -> mouse::Interaction {
        match cursor.position() {
            Some(position) => {
                let radius = bounds.width.min(bounds.height) / 2.0;
                let cursor_radius = bounds.center().distance(position) / radius;

                if CENTER_BUTTON_REGION.contains(cursor_radius.clone()) {
                    mouse::Interaction::Crosshair
                } else if CLOCK_FACE_REGION.contains(cursor_radius) {
                    mouse::Interaction::NotAllowed
                } else {
                    mouse::Interaction::default()
                }
            },
            None => mouse::Interaction::default(),
        }
    }
}

fn hand_rotation(n: u8, total: u8) -> f32 {
    let turns = n as f32 / total as f32;

    2.0 * std::f32::consts::PI * turns
}
