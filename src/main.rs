use iced::executor;
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
        iced::time::every(std::time::Duration::from_millis(500)).map(|_| {
            Message::Tick(chrono::offset::Local::now())
        })
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
            let radius = frame.width().min(frame.height()) / 2.0;

            let background = Path::circle(center, radius);
            frame.fill(&background, Color::from_rgb8(0x12, 0x93, 0xD8));

            let hour_hand =
                Path::line(Point::ORIGIN, Point::new(0.0, -0.5 * radius));

            let minute_hand =
                Path::line(Point::ORIGIN, Point::new(0.0, -0.8 * radius));

            let second_hand =
                Path::line(Point::ORIGIN, Point::new(0.0, -0.9 * radius));

            let width = radius / 200.0;

            let second_width = || -> Stroke {
                Stroke {
                    width,
                    style: stroke::Style::Solid(Color::WHITE),
                    line_cap: LineCap::Round,
                    ..Stroke::default()
                }
            };

            let minute_width = || -> Stroke {
                Stroke {
                    width: width * 6.0,
                    style: stroke::Style::Solid(Color::WHITE),
                    line_cap: LineCap::Round,
                    ..Stroke::default()
                }
            };

            let hour_width = || -> Stroke {
                Stroke {
                    width: width * 11.0,
                    style: stroke::Style::Solid(Color::WHITE),
                    line_cap: LineCap::Round,
                    ..Stroke::default()
                }
            };

            frame.translate(Vector::new(center.x, center.y));

            frame.with_save(|frame| {
                frame.rotate(hand_rotation(self.now.naive_local().time().hour() as u8, 12));
                frame.stroke(&hour_hand, hour_width());
            });

            frame.with_save(|frame| {
                frame.rotate(hand_rotation(self.now.naive_local().time().minute() as u8, 60));
                frame.stroke(&minute_hand, minute_width());
            });

            frame.with_save(|frame| {
                frame.rotate(hand_rotation(self.now.naive_local().time().second() as u8, 60));
                frame.stroke(&second_hand, second_width());
            })
        });

        vec![clock]
    }
}

fn hand_rotation(n: u8, total: u8) -> f32 {
    let turns = n as f32 / total as f32;

    2.0 * std::f32::consts::PI * turns
}
