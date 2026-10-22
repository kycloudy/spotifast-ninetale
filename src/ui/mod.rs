//! Window layout: panels, overlays, keyboard shortcuts.

use std::sync::Arc;

pub mod artist;
pub mod collection;
pub(crate) mod devices;
mod dialogs;
pub(crate) mod friends;
pub mod home;
mod keys;
pub mod library;
pub mod login;
mod logo;
mod lyrics;
pub mod new_releases;
pub mod player_bar;
mod prerelease;
pub mod queue;
pub mod radio;
pub mod search;
pub mod settings;
pub mod show;
pub mod sidebar;
pub mod topbar;
mod update;
pub mod widgets;
pub mod winamp;

use egui::{Align2, Color32, Context, CornerRadius, Frame, Id, Margin, Rect, Stroke, vec2};

use crate::api::models::pick_image;
use crate::app::App;
use crate::backend::AuthStatus;
use crate::model::{Action, Loadable, Page, ToastKind};
use crate::theme::{self, Icon};

pub fn show(app: &mut App, ui: &mut egui::Ui) {
    let ctx = ui.ctx().clone();
    let ctx = &ctx;
    keys::handle(app, ctx);
    for path in winamp::dropped_skins(ctx) {
        app.actions.push(Action::InstallSkin(path));
    }
    let signed_in = app.is_connected() && app.user.is_some();
    let connecting = matches!(app.auth, AuthStatus::Connecting | AuthStatus::Starting)
        || (app.is_connected() && app.user.is_none());
    if !signed_in {
        player_bar::end_tint_session(ctx);
        login::show(app, ui, connecting);
        update::show(app, ctx);
        toasts(app, ctx, 20.0);
        caption_buttons(ui, &app.palette, app.locale);
        fastframe_titlebar::resize(ui);
        return;
    }
    if titlebar_spans_window(cfg!(target_os = "macos"), fastframe_titlebar::enabled()) {
        fastframe_titlebar::drag(
            ui,
            Rect::from_min_size(
                ui.max_rect().min,
                vec2(
                    ui.max_rect().width(),
                    theme::top_bar_height() + theme::titlebar_inset(ui.ctx()),
                ),
            ),
        );
    }
    // The sidebar and the panels beside the page run the window's full
    // height, as Zeron's do; the player floats under the page alone. A page
    // too narrow for the player's controls gives the card the window's
    // width instead, under the panels. The page's width does not depend on
    // where the card sits, so last frame's decides it.
    let fullscreen_lyrics = app.lyrics_fullscreen.is_some();
    let page_width = ctx
        .data(|data| data.get_temp::<f32>(page_width_id()))
        .unwrap_or(f32::INFINITY);
    let card_spans = fullscreen_lyrics || !player_bar::fits_under(page_width);
    if card_spans {
        player_bar::show(app, ui);
    }
    if !fullscreen_lyrics {
        if app.settings.sidebar_visible {
            sidebar::show(app, ui);
        }
        if app.show_queue_panel {
            queue::side_panel(app, ui);
        }
        if app.show_lyrics_panel {
            lyrics::side_panel(app, ui);
        }
        if app.show_friends_panel {
            friends::side_panel(app, ui);
        }
    }
    if !card_spans {
        player_bar::show(app, ui);
    }
    if fullscreen_lyrics {
        lyrics::fullscreen(app, ui);
    } else {
        central(app, ui);
        keep_room_for_panels(app, ctx);
    }
    devices::popup(app, ctx);
    dialogs::show(app, ctx);
    update::show(app, ctx);
    widgets::drag_ghost(ctx, &app.palette, app.locale);
    toasts(app, ctx, theme::PLAYER_BAR_HEIGHT + 16.0);
    caption_buttons(ui, &app.palette, app.locale);
    fastframe_titlebar::resize(ui);
}

/// The main window's narrowest width with these panels open: their least
/// widths and the page's.
fn main_min_width(page: f32, sidebar: bool, right_panel: bool) -> f32 {
    let sidebar = if sidebar { SIDEBAR_MIN_WIDTH } else { 0.0 };
    let right = if right_panel {
        theme::SIDE_PANEL_MIN_WIDTH
    } else {
        0.0
    };
    (sidebar + right + page).max(crate::window::MAIN_MIN_SIZE[0])
}

/// The sidebar's narrowest width.
pub(crate) const SIDEBAR_MIN_WIDTH: f32 = 210.0;

/// Raise the window's minimum width while the Queue or Lyrics panel is open,
/// so even at its narrowest the page beside the panels keeps the room its
/// top bar needs (#624), and lower it again once they close. Window managers
/// that tile ignore the minimum; the panels still give way there.
fn keep_room_for_panels(app: &App, ctx: &Context) {
    if crate::window::fixed_size() {
        return;
    }
    let width = main_min_width(
        topbar::least_width(ctx),
        app.settings.sidebar_visible,
        app.show_queue_panel || app.show_lyrics_panel || app.show_friends_panel,
    )
    .round();
    let id = Id::new("main-min-width");
    let sent = ctx.data(|data| data.get_temp::<f32>(id));
    // The window starts at MAIN_MIN_SIZE, so there is nothing to send until
    // the panels need more.
    if sent.unwrap_or(crate::window::MAIN_MIN_SIZE[0]) != width {
        ctx.data_mut(|data| data.insert_temp(id, width));
        ctx.send_viewport_cmd(egui::ViewportCommand::MinInnerSize(vec2(
            width,
            crate::window::MAIN_MIN_SIZE[1],
        )));
    }
}

/// How a resizable side panel fits beside the page.
pub(crate) struct PanelFit {
    /// The widths the panel may take this frame.
    pub range: std::ops::RangeInclusive<f32>,
    /// Whether the room the page needs holds the panel under the width the
    /// person chose for it.
    pub yielding: bool,
}

/// Fit a side panel between `range` and the page beside it. The panel gives
/// up width, down to its minimum, so the page keeps `room_for_page`, the
/// least its top bar needs (#624). The width chosen for the panel stays in
/// the settings and comes back once the window widens again.
///
/// `room` is what the panel and everything after it share.
pub(crate) fn yielding_panel(
    ctx: &Context,
    id: &str,
    range: std::ops::RangeInclusive<f32>,
    chosen: f32,
    room: f32,
) -> PanelFit {
    let (min, max) = range.into_inner();
    let max = max.min(room).max(min);
    let width = chosen.clamp(min, max);
    // egui remembers the width the panel last drew at. While nobody drags
    // its handle, hand it back the chosen width, either way: a resize made
    // elsewhere (the fullscreen queue column shares the queue width) shows
    // here too, and the panel never writes a stale width back over it.
    let id = Id::new(id);
    if !ctx.is_being_dragged(id.with("__resize"))
        && let Some(mut state) = egui::containers::panel::PanelState::load(ctx, id)
        && (state.outer_rect.width() - width).abs() > 0.5
    {
        state.outer_rect.set_width(width);
        ctx.data_mut(|data| data.insert_persisted(id, state));
    }
    PanelFit {
        range: min..=max,
        yielding: max < chosen - 0.5,
    }
}

/// Whether the person is dragging this panel's resize handle right now.
pub(crate) fn panel_dragged(ctx: &Context, id: &str) -> bool {
    ctx.is_being_dragged(Id::new(id).with("__resize"))
}

/// Whether a panel's new width is one the person chose: it moved while not
/// held back by the page, or by their own drag.
pub(crate) fn panel_width_chosen(ctx: &Context, id: &str, fit: &PanelFit) -> bool {
    !fit.yielding || panel_dragged(ctx, id)
}

/// Keeps the most recent loading preview of each metadata type available to
/// the loaded hero as an artwork fallback. The fixed typed slot bounds this to
/// one playlist, album, artist, and show instead of scanning known metadata on
/// every loaded frame.
fn loading_preview<T>(
    ctx: &Context,
    key: &str,
    details: &Loadable<T>,
    known: impl FnOnce() -> Option<T>,
) -> Option<Arc<T>>
where
    T: Send + Sync + 'static,
{
    let memory_id = Id::new("collection-loading-preview");
    let key = Id::new(key);
    if details.get().is_some() {
        ctx.data(|data| data.get_temp::<(Id, Arc<T>)>(memory_id))
            .filter(|(stored_key, _)| *stored_key == key)
            .map(|(_, preview)| preview)
    } else {
        let preview = known().map(Arc::new);
        ctx.data_mut(|data| match &preview {
            Some(preview) => {
                data.insert_temp(memory_id, (key, Arc::clone(preview)));
            }
            None => data.remove::<(Id, Arc<T>)>(memory_id),
        });
        preview
    }
}

/// The page's own cover, at `target` width, when it has one.
fn page_image(app: &App, page: &Page, target: u32) -> Option<String> {
    match page {
        Page::Playlist(id) => app
            .playlist_pages
            .get(id)
            .and_then(|page| page.playlist.get())
            .or_else(|| app.known_playlist(id))
            .and_then(|playlist| pick_image(&playlist.images, target))
            .map(str::to_string),
        Page::Album(id) => app
            .album_pages
            .get(id)
            .and_then(|page| page.album.get())
            .or_else(|| app.known_album(id))
            .and_then(|album| pick_image(&album.images, target))
            .map(str::to_string),
        Page::Artist(id) => app
            .artist_pages
            .get(id)
            .and_then(|page| page.artist.get())
            .or_else(|| app.known_artist(id))
            .and_then(|artist| pick_image(&artist.images, target))
            .map(str::to_string),
        Page::Show(id) => app
            .show_pages
            .get(id)
            .and_then(|page| page.show.get())
            .or_else(|| app.known_show(id))
            .and_then(|show| pick_image(&show.images, target))
            .map(str::to_string),
        Page::Radio(seed) => pick_image(&app.radio_images(seed), target).map(str::to_string),
        _ => None,
    }
}

/// What the top bar's pill calls the shown page, with its icon.
pub(crate) fn page_label(app: &App) -> (Icon, String) {
    use crate::i18n::gettext;
    let locale = app.locale;
    let named = |name: Option<&str>, fallback: std::borrow::Cow<'_, str>| {
        name.filter(|name| !name.is_empty())
            .map_or_else(|| fallback.into_owned(), str::to_owned)
    };
    match app.page() {
        Page::Home => (Icon::House, gettext(locale, "Home").into_owned()),
        Page::Search => (Icon::Search, gettext(locale, "Search").into_owned()),
        Page::TopSongs => (
            Icon::TrendingUp,
            gettext(locale, "Your top songs").into_owned(),
        ),
        Page::LikedSongs => (Icon::Heart, gettext(locale, "Liked Songs").into_owned()),
        Page::NewReleases => (Icon::Disc, gettext(locale, "New releases").into_owned()),
        Page::Albums => (Icon::Disc, gettext(locale, "Albums").into_owned()),
        Page::Artists => (Icon::Users, gettext(locale, "Artists").into_owned()),
        Page::Podcasts => (Icon::Headphones, gettext(locale, "Podcasts").into_owned()),
        Page::Episodes => (Icon::Bookmark, gettext(locale, "Episodes").into_owned()),
        Page::Playlist(id) => (
            Icon::ListMusic,
            named(
                app.playlist_pages
                    .get(id)
                    .and_then(|page| page.playlist.get())
                    .or_else(|| app.known_playlist(id))
                    .map(|playlist| playlist.name.as_str()),
                gettext(locale, "Playlist"),
            ),
        ),
        Page::Album(id) => (
            Icon::Disc,
            named(
                app.album_pages
                    .get(id)
                    .and_then(|page| page.album.get())
                    .or_else(|| app.known_album(id))
                    .map(|album| album.name.as_str()),
                gettext(locale, "Album"),
            ),
        ),
        Page::Artist(id) => (
            Icon::User,
            named(
                app.artist_pages
                    .get(id)
                    .and_then(|page| page.artist.get())
                    .or_else(|| app.known_artist(id))
                    .map(|artist| artist.name.as_str()),
                gettext(locale, "Artist"),
            ),
        ),
        Page::Show(id) => (
            Icon::Headphones,
            named(
                app.show_pages
                    .get(id)
                    .and_then(|page| page.show.get())
                    .or_else(|| app.known_show(id))
                    .map(|show| show.name.as_str()),
                gettext(locale, "Podcast"),
            ),
        ),
        Page::Radio(_) => (Icon::Radio, gettext(locale, "Radio").into_owned()),
        Page::Queue => (Icon::ListVideo, gettext(locale, "Queue").into_owned()),
        Page::Settings => (Icon::Settings, gettext(locale, "Settings").into_owned()),
    }
}

fn page_tint(app: &mut App) -> Option<Color32> {
    let page = app.page().clone();
    if page == Page::LikedSongs {
        return app
            .settings
            .accent_from_art
            .then_some(Color32::from_rgb(0x50, 0x38, 0xc8));
    }
    let image = page_image(app, &page, 64);
    if !app.settings.accent_from_art && image.is_some() {
        return None;
    }
    match image {
        Some(url) => app.tint_for(Some(&url)).or_else(|| app.now_playing_tint()),
        None => app.now_playing_tint(),
    }
}

/// The art dithered behind the page's header: its own cover, else the
/// playing song's. None when art colours are off or the dither is.
fn page_dither_art(app: &App) -> Option<String> {
    if !app.settings.dither_headers || !app.settings.accent_from_art {
        return None;
    }
    page_image(app, app.page(), 640).or_else(|| {
        let now = app.now_playing()?;
        now.art_url.or(now.art_small)
    })
}

/// The page's width, kept for the next frame's player card.
fn page_width_id() -> Id {
    Id::new("page-width")
}

fn central(app: &mut App, ui: &mut egui::Ui) {
    let palette = app.palette;
    let tint = page_tint(app);
    let dither_art = page_dither_art(app);
    egui::CentralPanel::default()
        .frame(Frame::new().fill(palette.window))
        .show(ui, |ui| {
            let rect = ui.max_rect();
            if ui.ctx().data(|data| data.get_temp::<f32>(page_width_id())) != Some(rect.width()) {
                ui.ctx()
                    .data_mut(|data| data.insert_temp(page_width_id(), rect.width()));
                ui.ctx().request_repaint();
            }
            // Home's search box sits inside the art, as Zeron's composer
            // sits in its wallpaper, so Home's dither runs deeper and bolder.
            let page = app.page().clone();
            let home = matches!(page, Page::Home);
            let header_height = if home { 420.0 } else { 340.0 };
            let header = Rect::from_min_size(rect.min, vec2(rect.width(), header_height));
            let quiet = matches!(
                page,
                Page::Home | Page::Search | Page::Settings | Page::Queue
            );
            if let Some((top, bottom)) = header_wash(&palette, quiet, tint) {
                widgets::paint_vertical_gradient(ui, header, top, bottom);
            }
            // The dots sit over the tint, softer on pages whose header is
            // only text, so titles stay easy to read.
            let strength = if home {
                0.34
            } else if quiet {
                0.26
            } else {
                0.36
            };
            // The art stays put while the page scrolls, so it fades as the
            // header leaves and scrolled rows sit on the plain background.
            let scroll_key = Id::new("page-scrolled").with(app.page().encode());
            let scrolled = ui
                .ctx()
                .data(|data| data.get_temp::<f32>(scroll_key))
                .unwrap_or(0.0);
            let opacity = dither_opacity(scrolled);
            let loader = app.backend.art().clone();
            app.dither_hero.paint(
                ui,
                &loader,
                dither_art.as_deref(),
                header,
                crate::dither::Look {
                    dark: palette.dark,
                    strength,
                    opacity,
                    fade_to_bottom: true,
                    corner_radius: 0,
                },
            );
            if dither_art.is_some() && opacity > 0.0 {
                // A soft shade keeps the top bar's controls readable over
                // bright art.
                let shade = Rect::from_min_size(
                    rect.min,
                    vec2(
                        rect.width(),
                        theme::TOP_BAR_HEIGHT + theme::titlebar_inset(ui.ctx()) + 20.0,
                    ),
                );
                widgets::paint_vertical_gradient(
                    ui,
                    shade,
                    palette.window.gamma_multiply(0.7 * opacity),
                    egui::Color32::TRANSPARENT,
                );
            }
            ui.spacing_mut().item_spacing = vec2(8.0, 6.0);
            // egui fades a scrolled page's edge into the panel's plain
            // colour, which shows as a pale band over a cover's tint; the
            // page meets the header with a plain edge instead.
            ui.spacing_mut().scroll.fade.strength = 0.0;
            topbar::show(app, ui);
            let scroll = crate::autoscroll::show(
                ui,
                egui::ScrollArea::vertical()
                    .id_salt(("page", page.encode()))
                    .auto_shrink([false, false]),
                egui::Vec2b::new(false, true),
                |ui| {
                    Frame::new()
                        .inner_margin(Margin {
                            left: widgets::PAGE_PADDING as i8,
                            right: widgets::PAGE_PADDING as i8,
                            top: 4,
                            bottom: 48,
                        })
                        .show(ui, |ui| {
                            ui.set_min_width(ui.available_width());
                            match page {
                                Page::Home => home::show(app, ui),
                                Page::TopSongs => collection::top_songs(app, ui),
                                Page::NewReleases => new_releases::page(app, ui),
                                Page::Search => search::show(app, ui),
                                Page::LikedSongs => collection::liked(app, ui),
                                Page::Albums | Page::Artists | Page::Podcasts | Page::Episodes => {
                                    library::show(app, ui, page)
                                }
                                Page::Playlist(id) => collection::playlist(app, ui, &id),
                                Page::Album(id) => collection::album(app, ui, &id),
                                Page::Artist(id) => artist::show(app, ui, &id),
                                Page::Show(id) => show::show(app, ui, &id),
                                Page::Radio(seed) => radio::radio(app, ui, &seed),
                                Page::Queue => queue::page(app, ui),
                                Page::Settings => settings::show(app, ui),
                            }
                        });
                },
            );
            let offset = scroll.state.offset.y;
            if (offset - scrolled).abs() > 0.5 {
                ui.ctx()
                    .data_mut(|data| data.insert_temp(scroll_key, offset));
                ui.ctx().request_repaint();
            }
            if home {
                home::next_up_pill(app, ui, scroll.inner_rect);
            }
        });
}

/// The gradient behind a page's header, top to bottom: the art's colour
/// blended into the window, stronger on a cover's own page than on an
/// overview, or else the palette's `tint` fading out, whose alpha is its
/// strength. `None` leaves the plain window.
fn header_wash(
    palette: &theme::Palette,
    overview: bool,
    art: Option<Color32>,
) -> Option<(Color32, Color32)> {
    if let Some(art) = art {
        let strength = if overview { 0.45 } else { 0.85 };
        return Some((blend(palette.window, art, strength), palette.window));
    }
    (palette.tint.a() > 0).then_some((palette.tint, Color32::TRANSPARENT))
}

/// How much of the header's dither shows with the page scrolled this far:
/// all of it at the top, none once the header has scrolled away.
fn dither_opacity(scrolled: f32) -> f32 {
    let t = 1.0 - (scrolled / DITHER_SCROLL_FADE).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// How far the page scrolls before the header's dither has faded out.
const DITHER_SCROLL_FADE: f32 = 260.0;

/// Without a native title bar, the top of every panel drags the window, not
/// only the page's top bar.
const fn titlebar_spans_window(on_macos: bool, custom_titlebar: bool) -> bool {
    on_macos || custom_titlebar
}

/// The room the top bar's own controls need beside the caption buttons: the
/// 760-point minimum with the default 250-point sidebar leaves 510 points.
const TOPBAR_MIN_CONTENT: f32 = 510.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct WindowControlsReservation {
    pub topbar_width: f32,
    pub topbar_top: f32,
    pub queue_top: f32,
    pub lyrics_top: f32,
}

/// The caption buttons on the right belong to the outermost header: the
/// queue's or the lyrics' when either panel is open, otherwise the top bar,
/// beside its controls or above them when the page is too narrow.
fn controls_reservation(
    footprint: fastframe_titlebar::Footprint,
    queue: bool,
    lyrics: bool,
    topbar_width: f32,
) -> WindowControlsReservation {
    let mut space = WindowControlsReservation {
        topbar_width: 0.0,
        topbar_top: 0.0,
        queue_top: 0.0,
        lyrics_top: 0.0,
    };
    if footprint.right <= 0.0 {
        return space;
    }
    if queue {
        space.queue_top = footprint.height;
    } else if lyrics {
        space.lyrics_top = footprint.height;
    } else {
        let reserve = footprint.reserve(topbar_width, TOPBAR_MIN_CONTENT);
        space.topbar_width = reserve.inline_width;
        space.topbar_top = reserve.push_down;
    }
    space
}

/// Left-hand Linux controls sit above the sidebar's navigation rows.
pub(super) fn linux_left_controls_inset(ctx: &egui::Context) -> f32 {
    if !cfg!(target_os = "linux") {
        return 0.0;
    }
    let footprint = fastframe_titlebar::footprint(ctx, theme::CUSTOM_TOP_BAR_HEIGHT);
    if footprint.left > 0.0 {
        footprint.height
    } else {
        0.0
    }
}

pub(super) fn window_controls_reservation(
    ctx: &egui::Context,
    queue: bool,
    lyrics: bool,
    topbar_width: f32,
) -> WindowControlsReservation {
    controls_reservation(
        fastframe_titlebar::footprint(ctx, theme::CUSTOM_TOP_BAR_HEIGHT),
        queue,
        lyrics,
        topbar_width,
    )
}

/// Draws the caption buttons over the window's top corners in the palette's
/// colours: Windows' glyphs in the secondary text colour, GNOME's round
/// buttons in the text colour on the hover surface.
fn caption_buttons(ui: &mut egui::Ui, palette: &theme::Palette, locale: crate::i18n::Locale) {
    use crate::i18n::gettext;
    let style = fastframe_titlebar::Style {
        icon: if cfg!(target_os = "linux") {
            palette.text
        } else {
            palette.secondary
        },
        button: palette.surface_hover,
        header_height: theme::CUSTOM_TOP_BAR_HEIGHT,
    };
    // Literal calls, so xgettext extracts each label.
    let minimize = gettext(locale, "Minimize");
    let maximize = gettext(locale, "Maximize");
    let restore = gettext(locale, "Restore");
    let close = gettext(locale, "Close");
    let labels = fastframe_titlebar::Labels {
        minimize: &minimize,
        maximize: &maximize,
        restore: &restore,
        close: &close,
    };
    fastframe_titlebar::controls(ui, &style, &labels);
}

pub fn blend(base: Color32, tint: Color32, amount: f32) -> Color32 {
    let a = egui::Rgba::from(base);
    let b = egui::Rgba::from(tint);
    let mixed = a * (1.0 - amount) + b * amount;
    let mut color = Color32::from(mixed);
    color[3] = 255;
    color
}

fn toasts(app: &mut App, ctx: &egui::Context, bottom_offset: f32) {
    if app.toasts.is_empty() {
        return;
    }
    let palette = app.palette;
    egui::Area::new(egui::Id::new("toasts"))
        .anchor(Align2::RIGHT_BOTTOM, vec2(-20.0, -bottom_offset))
        .order(egui::Order::Tooltip)
        .interactable(false)
        .show(ctx, |ui| {
            ui.spacing_mut().item_spacing.y = 8.0;
            for toast in &app.toasts {
                let age = toast.created.elapsed().as_secs_f32();
                let alpha = if age < 0.15 {
                    age / 0.15
                } else if age > 2.8 {
                    ((3.2 - age) / 0.4).clamp(0.0, 1.0)
                } else {
                    1.0
                };
                ui.set_opacity(alpha);
                Frame::new()
                    .fill(palette.overlay)
                    .stroke(Stroke::new(1.0, palette.outline))
                    .corner_radius(CornerRadius::same(theme::RADIUS))
                    .inner_margin(Margin::symmetric(14, 10))
                    .shadow(egui::epaint::Shadow {
                        offset: [0, 4],
                        blur: 16,
                        spread: 0,
                        color: palette.shadow,
                    })
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            let (icon, color) = match toast.kind {
                                ToastKind::Info => (Icon::CircleCheck, palette.accent),
                                ToastKind::Error => (Icon::CircleAlert, palette.danger),
                            };
                            theme::icon(ui, icon, 16.0, color);
                            // Laid out at its own width. The area
                            // remembers its size, so after a short toast a
                            // label left to wrap at the area's width broke
                            // long messages on every word.
                            let galley = ui.painter().layout(
                                toast.message.clone(),
                                theme::medium(13.5),
                                palette.text,
                                280.0,
                            );
                            ui.add(egui::Label::new(galley));
                        });
                    });
            }
        });
}

#[cfg(test)]
mod window_chrome_tests {
    use super::*;
    use fastframe_titlebar::Footprint;

    /// Windows' three buttons in its 48-point header.
    const BUTTONS: Footprint = Footprint {
        left: 0.0,
        right: 3.0 * 36.0 + 5.0,
        height: theme::CUSTOM_TOP_BAR_HEIGHT,
    };

    #[test]
    fn the_side_panels_drag_a_window_without_a_native_title_bar() {
        assert!(titlebar_spans_window(true, false));
        assert!(titlebar_spans_window(false, true));
        assert!(!titlebar_spans_window(false, false));
    }

    #[test]
    fn caption_space_belongs_to_the_outermost_header() {
        let values = |footprint, queue, lyrics| {
            let space = controls_reservation(footprint, queue, lyrics, f32::INFINITY);
            [
                space.topbar_width,
                space.topbar_top,
                space.queue_top,
                space.lyrics_top,
            ]
        };
        let [width, height] = [BUTTONS.right, BUTTONS.height];
        assert_eq!(values(BUTTONS, false, false), [width, 0.0, 0.0, 0.0]);
        assert_eq!(values(BUTTONS, true, false), [0.0, 0.0, height, 0.0]);
        assert_eq!(values(BUTTONS, false, true), [0.0, 0.0, 0.0, height]);
        assert_eq!(values(BUTTONS, true, true), [0.0, 0.0, height, 0.0]);
        // No buttons on the right: full screen, the native frame, macOS's
        // traffic lights, or a GNOME layout with them all on the left.
        let none = Footprint {
            left: 50.0,
            ..Footprint::default()
        };
        for (queue, lyrics) in [(false, false), (true, false), (false, true)] {
            assert_eq!(values(none, queue, lyrics), [0.0; 4]);
        }
    }

    #[test]
    fn minimum_windows_window_stacks_caption_space_above_the_topbar() {
        let available = 760.0 - 250.0;
        let space = controls_reservation(BUTTONS, false, false, available);
        assert_eq!(space.topbar_width, 0.0);
        assert_eq!(space.topbar_top, BUTTONS.height);

        let inline =
            controls_reservation(BUTTONS, false, false, TOPBAR_MIN_CONTENT + BUTTONS.right);
        assert_eq!(inline.topbar_width, BUTTONS.right);
        assert_eq!(inline.topbar_top, 0.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A page fades from its cover's colour while art tints the interface,
    /// otherwise from the palette's own wash, and from nothing when the
    /// palette has none.
    #[test]
    fn the_header_fades_from_the_art_or_the_palettes_wash() {
        let mut palette = theme::Palette::dark();
        assert_eq!(header_wash(&palette, false, None), None);

        let art = Color32::from_rgb(0x50, 0x38, 0xc8);
        let (page, _) = header_wash(&palette, false, Some(art)).unwrap();
        let (overview, _) = header_wash(&palette, true, Some(art)).unwrap();
        assert_eq!(page, blend(palette.window, art, 0.85));
        assert_eq!(overview, blend(palette.window, art, 0.45));

        palette.tint = Color32::from_black_alpha(24);
        assert_eq!(
            header_wash(&palette, false, None),
            Some((palette.tint, Color32::TRANSPARENT))
        );
        assert_eq!(
            header_wash(&palette, false, Some(art)).unwrap().0,
            page,
            "art wins over the wash"
        );
    }

    /// The header's art stays put while the page scrolls, so it fades out
    /// as the header leaves rather than sit behind the scrolled rows.
    #[test]
    fn the_dither_fades_out_as_the_page_scrolls() {
        assert_eq!(dither_opacity(0.0), 1.0);
        assert!(dither_opacity(DITHER_SCROLL_FADE / 2.0) < 1.0);
        assert!(dither_opacity(DITHER_SCROLL_FADE / 2.0) > 0.0);
        assert!(
            dither_opacity(DITHER_SCROLL_FADE / 4.0) > dither_opacity(DITHER_SCROLL_FADE / 2.0)
        );
        assert_eq!(dither_opacity(DITHER_SCROLL_FADE), 0.0);
        assert_eq!(dither_opacity(DITHER_SCROLL_FADE * 4.0), 0.0);
    }
}
