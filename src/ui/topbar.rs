//! Navigation arrows, the page's name, and the account menu above every
//! page, in Zeron's compact bar. Search lives in the sidebar; while the
//! sidebar is hidden, the bar takes the field back.

use std::sync::Arc;

use egui::{Align, CornerRadius, Galley, Layout, Sense, Vec2, pos2, vec2};

use crate::api::models::pick_image;
use crate::app::App;
use crate::i18n::gettext;
use crate::model::{Action, Page};
use crate::theme::{self, Icon, Palette};

/// The gap the bar keeps between everything it lays out.
const ITEM_SPACING: f32 = 8.0;
/// The account avatar, and the icon in each of the three buttons beside it.
const AVATAR_SIZE: f32 = 28.0;
const ICON_BUTTON_ICON: f32 = 16.0;
/// `theme::icon_button` pads its icon by 12 px.
const ICON_BUTTON_SIZE: f32 = ICON_BUTTON_ICON + 12.0;
/// The back and forward buttons.
const NAV_SIZE: f32 = 28.0;
/// The page's name: a quiet pill as tall as the buttons.
const PILL_HEIGHT: f32 = 28.0;
const SPINNER_SIZE: f32 = 15.0;
/// A badge is as tall as its text plus this, and as wide as its text plus
/// the padding its own label needs.
const BADGE_PADDING_Y: f32 = 12.0;
/// The text starts 24 px in; leave 8 px after it to match the space before
/// the icon.
const UPDATE_BADGE_PADDING: f32 = 32.0;
/// The width the search field, or the page's pill, aims for, the most it
/// ever takes, and the least it shrinks to before the badge gives up its
/// label instead.
const SEARCH_IDEAL: f32 = 200.0;
const SEARCH_MAX: f32 = 440.0;
const SEARCH_FLOOR: f32 = 130.0;
// After the badges collapse, a right panel can leave less than 130 points.
// Keep the original 80-point minimum inside the page's own toolbar.
const SEARCH_MIN: f32 = 80.0;
/// Everything at the right end whose width never changes: the page padding,
/// the avatar, the gap the account menu leaves, the three icon buttons, and
/// the spacing between them. The cursor stops at the left edge of the last
/// button, so this counts three gaps, not four. The spinner and the badge
/// are measured on top of it because they come and go.
const RIGHT_CONTROLS_WIDTH: f32 =
    super::widgets::PAGE_PADDING + AVATAR_SIZE + 4.0 + 3.0 * ICON_BUTTON_SIZE + 3.0 * ITEM_SPACING;

/// What precedes the field until the bar has drawn once: the page padding,
/// the back and forward buttons and the gaps after them.
const LEAD_GUESS: f32 = super::widgets::PAGE_PADDING + 2.0 * NAV_SIZE + 3.0 * ITEM_SPACING + 4.0;
/// A badge collapsed to its icon: a square as tall as its 12.5 pt label.
const BADGE_CHIP: f32 = 15.0 + BADGE_PADDING_Y;

fn lead_id() -> egui::Id {
    egui::Id::new("topbar-lead")
}

/// The narrowest the bar, and so the page under it, can be before its
/// controls run into each other: the narrowest field, with the spinner and
/// the update badge as an icon. Counting them even while they are away keeps
/// the panels and the window from changing width as they come and go.
pub fn least_width(ctx: &egui::Context) -> f32 {
    let lead = ctx
        .data(|data| data.get_temp(lead_id()))
        .unwrap_or(LEAD_GUESS);
    least_width_after(lead)
}

fn least_width_after(lead: f32) -> f32 {
    lead + SEARCH_MIN
        + RIGHT_CONTROLS_WIDTH
        + SPINNER_SIZE
        + ITEM_SPACING
        + ITEM_SPACING
        + BADGE_CHIP
}

/// How the top bar divides itself for one window width.
#[derive(Clone, Copy, Debug, PartialEq)]
struct TopbarFit {
    /// How wide the search field, or the page's pill, may be.
    search: f32,
    /// Whether the badge has the room to spell itself out.
    labels: bool,
}

/// Divide the bar. The search field keeps the half it has always had, but
/// never so much that the right end has to reach over it, and the badge
/// falls back to its icon before the field shrinks past reading size.
///
/// `labelled` and `icons` are what the badge asks for with and without its
/// text, each already including the spacing that precedes it.
fn topbar_fit(room: f32, controls: f32, labelled: f32, icons: f32) -> TopbarFit {
    // SEARCH_IDEAL is above SEARCH_FLOOR, so the clamp below is well ordered.
    let ideal = (room * 0.5).clamp(SEARCH_IDEAL, SEARCH_MAX);
    let labels = room - controls - labelled >= SEARCH_FLOOR;
    let badges = if labels { labelled } else { icons };
    TopbarFit {
        search: (room - controls - badges).clamp(SEARCH_MIN, ideal),
        labels,
    }
}

/// What a badge asks of the bar, including the spacing before it.
fn badge_width(galley: Option<&Arc<Galley>>, padding: f32, labels: bool) -> f32 {
    galley.map_or(0.0, |galley| {
        ITEM_SPACING
            + if labels {
                galley.size().x + padding
            } else {
                galley.size().y + BADGE_PADDING_Y
            }
    })
}

/// A pill at the right end of the bar: an icon with its label, or the icon
/// alone once the bar is too narrow to spare the room for words.
fn badge(
    ui: &mut egui::Ui,
    palette: &Palette,
    icon: Icon,
    galley: Arc<Galley>,
    padding: f32,
    labels: bool,
) -> egui::Response {
    let height = galley.size().y + BADGE_PADDING_Y;
    let size = if labels {
        vec2(galley.size().x + padding, height)
    } else {
        Vec2::splat(height)
    };
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    // Collapsed to its icon the badge has no text on screen, so its label
    // reaches a screen reader as the widget's name.
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Button, ui.is_enabled(), galley.text())
    });
    ui.painter().rect_filled(
        rect,
        CornerRadius::same(14),
        palette.accent.gamma_multiply(0.16),
    );
    let icon_center = if labels {
        pos2(rect.left() + 14.0, rect.center().y)
    } else {
        rect.center()
    };
    icon.image(palette.accent, 13.0).paint_at(
        ui,
        egui::Rect::from_center_size(icon_center, Vec2::splat(13.0)),
    );
    if labels {
        ui.painter().galley(
            pos2(rect.left() + 24.0, rect.center().y - galley.size().y / 2.0),
            galley,
            palette.accent,
        );
    }
    response
}

fn nav_button(
    ui: &mut egui::Ui,
    palette: &Palette,
    icon: Icon,
    enabled: bool,
    tooltip: &str,
) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(
        Vec2::splat(NAV_SIZE),
        if enabled {
            Sense::click()
        } else {
            Sense::hover()
        },
    );
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, enabled, tooltip));
    if ui.is_rect_visible(rect) {
        if enabled && response.hovered() {
            ui.painter().rect_filled(
                rect,
                f32::from(theme::RADIUS_SMALL + 2),
                palette.surface_hover,
            );
        }
        let color = if !enabled {
            palette.dim
        } else if response.hovered() {
            palette.text
        } else {
            palette.secondary
        };
        theme::paint_icon(ui, icon, rect, 16.0, color);
    }
    theme::focus_ring(ui, &response);
    if enabled {
        response.on_hover_text(tooltip)
    } else {
        response
    }
}

/// The shown page's name in a quiet pill, as Zeron names its view.
fn page_pill(ui: &mut egui::Ui, palette: &Palette, icon: Icon, label: &str, max_width: f32) {
    let font = theme::medium(13.0);
    let mut job = egui::text::LayoutJob::simple_singleline(label.to_owned(), font, palette.text);
    // Icon, its gap and the padding either side.
    let chrome = 10.0 + 14.0 + 6.0 + 10.0;
    job.wrap = egui::text::TextWrapping::truncate_at_width((max_width - chrome).max(24.0));
    let galley = ui.painter().layout_job(job);
    let size = vec2(chrome + galley.size().x, PILL_HEIGHT);
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    let response = ui.interact(rect, egui::Id::new("page-pill"), Sense::hover());
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Label, true, label));
    if ui.is_rect_visible(rect) {
        ui.painter()
            .rect_filled(rect, f32::from(theme::RADIUS - 2), pill_fill(palette));
        let icon_rect = egui::Rect::from_center_size(
            pos2(rect.left() + 17.0, rect.center().y),
            Vec2::splat(14.0),
        );
        icon.image(palette.text, 14.0).paint_at(ui, icon_rect);
        ui.painter().galley(
            pos2(rect.left() + 30.0, rect.center().y - galley.size().y / 2.0),
            galley,
            palette.text,
        );
    }
}

/// The pill's fill: the page shows faintly through it.
fn pill_fill(palette: &Palette) -> egui::Color32 {
    if palette.dark {
        egui::Color32::from_white_alpha(18)
    } else {
        egui::Color32::from_black_alpha(13)
    }
}

/// The one search field for all of Spotify, wherever it is drawn: in the
/// sidebar, or in this bar while the sidebar is hidden. Typing opens the
/// search page; Enter searches at once; Escape lets go of the field.
pub(crate) fn global_search(app: &mut App, ui: &mut egui::Ui, width: f32, hint: &str) {
    let palette = app.palette;
    let id = egui::Id::new("global-search");
    let before = app.search.query.clone();
    let (response, rect) = super::widgets::search_field_in(
        ui,
        &palette,
        app.locale,
        id,
        &mut app.search.query,
        hint,
        width,
    );
    if app.search.query.is_empty() && !response.has_focus() {
        super::widgets::search_shortcut_hint(
            ui,
            &palette,
            rect,
            super::keys::platform_shortcut("Ctrl F", "Cmd F"),
        );
    }
    if app.search.focus_requested {
        app.search.focus_requested = false;
        response.request_focus();
    }
    // Clear empties the field and hands it focus in the same frame.
    // Neither should leave the page: only typing a query does.
    let cleared = app.search.query.is_empty() && !before.is_empty();
    if response.gained_focus() && !cleared && !matches!(app.page(), Page::Search) {
        app.actions.push(Action::Open(Page::Search));
    }
    if app.search.query != before {
        app.search.typed_at = Some(std::time::Instant::now());
        app.search.from_home = false;
        if !cleared && !matches!(app.page(), Page::Search) {
            app.actions.push(Action::Open(Page::Search));
        }
    }
    if response.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter)) {
        let query = app.search.query.clone();
        app.actions.push(Action::Search(query));
    }
    if response.has_focus() && ui.input(|input| input.key_pressed(egui::Key::Escape)) {
        response.surrender_focus();
    }
}

/// Centres the toggle's icon on the sidebar Home row's icon.
pub(crate) const SIDEBAR_TOGGLE_LEFT: f32 = 16.0;

/// The same control and window position whether the sidebar is open or shut.
pub(crate) fn sidebar_toggle(app: &mut App, ui: &mut egui::Ui) -> egui::Response {
    let (_, slot) = ui.allocate_space(Vec2::splat(NAV_SIZE));
    let center_y = ui.ctx().content_rect().top()
        + theme::titlebar_inset(ui.ctx())
        + theme::TOP_BAR_HEIGHT / 2.0;
    let rect = egui::Rect::from_center_size(pos2(slot.center().x, center_y), Vec2::splat(NAV_SIZE));
    let mut button = ui.new_child(egui::UiBuilder::new().max_rect(rect));
    let (windows, mac) = if app.settings.sidebar_visible {
        (
            gettext(app.locale, "Hide sidebar (Ctrl+B)"),
            gettext(app.locale, "Hide sidebar (Cmd+B)"),
        )
    } else {
        (
            gettext(app.locale, "Show sidebar (Ctrl+B)"),
            gettext(app.locale, "Show sidebar (Cmd+B)"),
        )
    };
    let response = nav_button(
        &mut button,
        &app.palette,
        Icon::PanelLeft,
        true,
        super::keys::platform_shortcut(&windows, &mac),
    );
    if response.clicked() {
        app.actions.push(Action::ToggleSidebar);
    }
    response
}

pub fn show(app: &mut App, ui: &mut egui::Ui) {
    let palette = app.palette;
    let locale = app.locale;
    let width = ui.available_width();
    let window_controls = super::window_controls_reservation(
        ui.ctx(),
        app.show_queue_panel,
        app.show_lyrics_panel,
        width,
    );
    // Where the titlebar used to be: the bar grows upwards into that space and
    // its empty parts drag the window.
    let inset = theme::titlebar_inset(ui.ctx());
    let content_height = theme::TOP_BAR_HEIGHT + inset;
    if crate::window::custom_titlebar() {
        super::titlebar_drag(
            ui,
            egui::Rect::from_min_size(
                ui.cursor().min,
                vec2(width, content_height + window_controls.topbar_top),
            ),
        );
    }
    ui.add_space(window_controls.topbar_top);
    ui.allocate_ui_with_layout(
        vec2(width, content_height),
        Layout::left_to_right(Align::Center),
        |ui| {
            ui.add_space(if app.settings.sidebar_visible {
                super::widgets::PAGE_PADDING
            } else {
                SIDEBAR_TOGGLE_LEFT
            });
            ui.spacing_mut().item_spacing.x = ITEM_SPACING;
            if !app.settings.sidebar_visible {
                sidebar_toggle(app, ui);
                ui.add_space(2.0);
            }
            if !app.settings.sidebar_visible
                && nav_button(ui, &palette, Icon::House, true, &gettext(locale, "Home")).clicked()
            {
                app.actions.push(Action::Open(Page::Home));
            }
            if nav_button(
                ui,
                &palette,
                Icon::ArrowLeft,
                app.can_go_back(),
                &gettext(locale, "Back"),
            )
            .clicked()
            {
                app.actions.push(Action::Back);
            }
            if nav_button(
                ui,
                &palette,
                Icon::ArrowRight,
                app.can_go_forward(),
                &gettext(locale, "Forward"),
            )
            .clicked()
            {
                app.actions.push(Action::Forward);
            }
            ui.add_space(4.0);

            // The badge sits at the right end but grows with its text, so
            // measure it here, before the field or the pill takes its share.
            let update = app.update.clone();
            let update_galley = update.as_ref().map(|update| {
                let label = match &app.update_download {
                    crate::updates::DownloadState::Ready(_) => {
                        gettext(locale, "Update ready").into_owned()
                    }
                    crate::updates::DownloadState::Downloading { .. } => {
                        gettext(locale, "Downloading update…").into_owned()
                    }
                    _ => {
                        // Translators: {version} is a version number such as 1.2.0.
                        gettext(locale, "Update to {version}").replace("{version}", &update.version)
                    }
                };
                ui.painter()
                    .layout_no_wrap(label, theme::medium(12.5), palette.accent)
            });
            // Ask once, so the bar reserves room for exactly the spinner it
            // then draws.
            let busy = app
                .backend
                .activity()
                .busy(std::time::Duration::from_millis(1000));
            let badges =
                |labels: bool| badge_width(update_galley.as_ref(), UPDATE_BADGE_PADDING, labels);
            let controls = RIGHT_CONTROLS_WIDTH
                + if busy {
                    SPINNER_SIZE + ITEM_SPACING
                } else {
                    0.0
                };

            let search_room = (ui.available_width() - window_controls.topbar_width).max(0.0);
            // What sits before the field: the page padding, the navigation
            // buttons, and any window buttons. The panels beside the page
            // keep room for it (`least_width`).
            let lead = width - ui.available_width() + window_controls.topbar_width;
            ui.ctx().data_mut(|data| data.insert_temp(lead_id(), lead));
            let fit = topbar_fit(search_room, controls, badges(true), badges(false));
            // Home has its own search box, so the bar names the page there
            // even while the sidebar is hidden.
            if app.settings.sidebar_visible || matches!(app.page(), Page::Home) {
                let (icon, label) = super::page_label(app);
                page_pill(ui, &palette, icon, &label, fit.search);
            } else {
                global_search(
                    app,
                    ui,
                    fit.search,
                    &gettext(locale, "What do you want to play?"),
                );
            }

            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.add_space(window_controls.topbar_width);
                ui.add_space(super::widgets::PAGE_PADDING);
                // Account.
                let (name, avatar) = app
                    .user
                    .as_ref()
                    .map(|user| {
                        (
                            user.name().to_string(),
                            pick_image(&user.images, 64).map(str::to_string),
                        )
                    })
                    .unwrap_or_default();
                let (rect, response) =
                    ui.allocate_exact_size(Vec2::splat(AVATAR_SIZE), Sense::click());
                if ui.is_rect_visible(rect) {
                    let fill = if response.hovered() {
                        palette.surface_hover
                    } else {
                        palette.surface
                    };
                    ui.painter()
                        .circle_filled(rect.center(), AVATAR_SIZE / 2.0, fill);
                    let inner = egui::Rect::from_center_size(rect.center(), Vec2::splat(24.0));
                    match avatar.as_deref() {
                        Some(url) => super::widgets::paint_cover(
                            ui,
                            &palette,
                            Some(url),
                            inner,
                            12.0,
                            Icon::User,
                            Some(app.backend.art()),
                        ),
                        None => {
                            let initial = name
                                .chars()
                                .next()
                                .unwrap_or('?')
                                .to_uppercase()
                                .to_string();
                            ui.painter()
                                .circle_filled(inner.center(), 12.0, palette.accent);
                            ui.painter().text(
                                inner.center(),
                                egui::Align2::CENTER_CENTER,
                                initial,
                                theme::bold(12.0),
                                palette.on_accent,
                            );
                        }
                    }
                }
                let response = response.on_hover_text(&name);
                egui::Popup::menu(&response)
                    .frame(super::widgets::menu_frame(&palette))
                    .align(egui::RectAlign::BOTTOM_END)
                    .show(|ui| {
                        ui.set_width(200.0);
                        ui.add_space(4.0);
                        ui.horizontal(|ui| {
                            ui.add_space(10.0);
                            theme::text(ui, &name, theme::semibold(14.0), palette.text);
                        });
                        if let Some(product) =
                            app.user.as_ref().and_then(|user| user.product.clone())
                        {
                            ui.horizontal(|ui| {
                                ui.add_space(10.0);
                                theme::text(
                                    ui,
                                    capitalize(&product),
                                    theme::regular(12.0),
                                    palette.secondary,
                                );
                            });
                        }
                        super::widgets::menu_separator(ui, &palette);
                        if super::widgets::menu_item(
                            ui,
                            &palette,
                            Some(Icon::Settings),
                            &gettext(locale, "Settings"),
                        ) {
                            app.actions.push(Action::Open(Page::Settings));
                        }
                        if super::widgets::menu_item(
                            ui,
                            &palette,
                            Some(Icon::Info),
                            &gettext(locale, "Keyboard shortcuts"),
                        ) {
                            app.actions
                                .push(Action::ShowDialog(crate::model::Dialog::Shortcuts));
                        }
                        super::widgets::menu_separator(ui, &palette);
                        if super::widgets::menu_item(
                            ui,
                            &palette,
                            Some(Icon::LogOut),
                            &gettext(locale, "Sign out"),
                        ) {
                            app.actions.push(Action::SignOut);
                        }
                    });
                ui.add_space(4.0);
                if theme::icon_button(
                    ui,
                    Icon::Settings,
                    ICON_BUTTON_ICON,
                    palette.secondary,
                    palette.text,
                    &gettext(locale, "Settings"),
                )
                .clicked()
                {
                    app.actions.push(Action::Open(Page::Settings));
                }
                if theme::icon_button(
                    ui,
                    Icon::AudioLines,
                    ICON_BUTTON_ICON,
                    if app.settings.milkdrop_open {
                        palette.accent
                    } else {
                        palette.secondary
                    },
                    palette.text,
                    super::keys::platform_shortcut(
                        &gettext(locale, "MilkDrop visualiser (Ctrl+Shift+K)"),
                        &gettext(locale, "MilkDrop visualiser (Cmd+Shift+K)"),
                    ),
                )
                .clicked()
                {
                    app.actions.push(Action::ToggleWinampMilkdrop);
                }
                if theme::icon_button(
                    ui,
                    Icon::Shrink,
                    ICON_BUTTON_ICON,
                    palette.secondary,
                    palette.text,
                    super::keys::platform_shortcut(
                        &gettext(locale, "Winamp mini player (Ctrl+M)"),
                        &gettext(locale, "Winamp mini player (Cmd+Shift+M)"),
                    ),
                )
                .clicked()
                {
                    app.actions.push(Action::ToggleWinampWindow);
                }
                // A quiet spinner once the app has been talking to Spotify for a
                // while, long enough that fast requests never flash it.
                if busy {
                    theme::spinner(ui, SPINNER_SIZE, palette.secondary)
                        .on_hover_text(gettext(locale, "Waiting for Spotify…").as_ref());
                }
                // A newer release. Most people never visit a releases page,
                // so the app says so, quietly, until they do.
                if let (Some(galley), Some(update)) = (update_galley, update)
                    && badge(
                        ui,
                        &palette,
                        Icon::Info,
                        galley,
                        UPDATE_BADGE_PADDING,
                        fit.labels,
                    )
                    .on_hover_text(
                        // Translators: {version} is a version number such as 1.2.0.
                        gettext(locale, "Version {version} is available.")
                            .replace("{version}", &update.version),
                    )
                    .clicked()
                {
                    app.actions.push(Action::ShowUpdate);
                }
            });
        },
    );
}

fn capitalize(text: &str) -> String {
    let mut chars = text.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

#[cfg(test)]
mod topbar_fit_tests {
    use super::*;

    // What the badges measure on a bar showing "Playing on MacBook de Luis"
    // and "Update to 0.7.1", each including the spacing before it.
    const DEVICE: f32 = ITEM_SPACING + 176.0;
    const UPDATE: f32 = ITEM_SPACING + 152.0;
    // Collapsed, a badge is a square chip as tall as its text.
    const CHIP: f32 = ITEM_SPACING + 15.0 + BADGE_PADDING_Y;

    /// The narrowest bar the app can produce: a 760 px window, its sidebar,
    /// and the navigation buttons all taken out.
    const NARROWEST_BAR: f32 = 398.0;

    fn right_end(room: f32, labelled: f32, icons: f32) -> f32 {
        let fit = topbar_fit(room, RIGHT_CONTROLS_WIDTH, labelled, icons);
        let badges = if fit.labels { labelled } else { icons };
        RIGHT_CONTROLS_WIDTH + badges - (room - fit.search)
    }

    #[test]
    fn a_wide_bar_keeps_the_field_it_always_had() {
        let fit = topbar_fit(2000.0, RIGHT_CONTROLS_WIDTH, DEVICE + UPDATE, CHIP * 2.0);
        assert_eq!(fit.search, SEARCH_MAX);
        assert!(fit.labels);
        // Half the room, as before, while half still fits.
        let fit = topbar_fit(700.0, RIGHT_CONTROLS_WIDTH, 0.0, 0.0);
        assert_eq!(fit.search, 350.0);
    }

    #[test]
    fn the_right_end_never_reaches_over_the_search_field() {
        let mut room = NARROWEST_BAR;
        while room <= 2400.0 {
            for (labelled, icons) in [
                (0.0, 0.0),
                (DEVICE, CHIP),
                (UPDATE, CHIP),
                (DEVICE + UPDATE, CHIP * 2.0),
            ] {
                let over = right_end(room, labelled, icons);
                assert!(
                    over <= 0.0,
                    "badges overlap the field by {over} px on a {room} px bar"
                );
            }
            room += 1.0;
        }
    }

    #[test]
    fn a_right_panel_can_narrow_search_after_the_badges_collapse() {
        let room = RIGHT_CONTROLS_WIDTH + CHIP * 2.0 + 100.0;
        let fit = topbar_fit(room, RIGHT_CONTROLS_WIDTH, DEVICE + UPDATE, CHIP * 2.0);
        assert!(!fit.labels);
        assert_eq!(fit.search, 100.0);
        assert_eq!(right_end(room, DEVICE + UPDATE, CHIP * 2.0), 0.0);
    }

    #[test]
    fn a_narrow_bar_trades_the_badge_labels_for_their_icons() {
        assert!(topbar_fit(1400.0, RIGHT_CONTROLS_WIDTH, DEVICE, CHIP).labels);
        // The 1080 px window of the report that started this.
        assert!(topbar_fit(952.0, RIGHT_CONTROLS_WIDTH, DEVICE + UPDATE, CHIP * 2.0).labels);
        assert!(!topbar_fit(NARROWEST_BAR, RIGHT_CONTROLS_WIDTH, DEVICE, CHIP).labels);
    }

    /// At the least width the panels leave it, the bar still holds the
    /// spinner and both badges beside the narrowest field (#624).
    #[test]
    fn the_least_width_holds_every_control_beside_the_field() {
        let lead = LEAD_GUESS;
        let room = least_width_after(lead) - lead;
        let controls = RIGHT_CONTROLS_WIDTH + SPINNER_SIZE + ITEM_SPACING;
        // The device badge moved to the player; only the update badge is left.
        let fit = topbar_fit(room, controls, UPDATE, CHIP);
        assert!(!fit.labels);
        assert_eq!(fit.search, SEARCH_MIN);
        assert!(controls + CHIP + fit.search <= room);
    }

    #[test]
    fn the_field_stays_readable_however_tight_the_bar_gets() {
        let mut room = NARROWEST_BAR;
        while room <= 2400.0 {
            let fit = topbar_fit(room, RIGHT_CONTROLS_WIDTH, DEVICE + UPDATE, CHIP * 2.0);
            assert!(fit.search >= SEARCH_FLOOR, "field is {} px", fit.search);
            assert!(fit.search <= SEARCH_MAX);
            room += 1.0;
        }
    }
}
