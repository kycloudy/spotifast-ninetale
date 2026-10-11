//! The Ninetale wordmark above the sidebar's Home row, on Home.
//!
//! `assets/logo/ninetale.png` is the design's animation, one pixel per dot
//! and its 24 frames stacked top to bottom: a band of solid dots sweeps
//! across the letters while the rest thin out. Scaled to the sidebar, those
//! dots would blur or shimmer, so each frame is dithered again for the
//! screen, one dot per physical pixel. Each letter keeps its edge, and
//! inside it the dots follow how densely the frame fills that spot.
//!
//! The wordmark moves only while the window has focus, so an idle app in
//! the background never wakes to draw it.
//!
//! The texture is white and painted in the theme's text colour, so it is
//! ink on a light theme and light on a dark one, as the design shows.

use std::sync::OnceLock;

use egui::{Color32, ColorImage, Rect, Sense, TextureHandle, TextureOptions, Vec2, pos2};

use crate::theme::Palette;

/// The wordmark's height on screen.
pub const HEIGHT: f32 = 20.0;
/// The design's frames and how long each shows.
const FRAMES: usize = 24;
const FRAME_SECONDS: f64 = 0.08;

/// Only time spent focused advances the wordmark. Playback and artwork
/// can still repaint an unfocused window without moving its dots.
#[derive(Clone, Default)]
struct Animation {
    elapsed: f64,
    last_focused: Option<f64>,
}

impl Animation {
    fn step(&mut self, time: f64, focused: bool) -> f64 {
        if focused {
            if let Some(last) = self.last_focused {
                self.elapsed += (time - last).max(0.0);
            }
            self.last_focused = Some(time);
        } else {
            self.last_focused = None;
        }
        self.elapsed / FRAME_SECONDS
    }
}

const BAYER: [[u8; 4]; 4] = [[0, 8, 2, 10], [12, 4, 14, 6], [3, 11, 1, 9], [15, 7, 13, 5]];
/// Each screen pixel is judged from this many samples a side.
const SUBSAMPLES: usize = 4;

/// The design's dots, with the letters' outlines and how densely each
/// frame fills each spot worked out from them.
struct Design {
    width: usize,
    height: usize,
    /// 1 inside a letter, 0 outside: every frame's dots together, with the
    /// gaps between neighbours closed. The letters never move, so one
    /// outline serves every frame, even one whose dots are sparse.
    shape: Vec<f32>,
    /// For each frame, the share of a letter's dots lit around each dot,
    /// 0 to 1.
    density: Vec<Vec<f32>>,
}

impl Design {
    fn load() -> &'static Self {
        static DESIGN: OnceLock<Design> = OnceLock::new();
        DESIGN.get_or_init(|| {
            let image = image::load_from_memory(include_bytes!("../../assets/logo/ninetale.png"))
                .expect("the bundled wordmark decodes")
                .to_luma_alpha8();
            let (width, height) = (image.width() as usize, image.height() as usize / FRAMES);
            let dots: Vec<bool> = image.pixels().map(|pixel| pixel[1] > 127).collect();
            let frames = dots
                .chunks(width * height)
                .take(FRAMES)
                .map(<[bool]>::to_vec)
                .collect();
            Self::from_frames(width, height, frames)
        })
    }

    fn from_frames(width: usize, height: usize, frames: Vec<Vec<bool>>) -> Self {
        let at = |grid: &[bool], x: isize, y: isize| {
            x >= 0
                && y >= 0
                && (x as usize) < width
                && (y as usize) < height
                && grid[y as usize * width + x as usize]
        };
        let around = |x: usize, y: usize| {
            (-1..=1).flat_map(move |dy| (-1..=1).map(move |dx| (x as isize + dx, y as isize + dy)))
        };
        let all: Vec<bool> = (0..width * height)
            .map(|index| frames.iter().any(|dots| dots[index]))
            .collect();
        // Closing: grow every dot by one, then shrink back, which fills the
        // dither's gaps but keeps each letter's outline.
        let grown: Vec<bool> = (0..width * height)
            .map(|index| around(index % width, index / width).any(|(x, y)| at(&all, x, y)))
            .collect();
        let shape: Vec<bool> = (0..width * height)
            .map(|index| around(index % width, index / width).all(|(x, y)| at(&grown, x, y)))
            .collect();
        let density = frames
            .iter()
            .map(|dots| {
                (0..width * height)
                    .map(|index| {
                        let (x, y) = (index % width, index / width);
                        let inside = around(x, y).filter(|&(x, y)| at(&shape, x, y)).count();
                        let lit = around(x, y)
                            .filter(|&(x, y)| at(&shape, x, y) && at(dots, x, y))
                            .count();
                        lit as f32 / inside.max(1) as f32
                    })
                    .collect()
            })
            .collect();
        Self {
            width,
            height,
            shape: shape
                .into_iter()
                .map(|inside| inside as u8 as f32)
                .collect(),
            density,
        }
    }

    /// The wordmark's width for a height of `rows` pixels.
    fn columns(&self, rows: usize) -> usize {
        (rows as f32 * self.width as f32 / self.height as f32).round() as usize
    }

    /// Frame `frame` of the wordmark `rows` pixels tall, white dots on
    /// transparent.
    fn raster(&self, frame: usize, rows: usize) -> ColorImage {
        let density_of = &self.density[frame];
        let rows = rows.max(1);
        let columns = self.columns(rows).max(1);
        let scale_x = self.width as f32 / columns as f32;
        let scale_y = self.height as f32 / rows as f32;
        let mut pixels = vec![Color32::TRANSPARENT; columns * rows];
        for row in 0..rows {
            for column in 0..columns {
                let (mut shape, mut density) = (0.0, 0.0);
                for sy in 0..SUBSAMPLES {
                    for sx in 0..SUBSAMPLES {
                        let x = (column as f32 + (sx as f32 + 0.5) / SUBSAMPLES as f32) * scale_x;
                        let y = (row as f32 + (sy as f32 + 0.5) / SUBSAMPLES as f32) * scale_y;
                        let index = (y as usize).min(self.height - 1) * self.width
                            + (x as usize).min(self.width - 1);
                        shape += self.shape[index];
                        density += density_of[index] * self.shape[index];
                    }
                }
                if shape < (SUBSAMPLES * SUBSAMPLES) as f32 / 2.0 {
                    continue;
                }
                let threshold = (BAYER[row % 4][column % 4] as f32 + 0.5) / 16.0;
                if density / shape > threshold {
                    pixels[row * columns + column] = Color32::WHITE;
                }
            }
        }
        ColorImage::new([columns, rows], pixels)
    }
}

/// The wordmark's size in points at `pixels_per_point`, and its height in
/// physical pixels.
fn size(pixels_per_point: f32) -> (Vec2, usize) {
    let rows = (HEIGHT * pixels_per_point).round().max(1.0) as usize;
    let columns = Design::load().columns(rows);
    (
        Vec2::new(columns as f32, rows as f32) / pixels_per_point,
        rows,
    )
}

/// Draws the wordmark in the next space of `ui`'s layout.
pub fn show(ui: &mut egui::Ui, palette: &Palette) -> egui::Response {
    let ppp = ui.ctx().pixels_per_point();
    let (points, rows) = size(ppp);
    let (rect, response) = ui.allocate_exact_size(points, Sense::hover());
    ui.ctx().accesskit_node_builder(response.id, |node| {
        node.set_role(egui::accesskit::Role::Image);
        node.set_label("Ninetale");
    });
    if !ui.is_rect_visible(rect) {
        return response;
    }
    let id = egui::Id::new("ninetale-logo");
    let textures = ui
        .ctx()
        .data(|data| data.get_temp::<(usize, Vec<TextureHandle>)>(id))
        .filter(|(made_for, _)| *made_for == rows)
        .map(|(_, textures)| textures)
        .unwrap_or_else(|| {
            let textures: Vec<TextureHandle> = (0..FRAMES)
                .map(|frame| {
                    ui.ctx().load_texture(
                        format!("ninetale-logo-{frame}"),
                        Design::load().raster(frame, rows),
                        TextureOptions::NEAREST,
                    )
                })
                .collect();
            ui.ctx()
                .data_mut(|data| data.insert_temp(id, (rows, textures.clone())));
            textures
        });
    let (time, focused) = ui.input(|input| {
        (
            input.time,
            input.viewport().focused.unwrap_or(input.focused),
        )
    });
    let step = ui.ctx().data_mut(|data| {
        data.get_temp_mut_or_default::<Animation>(id.with("animation"))
            .step(time, focused)
    });
    let texture = &textures[step as usize % FRAMES];
    if focused {
        let next = (step.floor() + 1.0 - step) * FRAME_SECONDS;
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_secs_f64(next.max(0.001)));
    }
    // On whole physical pixels, so each dot is exactly one.
    let min = pos2((rect.min.x * ppp).round(), (rect.min.y * ppp).round()) / ppp;
    ui.painter().image(
        texture.id(),
        Rect::from_min_size(min, points),
        Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
        palette.text,
    );
    response
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn background_redraws_hold_the_frame_and_focus_resumes_it() {
        let mut animation = Animation::default();
        assert_eq!(animation.step(10.0, true), 0.0);
        let playing = animation.step(10.2, true);
        assert!((playing - 2.5).abs() < 1e-10);
        for time in [10.3, 12.0, 120.0] {
            assert_eq!(animation.step(time, false), playing);
        }
        assert_eq!(animation.step(130.0, true), playing);
        assert!((animation.step(130.08, true) - playing - 1.0).abs() < 1e-10);
    }

    #[test]
    fn viewport_focus_holds_the_drawn_texture_during_other_repaints() {
        let ctx = egui::Context::default();
        let draw = |time, focused| {
            let mut input = egui::RawInput {
                time: Some(time),
                ..Default::default()
            };
            input
                .viewports
                .get_mut(&egui::ViewportId::ROOT)
                .unwrap()
                .focused = Some(focused);
            let mut output = ctx.run_ui(input, |ui| {
                show(ui, &Palette::dark());
            });
            output.textures_delta.clear();
            output
                .shapes
                .iter()
                .find_map(|shape| match &shape.shape {
                    egui::Shape::Mesh(mesh) => Some(mesh.texture_id),
                    _ => None,
                })
                .expect("the wordmark's texture is drawn")
        };
        let first = draw(0.0, true);
        let advanced = draw(0.2, true);
        assert_ne!(first, advanced);
        assert_eq!(draw(1.0, false), advanced);
        assert_eq!(draw(2.0, false), advanced);
        assert_eq!(draw(30.0, true), advanced);
        assert_ne!(draw(30.08, true), advanced);
    }

    fn lit(image: &ColorImage) -> usize {
        image.pixels.iter().filter(|pixel| pixel.a() > 0).count()
    }

    #[test]
    fn the_bundled_design_is_the_wordmark_dot_grid() {
        let design = Design::load();
        assert_eq!((design.width, design.height), (271, 51));
        assert_eq!(design.density.len(), FRAMES);
        assert!(
            design
                .density
                .iter()
                .flatten()
                .all(|value| (0.0..=1.0).contains(value))
        );
    }

    /// Every dot is one physical pixel, so the texture is made for the
    /// screen's scale, keeps the design's proportions, and is never empty.
    #[test]
    fn the_wordmark_is_dithered_for_each_screen_scale() {
        for (ppp, frame) in [(1.0, 0), (1.25, 5), (1.5, 11), (2.0, 17), (3.0, 23)] {
            let (points, rows) = size(ppp);
            let image = Design::load().raster(frame, rows);
            assert_eq!(image.size[1], rows);
            assert!((points.y - HEIGHT).abs() <= 0.5 / ppp, "{ppp}: {points:?}");
            assert!((image.size[0] as f32 / rows as f32 - 271.0 / 51.0).abs() < 0.05);
            let share = lit(&image) as f32 / image.pixels.len() as f32;
            assert!((0.1..0.6).contains(&share), "{ppp}: {share}");
            assert!(
                image
                    .pixels
                    .iter()
                    .all(|pixel| *pixel == Color32::TRANSPARENT || *pixel == Color32::WHITE)
            );
        }
    }

    /// As in the design, a band of solid dots sweeps across the letters:
    /// the "N" is dense early in the loop and sparse after the band has
    /// passed, and the final "e" is sparse midway and dense at the end.
    #[test]
    fn the_dense_band_sweeps_across_the_letters() {
        let lit_in = |frame: usize, from: f32, to: f32| {
            let image = Design::load().raster(frame, 52);
            let [columns, rows] = image.size;
            let (from, to) = (
                (from * columns as f32) as usize,
                (to * columns as f32) as usize,
            );
            (0..rows)
                .flat_map(|row| (from..to).map(move |column| row * columns + column))
                .filter(|&index| image.pixels[index].a() > 0)
                .count()
        };
        let (n, e) = ((0.0, 0.15), (0.85, 1.0));
        assert!(lit_in(5, n.0, n.1) > lit_in(18, n.0, n.1));
        assert!(lit_in(23, e.0, e.1) > lit_in(14, e.0, e.1));
    }
}
