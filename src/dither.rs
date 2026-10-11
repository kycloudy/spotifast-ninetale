//! The dithered artwork behind a page's header or full-screen lyrics.
//!
//! The page's cover, or the playing song's art on pages without one, is
//! drawn as an ordered (Bayer) dither of coloured dots that thin out toward
//! the page below. Each dot is two physical pixels, so the texture is made
//! for the header's size on screen rather than scaled from the cover.
//!
//! Fetching, decoding and dithering all run on the art loader's runtime.
//! The header keeps showing the last texture, stretched if the window was
//! resized, until the new one is ready, and a new cover fades in over the
//! old one, so a change of page or song never flashes the plain background.

use std::sync::Arc;
use std::sync::mpsc::{Receiver, TryRecvError, channel};
use std::time::{Duration, Instant};

use egui::{Color32, ColorImage, Rect, TextureHandle, TextureOptions, pos2};

use crate::images::ArtLoader;

/// One dot, in physical pixels.
pub const DOT: f32 = 2.0;
/// The largest texture made, in dots, so a very wide window stays cheap.
const MAX_COLUMNS: usize = 2048;
const MAX_ROWS: usize = 512;
/// The decoded cover's longest side. Covers are scaled far past this, and
/// the dither carries no detail finer than a few dots anyway.
const SOURCE_SIZE: u32 = 160;
/// How long a new cover takes to fade in over the last one.
const CROSSFADE: Duration = Duration::from_millis(320);
/// How long a cover whose download failed waits to ask again.
const RETRY: Duration = Duration::from_secs(3);

/// The most of the header's dots a cover may light on average, before the
/// fade toward the page: a brighter cover's dots are thinned to this.
const MAX_MEAN_VALUE: f32 = 0.42;

const BAYER: [[u8; 4]; 4] = [[0, 8, 2, 10], [12, 4, 14, 6], [3, 11, 1, 9], [15, 7, 13, 5]];

/// A decoded cover, small and softened, ready to be dithered.
#[derive(Clone)]
pub struct Source {
    width: usize,
    height: usize,
    pixels: Arc<[[u8; 3]]>,
}

impl Source {
    pub fn from_rgb(width: usize, height: usize, pixels: Vec<[u8; 3]>) -> Option<Self> {
        (width > 0 && height > 0 && pixels.len() == width * height).then(|| Self {
            width,
            height,
            pixels: pixels.into(),
        })
    }

    fn decode(bytes: &[u8]) -> Option<Self> {
        let mut reader = image::ImageReader::new(std::io::Cursor::new(bytes))
            .with_guessed_format()
            .ok()?;
        let mut limits = image::Limits::default();
        limits.max_image_width = Some(8192);
        limits.max_image_height = Some(8192);
        limits.max_alloc = Some(64 * 1024 * 1024);
        reader.limits(limits);
        let image = reader
            .decode()
            .ok()?
            .thumbnail(SOURCE_SIZE, SOURCE_SIZE)
            .blur(0.8)
            .to_rgb8();
        let (width, height) = (image.width() as usize, image.height() as usize);
        Self::from_rgb(width, height, image.pixels().map(|pixel| pixel.0).collect())
    }

    /// The colour under a point of a `columns` by `rows` area the cover
    /// fills, cropped about its centre, sampled bilinearly.
    fn sample(&self, columns: usize, rows: usize, x: f32, y: f32) -> [f32; 3] {
        let (width, height) = (self.width as f32, self.height as f32);
        let scale = (columns as f32 / width).max(rows as f32 / height);
        let left = (width - columns as f32 / scale) * 0.5;
        let top = (height - rows as f32 / scale) * 0.5;
        let sx = (left + x / scale - 0.5).clamp(0.0, width - 1.0);
        let sy = (top + y / scale - 0.5).clamp(0.0, height - 1.0);
        let (x0, y0) = (sx.floor() as usize, sy.floor() as usize);
        let (x1, y1) = ((x0 + 1).min(self.width - 1), (y0 + 1).min(self.height - 1));
        let (fx, fy) = (sx - x0 as f32, sy - y0 as f32);
        let at = |x: usize, y: usize| self.pixels[y * self.width + x];
        let mut out = [0.0; 3];
        for (channel, value) in out.iter_mut().enumerate() {
            let top = at(x0, y0)[channel] as f32 * (1.0 - fx) + at(x1, y0)[channel] as f32 * fx;
            let bottom = at(x0, y1)[channel] as f32 * (1.0 - fx) + at(x1, y1)[channel] as f32 * fx;
            *value = top * (1.0 - fy) + bottom * fy;
        }
        out
    }
}

/// What a texture was made for. A texture made for another key is still
/// drawn while its replacement is made.
#[derive(Clone, Debug, PartialEq)]
struct Key {
    uri: String,
    columns: usize,
    rows: usize,
    dark: bool,
    strength: u8,
    fade_to_bottom: bool,
}

/// How far down the header a dot can still appear: all of it at the top,
/// none at the bottom, easing out so the dots thin gradually.
fn fade(row: usize, rows: usize) -> f32 {
    let t = (row as f32 + 0.5) / rows.max(1) as f32;
    let t = 1.0 - t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// The dithered header for a `columns` by `rows` grid of dots.
///
/// Dark: a dot is lit where the cover is brighter than the Bayer threshold,
/// in the cover's colour at full brightness, as in Zeron's dither. Light: a
/// dot is inked where the cover is darker or more colourful than paper, in
/// the cover's colour held dark enough to read on white. Unlit dots are
/// transparent, so the page's own background shows between them.
pub fn raster(
    source: &Source,
    columns: usize,
    rows: usize,
    dark: bool,
    strength: f32,
) -> ColorImage {
    raster_with_fade(source, columns, rows, dark, strength, true)
}

fn raster_with_fade(
    source: &Source,
    columns: usize,
    rows: usize,
    dark: bool,
    strength: f32,
    fade_to_bottom: bool,
) -> ColorImage {
    let alpha = (strength.clamp(0.0, 1.0) * 255.0).round() as u8;
    let mut dots = Vec::with_capacity(columns * rows);
    for row in 0..rows {
        for column in 0..columns {
            let [r, g, b] = source.sample(columns, rows, column as f32 + 0.5, row as f32 + 0.5);
            let (max, min) = (r.max(g).max(b), r.min(g).min(b));
            dots.push(if dark {
                let gain = 255.0 / max.max(1.0);
                (max / 255.0, [r * gain, g * gain, b * gain])
            } else {
                // Lightness, then the colour pulled no lighter than 0.45.
                let lightness = (max + min) / 510.0;
                let gain = if lightness > 0.45 {
                    0.45 / lightness
                } else {
                    1.0
                };
                (1.0 - min / 255.0, [r * gain, g * gain, b * gain])
            });
        }
    }
    // A bright cover in the dark, or a dark one in the light, would light
    // nearly every dot and drown the header's text: such a cover's dots are
    // thinned until it lights no more of them than a moderate cover does.
    let mean = dots.iter().map(|(value, _)| value).sum::<f32>() / dots.len().max(1) as f32;
    let thinning = if mean > MAX_MEAN_VALUE {
        MAX_MEAN_VALUE / mean
    } else {
        1.0
    };
    let mut pixels = vec![Color32::TRANSPARENT; columns * rows];
    for row in 0..rows {
        let fade = if fade_to_bottom { fade(row, rows) } else { 1.0 } * thinning;
        for column in 0..columns {
            let (value, color) = dots[row * columns + column];
            let threshold = (BAYER[row % 4][column % 4] as f32 + 0.5) / 16.0;
            if value * fade > threshold {
                let [r, g, b] = color.map(|channel| channel.round().clamp(0.0, 255.0) as u8);
                pixels[row * columns + column] = Color32::from_rgba_unmultiplied(r, g, b, alpha);
            }
        }
    }
    ColorImage::new([columns, rows], pixels)
}

/// The dither's grid for a header of `rect` at `pixels_per_point`.
pub fn grid(rect: Rect, pixels_per_point: f32) -> (usize, usize) {
    let dots =
        |points: f32, max: usize| ((points * pixels_per_point / DOT).ceil() as usize).clamp(1, max);
    (
        dots(rect.width(), MAX_COLUMNS),
        dots(rect.height(), MAX_ROWS),
    )
}

type Decoded = Option<Source>;
type Rastered = (Key, ColorImage);

/// The header's dither: the decoded cover for the shown page and the
/// textures made from it.
#[derive(Default)]
pub struct DitherHero {
    source: Option<(String, Source)>,
    decoding: Option<(String, Receiver<Decoded>)>,
    retry: Option<(String, Instant)>,
    rastering: Option<Receiver<Rastered>>,
    current: Option<(Key, TextureHandle)>,
    previous: Option<TextureHandle>,
    shown_at: Option<Instant>,
}

/// How a header or lyrics backdrop's dither looks.
#[derive(Clone, Copy, Debug)]
pub struct Look {
    /// Whether the theme is dark.
    pub dark: bool,
    /// Each dot's opacity, baked into the texture.
    pub strength: f32,
    /// The whole texture's opacity, which fades it without making it again.
    pub opacity: f32,
    /// Headers thin out toward the page; backdrops keep dots throughout.
    pub fade_to_bottom: bool,
    /// Clips the artwork to the backdrop's rounded frame.
    pub corner_radius: u8,
}

impl DitherHero {
    /// Paints the dither for `uri` over `rect`, starting whatever work it
    /// still needs.
    pub fn paint(
        &mut self,
        ui: &egui::Ui,
        loader: &ArtLoader,
        uri: Option<&str>,
        rect: Rect,
        look: Look,
    ) {
        let Look {
            dark,
            strength,
            opacity,
            fade_to_bottom,
            corner_radius,
        } = look;
        let ctx = ui.ctx().clone();
        self.receive(&ctx);
        if let Some(uri) = uri {
            self.decode(&ctx, loader, uri);
            let (columns, rows) = grid(rect, ctx.pixels_per_point());
            let key = Key {
                uri: uri.to_owned(),
                columns,
                rows,
                dark,
                strength: (strength.clamp(0.0, 1.0) * 255.0) as u8,
                fade_to_bottom,
            };
            self.raster(&ctx, loader, key, strength);
        }
        let mix = self.shown_at.map_or(1.0, |at| {
            (at.elapsed().as_secs_f32() / CROSSFADE.as_secs_f32()).min(1.0)
        });
        if mix >= 1.0 {
            self.previous = None;
            self.shown_at = None;
        } else {
            ctx.request_repaint();
        }
        let opacity = opacity.clamp(0.0, 1.0);
        if uri.is_none() || opacity <= 0.0 {
            return;
        }
        let paint = |texture: &TextureHandle, opacity: f32| {
            let tint = Color32::WHITE.gamma_multiply(opacity);
            if corner_radius == 0 {
                ui.painter_at(rect).image(
                    texture.id(),
                    rect,
                    Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                    tint,
                );
            } else {
                egui::Image::new((texture.id(), rect.size()))
                    .tint(tint)
                    .corner_radius(corner_radius)
                    .paint_at(ui, rect);
            }
        };
        if let Some(previous) = &self.previous {
            paint(previous, (1.0 - mix) * opacity);
        }
        let Some((key, texture)) = &self.current else {
            return;
        };
        // Art made for another page, song or theme stays until this one's
        // is ready, then fades out under it.
        let fresh = Some(key.uri.as_str()) == uri && key.dark == dark;
        let shown = if fresh { mix } else { 1.0 };
        paint(texture, shown * opacity);
    }

    fn receive(&mut self, ctx: &egui::Context) {
        if let Some((uri, receiver)) = &self.decoding {
            match receiver.try_recv() {
                Ok(Some(source)) => {
                    self.source = Some((uri.clone(), source));
                    self.decoding = None;
                }
                Ok(None) | Err(TryRecvError::Disconnected) => {
                    self.retry = Some((uri.clone(), Instant::now() + RETRY));
                    ctx.request_repaint_after(RETRY);
                    self.decoding = None;
                }
                Err(TryRecvError::Empty) => {}
            }
        }
        if let Some(receiver) = &self.rastering {
            match receiver.try_recv() {
                Ok((key, image)) => {
                    self.rastering = None;
                    let texture = ctx.load_texture("dither-hero", image, TextureOptions::NEAREST);
                    // New art fades in over the old; the same art at a new
                    // size or strength takes its place at once.
                    let fades = self
                        .current
                        .as_ref()
                        .is_none_or(|(shown, _)| shown.uri != key.uri || shown.dark != key.dark);
                    let old = self.current.replace((key, texture));
                    if fades {
                        self.previous = old.map(|(_, texture)| texture);
                        self.shown_at = Some(Instant::now());
                    }
                }
                Err(TryRecvError::Disconnected) => self.rastering = None,
                Err(TryRecvError::Empty) => {}
            }
        }
    }

    fn decode(&mut self, ctx: &egui::Context, loader: &ArtLoader, uri: &str) {
        let have = self.source.as_ref().is_some_and(|(held, _)| held == uri);
        let busy = self.decoding.as_ref().is_some_and(|(held, _)| held == uri);
        let waiting = self
            .retry
            .as_ref()
            .is_some_and(|(held, at)| held == uri && Instant::now() < *at);
        if have || busy || waiting || !(uri.starts_with("https://") || uri.starts_with("http://")) {
            return;
        }
        let (tx, rx) = channel();
        self.decoding = Some((uri.to_owned(), rx));
        let ctx = ctx.clone();
        let url = uri.to_owned();
        loader.spawn_fetch(url, move |_, bytes| {
            let _ = tx.send(bytes.and_then(|bytes| Source::decode(&bytes)));
            ctx.request_repaint();
        });
    }

    fn raster(&mut self, ctx: &egui::Context, loader: &ArtLoader, key: Key, strength: f32) {
        if self.rastering.is_some()
            || self
                .current
                .as_ref()
                .is_some_and(|(shown, _)| *shown == key)
        {
            return;
        }
        let Some((uri, source)) = &self.source else {
            return;
        };
        if *uri != key.uri {
            return;
        }
        let source = source.clone();
        let (tx, rx) = channel();
        self.rastering = Some(rx);
        let ctx = ctx.clone();
        loader.spawn_blocking(move || {
            let image = raster_with_fade(
                &source,
                key.columns,
                key.rows,
                key.dark,
                strength,
                key.fade_to_bottom,
            );
            let _ = tx.send((key, image));
            ctx.request_repaint();
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flat(rgb: [u8; 3]) -> Source {
        Source::from_rgb(4, 4, vec![rgb; 16]).unwrap()
    }

    fn lit(image: &ColorImage) -> usize {
        image.pixels.iter().filter(|pixel| pixel.a() > 0).count()
    }

    #[test]
    fn a_lyrics_backdrop_keeps_cover_coloured_dots_at_the_bottom() {
        let source = flat([100, 50, 0]);
        let backdrop = raster_with_fade(&source, 64, 64, true, 0.3, false);
        let header = raster(&source, 64, 64, true, 0.3);
        let bottom = |image: &ColorImage| {
            image.pixels[48 * 64..]
                .iter()
                .filter(|pixel| pixel.a() > 0)
                .count()
        };
        assert!(bottom(&backdrop) > 300);
        assert!(bottom(&header) < bottom(&backdrop) / 10);
        assert!(
            backdrop
                .pixels
                .iter()
                .filter(|pixel| pixel.a() > 0)
                .all(|pixel| {
                    let [r, g, b, a] = pixel.to_srgba_unmultiplied();
                    r >= 250 && (125..=132).contains(&g) && b == 0 && a == 77
                })
        );
    }

    #[test]
    fn dots_thin_out_toward_the_page() {
        // A moderate cover, below the brightness at which dots are thinned.
        let image = raster(&flat([100, 60, 105]), 64, 64, true, 0.6);
        let row = |y: usize| {
            lit(&ColorImage::new(
                [64, 1],
                image.pixels[y * 64..(y + 1) * 64].to_vec(),
            ))
        };
        assert!(row(0) > 20, "the top is well lit: {}", row(0));
        assert_eq!(row(63), 0, "nothing reaches the bottom edge");
        let top: usize = (0..16).map(row).sum();
        let bottom: usize = (48..64).map(row).sum();
        assert!(top > bottom * 3, "{top} against {bottom}");
    }

    #[test]
    fn a_bright_cover_lights_no_more_dots_than_a_moderate_one() {
        // A moderate cover sits just under the cap; a bright one is thinned
        // to it, so they light about as many dots.
        let moderate = lit(&raster(&flat([102, 60, 90]), 64, 64, true, 0.6));
        let bright = lit(&raster(&flat([230, 240, 255]), 64, 64, true, 0.6));
        assert!(moderate > 0);
        assert!(
            bright as f32 <= moderate as f32 * 1.1,
            "a bright cover lit {bright} dots against {moderate}"
        );
        // The same holds for a dark cover on the light theme.
        let moderate = lit(&raster(&flat([255, 200, 153]), 64, 64, false, 0.6));
        let dark = lit(&raster(&flat([20, 10, 30]), 64, 64, false, 0.6));
        assert!(moderate > 0);
        assert!(
            dark as f32 <= moderate as f32 * 1.1,
            "a dark cover inked {dark} against {moderate}"
        );
    }

    #[test]
    fn a_dark_dot_wears_the_covers_colour_at_full_brightness() {
        let image = raster(&flat([100, 50, 0]), 8, 8, true, 1.0);
        let dot = image.pixels.iter().find(|pixel| pixel.a() > 0).unwrap();
        assert_eq!(dot.to_srgba_unmultiplied(), [255, 128, 0, 255]);
    }

    #[test]
    fn black_lights_nothing_in_the_dark_and_white_inks_nothing_in_the_light() {
        assert_eq!(lit(&raster(&flat([0, 0, 0]), 32, 32, true, 1.0)), 0);
        assert_eq!(lit(&raster(&flat([255, 255, 255]), 32, 32, false, 1.0)), 0);
    }

    #[test]
    fn a_light_dot_stays_dark_enough_to_read_on_white() {
        let image = raster(&flat([255, 230, 120]), 16, 16, false, 1.0);
        for dot in image.pixels.iter().filter(|pixel| pixel.a() > 0) {
            let [r, g, b, _] = dot.to_srgba_unmultiplied();
            let lightness = (r.max(g).max(b) as f32 + r.min(g).min(b) as f32) / 510.0;
            assert!(lightness <= 0.46, "{lightness}");
        }
    }

    #[test]
    fn strength_sets_every_dots_opacity() {
        let image = raster(&flat([40, 200, 120]), 16, 16, true, 0.5);
        assert!(
            image
                .pixels
                .iter()
                .filter(|pixel| pixel.a() > 0)
                .all(|pixel| pixel.a() == 128)
        );
    }

    #[test]
    fn the_cover_fills_a_wide_header_cropped_about_its_centre() {
        // Top half red, bottom half blue: a wide header shows the middle band,
        // where both meet, not just the top.
        let mut pixels = vec![[255, 0, 0]; 8];
        pixels.extend(vec![[0, 0, 255]; 8]);
        let source = Source::from_rgb(4, 4, pixels).unwrap();
        let top = source.sample(400, 100, 200.0, 0.5);
        let bottom = source.sample(400, 100, 200.0, 99.5);
        assert!(top[0] > top[2] && bottom[2] > bottom[0]);
        assert!(
            top[2] > 0.0 && bottom[0] > 0.0,
            "the band is cropped from the centre"
        );
    }

    #[test]
    fn the_grid_is_two_physical_pixels_a_dot_and_bounded() {
        let rect = Rect::from_min_size(pos2(0.0, 0.0), egui::vec2(800.0, 340.0));
        assert_eq!(grid(rect, 1.0), (400, 170));
        assert_eq!(grid(rect, 2.0), (800, 340));
        let huge = Rect::from_min_size(pos2(0.0, 0.0), egui::vec2(9000.0, 9000.0));
        assert_eq!(grid(huge, 2.0), (MAX_COLUMNS, MAX_ROWS));
    }

    #[test]
    fn a_source_needs_one_pixel_per_cell() {
        assert!(Source::from_rgb(2, 2, vec![[0, 0, 0]; 3]).is_none());
        assert!(Source::from_rgb(0, 0, Vec::new()).is_none());
    }
}
