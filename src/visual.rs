//! All drawing surfaces have explicit bounds; live PCM drives the marks.
use ferrite_design::prelude::*;
use gpui::{App, Bounds, IntoElement, Pixels, Window, canvas, div, fill, point, px, size};
pub fn trace(frames: Vec<[f32; 2]>, vectors: bool, cx: &App) -> impl IntoElement {
    let p = palette(cx);
    let (paper, ink, grid) = (hsla(p.sunken), hsla(p.accent), hsla(p.line));
    div().size_full().bg(paper).child(
        canvas(
            |_, _, _| {},
            move |b: Bounds<Pixels>, _, w: &mut Window, _| {
                let center = point(b.left() + b.size.width / 2., b.top() + b.size.height / 2.);
                w.paint_quad(fill(
                    Bounds::new(point(b.left(), center.y), size(b.size.width, px(1.))),
                    grid,
                ));
                if vectors {
                    w.paint_quad(fill(
                        Bounds::new(point(center.x, b.top()), size(px(1.), b.size.height)),
                        grid,
                    ));
                    let scale = b.size.width.min(b.size.height) * 0.42;
                    for [l, r] in frames {
                        let (l, r) = (l.clamp(-1., 1.), r.clamp(-1., 1.));
                        let x = center.x + scale * (l - r) * 0.5;
                        let y = center.y - scale * (l + r) * 0.5;
                        w.paint_quad(fill(Bounds::new(point(x, y), size(px(2.), px(2.))), ink));
                    }
                } else {
                    let step = b.size.width / frames.len().saturating_sub(1).max(1) as f32;
                    for (i, pair) in frames.windows(2).enumerate() {
                        let y = |f: [f32; 2]| {
                            center.y - b.size.height * 0.44 * ((f[0] + f[1]) * 0.5).clamp(-1., 1.)
                        };
                        let (a, z) = (y(pair[0]), y(pair[1]));
                        w.paint_quad(fill(
                            Bounds::new(
                                point(b.left() + step * i as f32, a.min(z)),
                                size(step.max(px(1.)), (a - z).abs().max(px(1.))),
                            ),
                            ink,
                        ));
                    }
                }
            },
        )
        .size_full(),
    )
}
pub fn spectrum(values: Vec<f32>, cx: &App) -> impl IntoElement {
    let p = palette(cx);
    let (paper, ink, line) = (hsla(p.sunken), hsla(p.accent), hsla(p.line));
    div().size_full().bg(paper).child(
        canvas(
            |_, _, _| {},
            move |b: Bounds<Pixels>, _, w: &mut Window, _| {
                for fraction in [0.25, 0.5, 0.75] {
                    w.paint_quad(fill(
                        Bounds::new(
                            point(b.left(), b.top() + b.size.height * fraction),
                            size(b.size.width, px(1.)),
                        ),
                        line,
                    ));
                }
                let step = b.size.width / values.len().max(1) as f32;
                for (i, v) in values.iter().enumerate() {
                    let h = b.size.height * v.clamp(0., 1.);
                    if h > px(0.) {
                        w.paint_quad(fill(
                            Bounds::new(
                                point(b.left() + step * i as f32, b.bottom() - h),
                                size((step - px(2.)).max(px(1.)), h),
                            ),
                            ink,
                        ));
                    }
                }
            },
        )
        .size_full(),
    )
}
pub fn overview(ranges: Vec<[f32; 2]>, cx: &App) -> impl IntoElement {
    let p = palette(cx);
    let (paper, ink) = (hsla(p.sunken), hsla(p.accent));
    div().size_full().bg(paper).child(
        canvas(
            |_, _, _| {},
            move |b: Bounds<Pixels>, _, w: &mut Window, _| {
                let center = b.top() + b.size.height / 2.;
                let step = b.size.width / ranges.len().max(1) as f32;
                for (i, [lo, hi]) in ranges.iter().enumerate() {
                    let top = center - b.size.height * 0.45 * hi.clamp(-1., 1.);
                    let bottom = center - b.size.height * 0.45 * lo.clamp(-1., 1.);
                    w.paint_quad(fill(
                        Bounds::new(
                            point(b.left() + step * i as f32, top),
                            size(step.max(px(1.)), (bottom - top).max(px(1.))),
                        ),
                        ink,
                    ));
                }
            },
        )
        .size_full(),
    )
}
pub fn scene(amplitudes: &[f32], phase: f32) -> ferrite_design::dither::Picture {
    let (width, height) = (120u32, 56u32);
    let bass = amplitudes
        .iter()
        .take(22)
        .copied()
        .fold(0., f32::max)
        .min(1.);
    let energy = amplitudes.iter().copied().fold(0., f32::max).min(1.);
    let mut pixels = Vec::with_capacity((width * height) as usize);
    for y in 0..height {
        for x in 0..width {
            let (dx, dy) = (
                (x as f32 - width as f32 / 2.) * 0.6,
                y as f32 - height as f32 / 2.,
            );
            let r = (dx * dx + dy * dy).sqrt();
            let angle = dy.atan2(dx);
            let wave = (r * (0.5 + bass * 1.5) - phase * 6. + angle * 3.)
                .sin()
                .abs();
            let value = ((1. - wave) * energy.sqrt() * 255.).clamp(0., 255.) as u8;
            pixels.push(value);
        }
    }
    ferrite_design::dither::Picture::new(width, height, pixels)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn audible_input_produces_visible_ascii_and_silence_stays_dark() {
        let active = scene(&[0.5; 64], 0.2);
        let (width, height) = active.size();
        let brightest = (0..height)
            .flat_map(|y| (0..width).map(move |x| (x, y)))
            .map(|(x, y)| active.raw(x, y))
            .max()
            .unwrap();
        assert!(
            brightest > 128,
            "Active audio must reach visible ASCII densities"
        );
        assert_ne!(active.id(), scene(&[0.5; 64], 0.4).id());
        let silent = scene(&[0.; 64], 0.2);
        assert!((0..height).all(|y| (0..width).all(|x| silent.raw(x, y) == 0)));
    }
}
