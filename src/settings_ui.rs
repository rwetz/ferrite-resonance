//! Visible, persistent appearance controls for this application.
use ferrite_design::prelude::*;
use gpui::{Context, Entity, IntoElement, Render, Window, div, px};

gpui::actions!(preferences, [ToggleSettings]);
const APPEARANCES: [Appearance; 3] = [Appearance::Dark, Appearance::Light, Appearance::System];

pub struct Settings {
    open: bool,
}

impl Settings {
    pub fn new() -> Self {
        Self {
            open: std::env::args().any(|arg| arg == "--settings"),
        }
    }

    pub fn toggle(&mut self, cx: &mut Context<Self>) {
        self.open = !self.open;
        cx.notify();
    }
}

pub fn button(settings: Entity<Settings>) -> Button {
    Button::new("open-settings")
        .icon(Icon::Sliders)
        .ghost()
        .small()
        .tooltip("Settings · Ctrl+,")
        .on_click(move |_, _, cx| settings.update(cx, |s, cx| s.toggle(cx)))
}

pub fn settings_command(
    settings: Entity<Settings>,
) -> ferrite_design::components::palette::PaletteCommand {
    command("Settings")
        .group("App")
        .icon(Icon::Sliders)
        .shortcut("Ctrl+,")
        .on_run(move |_, cx| {
            settings.update(cx, |s, cx| {
                s.open = true;
                cx.notify();
            })
        })
}

impl Render for Settings {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let weak = cx.weak_entity();
        let error = crate::prefs::save_error(cx);
        drawer("app-settings")
            .open(self.open)
            .title("Settings")
            .width(px(420.))
            .on_close(move |_, cx| {
                let _ = weak.update(cx, |s, cx| {
                    s.open = false;
                    cx.notify();
                });
            })
            .child(rule(Some("scheme"), window, cx))
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap_2()
                    .children(SCHEMES.iter().map(|s| {
                        div().w(px(180.)).child(
                            Button::new(gpui::ElementId::Name(
                                format!("settings-scheme-{}", s.key).into(),
                            ))
                            .label(s.name)
                            .secondary()
                            .full_width()
                            .selected(theme::scheme(cx).key == s.key)
                            .on_click(move |_, _, cx| crate::prefs::scheme(s, cx)),
                        )
                    })),
            )
            .child(
                field("settings-appearance", "Appearance").stacked().child(
                    segmented("settings-appearance-options")
                        .option("Dark")
                        .option("Light")
                        .option("System")
                        .selected(
                            APPEARANCES
                                .iter()
                                .position(|a| *a == theme::appearance(cx))
                                .unwrap_or(0),
                        )
                        .on_select(|i, w, cx| crate::prefs::appearance(APPEARANCES[*i], w, cx)),
                ),
            )
            .child(
                field("settings-fps", "Refresh rate")
                    .stacked()
                    .hint("Frames per second for Ferrite motion.")
                    .child(
                        number_input("settings-fps-value")
                            .value(motion::fps() as f64)
                            .range(12., 240.)
                            .step(1.)
                            .digits(3)
                            .suffix("FPS")
                            .on_change(|fps, _, cx| crate::prefs::fps(*fps as u32, cx)),
                    ),
            )
            .child(
                field("settings-density", "Row density").stacked().child(
                    theme::Density::ALL
                        .iter()
                        .fold(segmented("settings-density-options"), |seg, d| {
                            seg.option(d.label())
                        })
                        .selected(theme::density(cx).index())
                        .on_select(|i, _, cx| crate::prefs::density(theme::Density::ALL[*i], cx)),
                ),
            )
            .child(
                div()
                    .body(text::SM)
                    .text_color(hsla(palette(cx).fg_dim))
                    .child("Changes apply immediately and are saved for the next launch."),
            )
            .children(error.map(|error| {
                div()
                    .body(text::SM)
                    .text_color(hsla(palette(cx).danger))
                    .child(error)
            }))
    }
}
