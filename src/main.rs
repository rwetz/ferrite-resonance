#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod audio;
mod dsp;
mod prefs;
mod probe;
mod track;
mod visual;
mod worker;
use ferrite_design::prelude::*;
use gpui::{
    App, AppContext as _, Context, Entity, ExternalPaths, IntoElement, KeyBinding,
    PathPromptOptions, Render, Subscription, Task, Window, div, px, size,
};
use std::{
    path::PathBuf,
    sync::{Arc, atomic::Ordering},
    time::Duration,
};
gpui::actions!(resonance, [OpenSong, PlayPause]);

struct Resonance {
    palette: Entity<CommandPalette>,
    _appearance: Subscription,
    player: Arc<audio::Player>,
    track: Option<Arc<track::Track>>,
    report: Option<dsp::Report>,
    fft: dsp::Spectrum,
    amplitudes: Vec<f32>,
    frames: Vec<[f32; 2]>,
    mode: usize,
    busy: bool,
    status: String,
    generation: u64,
    job: Option<Task<()>>,
    _tick: Task<()>,
}
impl Resonance {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let tick = cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(33))
                    .await;
                if this.update(cx, |this, cx| this.tick(cx)).is_err() {
                    break;
                }
            }
        });
        let mut app = Self {
            palette: cx.new(|cx| CommandPalette::new(window, cx)),
            _appearance: theme::follow_system(window),
            player: audio::Player::new(),
            track: None,
            report: None,
            fft: dsp::Spectrum::new(),
            amplitudes: vec![0.; dsp::BINS],
            frames: vec![[0.; 2]; dsp::FFT_SIZE],
            mode: 0,
            busy: false,
            status: "Drop a song or open one with Ctrl+O".into(),
            generation: 0,
            job: None,
            _tick: tick,
        };
        if std::env::args().any(|a| a == "--mute") {
            app.player.volume(0.);
        }
        let weak = cx.weak_entity();
        let play = weak.clone();
        app.palette.update(cx, |palette, cx| {
            let mut commands = vec![
                command("Open song")
                    .group("Resonance")
                    .shortcut("Ctrl+O")
                    .on_run(move |_, cx| {
                        let _ = weak.update(cx, |this, cx| this.open(cx));
                    }),
                command("Play / pause")
                    .group("Resonance")
                    .shortcut("Space")
                    .on_run(move |_, cx| {
                        let _ = play.update(cx, |this, cx| {
                            this.player.toggle();
                            cx.notify();
                        });
                    }),
            ];
            for scheme in SCHEMES {
                commands.push(
                    command(format!("Scheme: {}", scheme.name))
                        .group("Theme")
                        .on_run(move |_, cx| prefs::scheme(scheme, cx)),
                );
            }
            commands.push(
                command("Toggle light / dark")
                    .group("Theme")
                    .on_run(|w, cx| {
                        prefs::appearance(
                            if theme::appearance(cx) == Appearance::Light {
                                Appearance::Dark
                            } else {
                                Appearance::Light
                            },
                            w,
                            cx,
                        )
                    }),
            );
            palette.set_commands(commands, cx);
        });
        if let Some(path) = cli_path() {
            app.load(path, cx);
        }
        app
    }
    fn open(&mut self, cx: &mut Context<Self>) {
        let paths = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Open a song".into()),
        });
        cx.spawn(async move |this, cx| {
            if let Ok(Ok(Some(paths))) = paths.await
                && let Some(path) = paths.into_iter().next()
            {
                let _ = this.update(cx, |this, cx| this.load(path, cx));
            }
        })
        .detach();
    }
    fn load(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        self.generation += 1;
        let generation = self.generation;
        self.player.playing.store(false, Ordering::Relaxed);
        self.busy = true;
        self.status = "Decoding and measuring audio...".into();
        self.job = Some(cx.spawn(async move |this, cx| {
            let result = worker::run(move || {
                let track = track::load(&path)?;
                let report = dsp::analyze(&track);
                Ok((track, report))
            })
            .await;
            let _ = this.update(cx, |this, cx| {
                if this.generation != generation {
                    return;
                }
                this.busy = false;
                match result {
                    Ok((track, report)) => {
                        this.status =
                            format!("{} Hz / {} stereo frames", track.rate, track.frames.len());
                        this.player.load(track.clone());
                        this.track = Some(track);
                        this.report = Some(report);
                        if std::env::args().any(|a| a == "--play") {
                            this.player.toggle();
                        }
                    }
                    Err(e) => this.status = e,
                }
                this.tick(cx);
            });
        }));
        cx.notify();
    }
    fn tick(&mut self, cx: &mut Context<Self>) {
        if let Some(t) = &self.track {
            let pos = (self.player.position.load(Ordering::Relaxed) as usize).min(t.frames.len());
            let start = pos.saturating_sub(dsp::FFT_SIZE);
            self.frames = t.frames[start..(start + dsp::FFT_SIZE).min(t.frames.len())].to_vec();
            self.amplitudes = self.fft.amplitudes(&self.frames, t.rate);
        }
        cx.notify();
    }
}
impl Render for Resonance {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = palette(cx);
        let playing = self.player.playing.load(Ordering::Relaxed);
        let pos = self.player.position.load(Ordering::Relaxed) as f64;
        let seconds = self.track.as_ref().map_or(0., |t| pos / t.rate as f64);
        let duration = self.track.as_ref().map_or(1., |t| t.seconds());
        let visual = match self.mode {
            1 => visual::spectrum(dsp::bars(&self.amplitudes), cx).into_any_element(),
            2 => visual::trace(self.frames.clone(), true, cx).into_any_element(),
            3 => div()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .overflow_hidden()
                .bg(hsla(p.sunken))
                .child(
                    responsive("ascii-room", {
                        let amplitudes = self.amplitudes.clone();
                        move |room, _, cx| {
                            ascii_art(visual::scene(&amplitudes, seconds as f32))
                                .cols((f32::from(room.width) / 9.).clamp(20., 180.) as usize)
                                .color(hsla(palette(cx).accent))
                                .into_any_element()
                        }
                    })
                    .size_full(),
                )
                .into_any_element(),
            _ => visual::trace(self.frames.clone(), false, cx).into_any_element(),
        };
        if let Some(track) = &self.track {
            probe::record(format!(
                "Song loaded; {} frames; spectrum peak {:.4}",
                track.frames.len(),
                self.amplitudes.iter().copied().fold(0., f32::max)
            ));
        }
        let error = self.player.error.lock().unwrap().clone();
        let status = error.clone().unwrap_or_else(|| self.status.clone());
        let overview = self
            .report
            .as_ref()
            .map(|r| visual::overview(r.waveform.clone(), cx).into_any_element());
        let details = self
            .report
            .as_ref()
            .map(|r| {
                format!(
                    "SAMPLE PEAK {} / {} dBFS   RMS {} / {} dBFS",
                    dsp::db(r.levels.peak[0]),
                    dsp::db(r.levels.peak[1]),
                    dsp::db(r.levels.rms[0]),
                    dsp::db(r.levels.rms[1])
                )
            })
            .unwrap_or("WAV / MP3 / FLAC / OGG / AAC / M4A".into());
        window_frame().child(power_on_in(
            "power",
            div()
                .size_full()
                .flex()
                .flex_col()
                .bg(hsla(p.bg))
                .text_color(hsla(p.fg))
                .body(text::BASE)
                .on_action(cx.listener(|this, _: &TogglePalette, w, cx| {
                    this.palette.update(cx, |p, cx| p.toggle(w, cx))
                }))
                .on_action(cx.listener(|this, _: &OpenSong, _, cx| this.open(cx)))
                .on_action(cx.listener(|this, _: &PlayPause, _, cx| {
                    this.player.toggle();
                    cx.notify();
                }))
                .child(self.palette.clone())
                .child(
                    title_bar("Resonance")
                        .child(
                            Button::new("menu")
                                .icon(Icon::Menu)
                                .ghost()
                                .small()
                                .on_click(cx.listener(|this, _, w, cx| {
                                    this.palette.update(cx, |p, cx| p.toggle(w, cx))
                                })),
                        )
                        .child(
                            Button::new("open")
                                .label("Open song")
                                .icon(Icon::Folder)
                                .ghost()
                                .small()
                                .disabled(self.busy)
                                .on_click(cx.listener(|this, _, _, cx| this.open(cx))),
                        ),
                )
                .child(
                    div()
                        .id("song-content")
                        .flex_1()
                        .min_h_0()
                        .flex()
                        .flex_col()
                        .p_4()
                        .gap_3()
                        .on_drop(cx.listener(|this, paths: &ExternalPaths, _, cx| {
                            if let Some(path) = paths.paths().first() {
                                this.load(path.clone(), cx);
                            }
                        }))
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap_3()
                                .child(
                                    div().flex_1().min_w_0().overflow_hidden().child(
                                        self.track
                                            .as_ref()
                                            .map_or("DROP A SONG TO BEGIN".into(), |t| {
                                                t.name.clone()
                                            }),
                                    ),
                                )
                                .child(
                                    segmented("mode")
                                        .option("Wave")
                                        .option("Spectrum")
                                        .option("Stereo")
                                        .option("ASCII")
                                        .selected(self.mode)
                                        .on_select(cx.listener(|this, index, _, cx| {
                                            this.mode = *index;
                                            cx.notify();
                                        })),
                                ),
                        )
                        .child(
                            panel(
                                [
                                    "Waveform",
                                    "Spectrum",
                                    "Stereo vectorscope",
                                    "Audio-reactive ASCII",
                                ][self.mode],
                            )
                            .meta(if self.mode == 1 {
                                "30 Hz - 20 kHz / -80 to 0 dBFS"
                            } else {
                                "SOURCE AUDIO / BEFORE VOLUME"
                            })
                            .flex_1()
                            .min_h(px(120.))
                            .child(div().flex_1().min_h_0().child(visual)),
                        )
                        .when_some(overview, |d, overview| {
                            d.child(
                                panel("Song overview")
                                    .meta(details)
                                    .child(div().h(px(72.)).w_full().child(overview)),
                            )
                        })
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap_3()
                                .child(
                                    Button::new("play")
                                        .label(if playing { "Pause" } else { "Play" })
                                        .icon(if playing { Icon::Pause } else { Icon::Play })
                                        .primary()
                                        .disabled(
                                            self.track.is_none() || self.busy || error.is_some(),
                                        )
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.player.toggle();
                                            cx.notify();
                                        })),
                                )
                                .child(
                                    Button::new("restart")
                                        .label("Restart")
                                        .ghost()
                                        .disabled(self.track.is_none())
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.player.seek(0);
                                            cx.notify();
                                        })),
                                )
                                .child(
                                    switch("loop")
                                        .label("Loop song")
                                        .checked(self.player.looping.load(Ordering::Relaxed))
                                        .on_change(cx.listener(|this, value, _, cx| {
                                            this.player.looping.store(*value, Ordering::Relaxed);
                                            cx.notify();
                                        })),
                                )
                                .child(
                                    div().flex_1().min_w_0().child(
                                        slider("seek")
                                            .range(0., duration as f32)
                                            .value(seconds as f32)
                                            .disabled(self.track.is_none())
                                            .format(|v| track::time(v as f64).into())
                                            .on_change(cx.listener(|this, value, _, cx| {
                                                if let Some(t) = &this.track {
                                                    this.player.seek(
                                                        (*value as f64 * t.rate as f64) as u64,
                                                    );
                                                }
                                                cx.notify();
                                            })),
                                    ),
                                )
                                .child(
                                    div().w(px(160.)).child(
                                        slider("volume")
                                            .range(0., 1.)
                                            .value(f32::from_bits(
                                                self.player.volume.load(Ordering::Relaxed),
                                            ))
                                            .format(|v| format!("VOL {:.0}%", v * 100.).into())
                                            .on_change(cx.listener(|this, value, _, cx| {
                                                this.player.volume(*value);
                                                cx.notify();
                                            })),
                                    ),
                                ),
                        ),
                )
                .child(status_bar().left_live(status).right_live(format!(
                    "{} / {}",
                    track::time(seconds),
                    track::time(duration)
                ))),
        ))
    }
}
fn cli_path() -> Option<PathBuf> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    args.iter()
        .position(|a| a == "--file")
        .and_then(|i| args.get(i + 1))
        .map(PathBuf::from)
}
fn main() {
    if std::env::args().any(|a| a == "--version") {
        println!("{} {}", env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"));
        return;
    }
    if std::env::args().any(|a| a == "--smoke") {
        let result = cli_path()
            .ok_or("Use --smoke --file PATH".into())
            .and_then(|p| track::load(&p));
        match result {
            Ok(t) => {
                let r = dsp::analyze(&t);
                println!(
                    "{}",
                    serde_json::json!({"file":t.name,"rate":t.rate,"frames":t.frames.len(),"seconds":t.seconds(),"peak":r.levels.peak,"rms":r.levels.rms,"correlation":r.levels.correlation,"clipped_samples":r.levels.clipped,"spectrum_windows":r.windows,"spectrum_peak":r.spectrum.iter().copied().fold(0.,f32::max)})
                );
            }
            Err(e) => {
                eprintln!("{e}");
                std::process::exit(1);
            }
        }
        return;
    }
    gpui_platform::application().run(|cx: &mut App| {
        prefs::init(cx);
        motion::set_fps(30);
        cx.bind_keys([
            KeyBinding::new("ctrl-shift-p", TogglePalette, None),
            KeyBinding::new("ctrl-o", OpenSong, None),
            KeyBinding::new("space", PlayPause, None),
        ]);
        let options = chrome::remembered_window_options(
            "resonance",
            "Resonance",
            size(px(1180.), px(780.)),
            cx,
        );
        cx.open_window(options, |w, cx| {
            chrome::square_corners(w);
            chrome::remember_window("resonance", w, cx);
            chrome::power_off_on_close(w, cx);
            cx.new(|cx| Resonance::new(w, cx))
        })
        .expect("Couldn't open Resonance");
        cx.activate(true);
    });
}
