//! A small, on-demand view of a recording's sample relationships.

use egui::{Align, CornerRadius, Frame, Layout, Margin, Rect, Sense, Stroke, pos2, vec2};

use crate::app::App;
use crate::i18n::gettext;
use crate::model::{Action, Loadable};
use crate::sample_connections::images::Target;
use crate::sample_connections::{Contributor, Found, Query, Recording, View};
use crate::theme::{self, Icon};

/// A contributor's column: the portrait, the name and the roles beneath.
const CREDIT_WIDTH: f32 = 136.0;
const PORTRAIT: f32 = 72.0;
/// A related recording's cover, and the gap before its words.
const ROW_COVER: f32 = 48.0;
const ROW_GAP: f32 = 12.0;

pub(super) fn show(app: &mut App, ctx: &egui::Context, view: &View) {
    let palette = app.palette;
    let locale = app.locale;
    let frame = Frame::new()
        .fill(palette.overlay)
        .stroke(Stroke::new(1.0, palette.outline))
        .corner_radius(CornerRadius::same(theme::RADIUS + 4))
        .inner_margin(Margin::same(24))
        .shadow(egui::epaint::Shadow {
            offset: [0, 10],
            blur: 40,
            spread: 0,
            color: palette.shadow,
        });
    let screen = ctx.content_rect();
    let response = egui::Modal::new(egui::Id::new("sample-connections"))
        .frame(frame)
        .backdrop_color(egui::Color32::from_black_alpha(if palette.dark {
            150
        } else {
            80
        }))
        .show(ctx, |ui| {
            // 16 points of window stay visible on each side of the frame.
            ui.set_width((screen.width() - 80.0).clamp(240.0, 740.0));
            header(app, ui, view);
            ui.add_space(16.0);
            recording_header(app, ui, view);
            ui.add_space(20.0);
            // Each recording opens at its top, not where the last one was left.
            egui::ScrollArea::vertical()
                .id_salt(("sample-relationships", view.request))
                .auto_shrink([false, true])
                .max_height((screen.height() - 300.0).max(120.0))
                .show(ui, |ui| match &view.result {
                    Loadable::NotLoaded | Loadable::Loading => {
                        ui.horizontal(|ui| {
                            theme::spinner(ui, 18.0, palette.accent);
                            ui.add_space(4.0);
                            theme::subtle(
                                ui,
                                &palette,
                                &gettext(locale, "Looking up sample information…"),
                            );
                        });
                    }
                    Loadable::Failed(_) => {
                        ui.horizontal_top(|ui| {
                            theme::icon(ui, Icon::CircleAlert, 16.0, palette.danger);
                            label(
                                ui,
                                &gettext(locale, "Couldn't load sample information. MusicBrainz may be busy. Please try again later."),
                                theme::regular(14.0),
                                palette.secondary,
                                usize::MAX,
                            );
                        });
                        ui.add_space(12.0);
                        if theme::pill_button(ui, &palette, &gettext(locale, "Try again"), false)
                            .clicked()
                        {
                            app.actions
                                .push(Action::SampleConnections(view.query.clone()));
                        }
                    }
                    Loadable::Loaded(Found::Candidates(candidates)) => {
                        if candidates.is_empty() {
                            note(ui, &palette, &gettext(locale, "No matching recording found on MusicBrainz."));
                        } else {
                            note(ui, &palette, &gettext(locale, "Choose the recording that matches your song. Edits and remixes can have different samples."));
                            ui.add_space(16.0);
                            recordings(app, ui, view, candidates, true);
                        }
                    }
                    Loadable::Loaded(Found::Connections(connections)) => {
                        if !connections.recording.disambiguation.is_empty() {
                            note(ui, &palette, &connections.recording.disambiguation);
                            ui.add_space(16.0);
                        }
                        contributors(app, ui, view, &connections.contributors);
                        for (heading, list) in [
                            (gettext(locale, "Samples from"), &connections.samples_from),
                            (gettext(locale, "Sampled by"), &connections.sampled_by),
                        ] {
                            ui.add_space(24.0);
                            theme::section_title(ui, &palette, &format!("{heading} ({})", list.len()));
                            ui.add_space(10.0);
                            if list.is_empty() {
                                note(ui, &palette, &gettext(locale, "No sample information found."));
                            } else {
                                recordings(app, ui, view, list, false);
                            }
                        }
                        ui.add_space(24.0);
                        let source = connections.recording.source_url();
                        if theme::link(
                            ui,
                            gettext(locale, "View recording on MusicBrainz"),
                            theme::semibold(13.0),
                            palette.text,
                        )
                        .clicked()
                        {
                            ui.ctx().open_url(egui::OpenUrl::new_tab(source));
                        }
                        ui.add_space(4.0);
                    }
                });
            ui.add_space(12.0);
            ui.separator();
            ui.add_space(4.0);
            note(
                ui,
                &palette,
                &gettext(locale, "Community metadata from MusicBrainz. Coverage varies by recording."),
            );
        });
    if response.should_close() {
        app.actions.push(Action::CloseDialog);
    }
}

/// The title, with Back once another recording has been explored, and Close.
fn header(app: &mut App, ui: &mut egui::Ui, view: &View) {
    let palette = app.palette;
    let locale = app.locale;
    ui.horizontal(|ui| {
        if !view.history.is_empty()
            && theme::icon_button(
                ui,
                Icon::ChevronLeft,
                20.0,
                palette.secondary,
                palette.text,
                &gettext(locale, "Back"),
            )
            .clicked()
        {
            app.actions.push(Action::SampleBack);
        }
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if theme::icon_button(
                ui,
                Icon::X,
                20.0,
                palette.secondary,
                palette.text,
                &gettext(locale, "Close"),
            )
            .clicked()
            {
                app.actions.push(Action::CloseDialog);
            }
            ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                theme::text(
                    ui,
                    gettext(locale, "Sample connections"),
                    theme::bold(22.0),
                    palette.text,
                );
            });
        });
    });
}

/// The song or recording the view is about, its words centred on its cover.
fn recording_header(app: &mut App, ui: &mut egui::Ui, view: &View) {
    let palette = app.palette;
    ui.horizontal(|ui| {
        let cover = match &view.query {
            Query::Song { cover, .. } => cover.as_deref(),
            Query::Recording(_) => None,
        };
        if let Some(cover) = cover {
            super::widgets::cover(ui, &palette, Some(cover), 64.0, 6.0, Icon::Music);
        } else if let Loadable::Loaded(Found::Connections(connections)) = &view.result {
            picture(
                app,
                ui,
                view,
                Target::Recording(connections.recording.id.clone()),
                64.0,
                None,
            );
        } else {
            super::widgets::cover(ui, &palette, None, 64.0, 6.0, Icon::Music);
        }
        ui.add_space(ROW_GAP - ui.spacing().item_spacing.x);
        let words = ui.available_width();
        ui.allocate_ui_with_layout(vec2(words, 64.0), Layout::top_down(Align::Min), |ui| {
            ui.set_min_height(64.0);
            let title = layout(ui, view.query.title(), theme::bold(20.0), palette.text, 2);
            let artist = layout(
                ui,
                view.query.artist(),
                theme::regular(14.0),
                palette.secondary,
                1,
            );
            let height = title.size().y + 2.0 + artist.size().y;
            ui.add_space(((64.0 - height) / 2.0).max(0.0));
            ui.spacing_mut().item_spacing.y = 2.0;
            ui.add(egui::Label::new(title));
            ui.add(egui::Label::new(artist));
        });
    });
}

fn recordings(app: &mut App, ui: &mut egui::Ui, view: &View, list: &[Recording], candidate: bool) {
    let palette = app.palette;
    for (index, recording) in list.iter().enumerate() {
        if index > 0 {
            ui.add_space(12.0);
            let y = ui.cursor().top();
            ui.painter().hline(
                ui.max_rect().x_range(),
                y,
                Stroke::new(1.0, palette.outline),
            );
            ui.add_space(13.0);
        }
        row(app, ui, view, recording, candidate);
    }
}

/// A related recording: its cover, then its words and actions in one column.
fn row(app: &mut App, ui: &mut egui::Ui, view: &View, recording: &Recording, candidate: bool) {
    let palette = app.palette;
    let locale = app.locale;
    ui.push_id(&recording.id, |ui| {
        ui.horizontal_top(|ui| {
            picture(
                app,
                ui,
                view,
                Target::Recording(recording.id.clone()),
                ROW_COVER,
                None,
            );
            ui.add_space(ROW_GAP - ui.spacing().item_spacing.x);
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 2.0;
                label(ui, &recording.title, theme::semibold(15.0), palette.text, 2);
                let mut detail = recording.artist.clone();
                for extra in [
                    recording.disambiguation.clone(),
                    recording
                        .duration_ms
                        .map(crate::util::format_duration_ms)
                        .unwrap_or_default(),
                ] {
                    if !extra.is_empty() {
                        if !detail.is_empty() {
                            detail.push_str(" · ");
                        }
                        detail.push_str(&extra);
                    }
                }
                label(ui, &detail, theme::regular(13.0), palette.secondary, 2);
                ui.add_space(8.0);
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing = vec2(8.0, 8.0);
                    let text = if candidate {
                        gettext(locale, "Choose recording")
                    } else {
                        gettext(locale, "Explore samples")
                    };
                    if theme::pill_button(ui, &palette, &text, false).clicked() {
                        app.actions.push(Action::SampleConnections(Query::Recording(
                            recording.clone(),
                        )));
                    }
                    if !candidate
                        && theme::pill_button(
                            ui,
                            &palette,
                            &gettext(locale, "Find on Spotify"),
                            false,
                        )
                        .clicked()
                    {
                        app.actions.push(Action::CloseDialog);
                        app.actions.push(Action::Search(recording.spotify_query()));
                    }
                });
            });
        });
    });
}

fn contributors(app: &mut App, ui: &mut egui::Ui, view: &View, credits: &[Contributor]) {
    let palette = app.palette;
    let locale = app.locale;
    theme::section_title(ui, &palette, &gettext(locale, "Contributors"));
    ui.add_space(12.0);
    if credits.is_empty() {
        note(
            ui,
            &palette,
            &gettext(locale, "No contributor information found."),
        );
        return;
    }
    // Columns share a top edge, however many lines a name or its roles take.
    ui.with_layout(
        Layout::left_to_right(Align::Min).with_main_wrap(true),
        |ui| {
            ui.spacing_mut().item_spacing = vec2(12.0, 20.0);
            for credit in credits {
                ui.allocate_ui_with_layout(
                    vec2(CREDIT_WIDTH, PORTRAIT),
                    Layout::top_down(Align::Center),
                    |ui| {
                        ui.set_width(CREDIT_WIDTH);
                        ui.spacing_mut().item_spacing.y = 2.0;
                        let response = picture(
                            app,
                            ui,
                            view,
                            Target::Artist(credit.id.clone()),
                            PORTRAIT,
                            Some(&credit.name),
                        );
                        response.widget_info(|| {
                            egui::WidgetInfo::labeled(egui::WidgetType::Link, true, &credit.name)
                        });
                        theme::focus_ring(ui, &response);
                        if response.clicked() {
                            ui.ctx()
                                .open_url(egui::OpenUrl::new_tab(credit.source_url()));
                        }
                        ui.add_space(8.0);
                        centered(ui, &credit.name, theme::semibold(14.0), palette.text, 2);
                        let roles = credit
                            .roles
                            .iter()
                            .map(|role| match role.as_str() {
                                "main artist" => gettext(locale, "Main artist"),
                                "producer" => gettext(locale, "Producer"),
                                "engineer" => gettext(locale, "Engineer"),
                                "mix" => gettext(locale, "Mixing"),
                                "composer" => gettext(locale, "Composer"),
                                "lyricist" => gettext(locale, "Lyricist"),
                                "writer" => gettext(locale, "Writer"),
                                "instrument" => gettext(locale, "Instrumentalist"),
                                "vocal" => gettext(locale, "Vocals"),
                                _ => role.as_str().into(),
                            })
                            .collect::<Vec<_>>()
                            .join(" · ");
                        centered(ui, &roles, theme::regular(12.0), palette.secondary, 3);
                    },
                );
            }
        },
    );
}

/// Resolving image URLs is an action, applied after drawing like any other request.
/// Offscreen rows do not start network work. Each target is requested once per view.
/// A `name` makes the image a round, clickable portrait that names its subject.
fn picture(
    app: &mut App,
    ui: &mut egui::Ui,
    view: &View,
    target: Target,
    size: f32,
    name: Option<&str>,
) -> egui::Response {
    let round = name.is_some();
    let (rect, response) = ui.allocate_exact_size(
        egui::Vec2::splat(size),
        if round {
            Sense::click()
        } else {
            Sense::hover()
        },
    );
    let picture = view.pictures.get(&target).and_then(Option::as_ref);
    if ui.is_rect_visible(rect) && !view.pictures.contains_key(&target) {
        app.actions.push(Action::SamplePicture {
            request: view.request,
            target,
        });
    }
    super::widgets::paint_cover(
        ui,
        &app.palette,
        picture.map(|p| p.url.as_str()),
        rect,
        if round { size / 2.0 } else { 5.0 },
        if round { Icon::User } else { Icon::Music },
        Some(app.backend.art()),
    );
    let response = if round {
        response.on_hover_cursor(egui::CursorIcon::PointingHand)
    } else {
        response
    };
    if picture.is_none() && name.is_none() {
        return response;
    }
    // One tooltip names the person and credits the photo.
    response.on_hover_ui(|ui| {
        if let Some(name) = name {
            theme::text(ui, name, theme::semibold(13.0), app.palette.text);
        }
        if let Some(picture) = picture {
            let text = layout(
                ui,
                &picture.credit,
                theme::regular(12.0),
                app.palette.secondary,
                usize::MAX,
            );
            ui.hyperlink_to(text, &picture.source);
        }
    })
}

/// Wrapped text laid out in reading order; see [`crate::bidi::layout`].
fn layout(
    ui: &egui::Ui,
    text: &str,
    font: egui::FontId,
    color: egui::Color32,
    max_rows: usize,
) -> std::sync::Arc<egui::Galley> {
    crate::bidi::layout(
        ui.painter(),
        text,
        font,
        color,
        ui.available_width(),
        max_rows,
        Some(crate::bidi::ELLIPSIS),
    )
}

fn label(ui: &mut egui::Ui, text: &str, font: egui::FontId, color: egui::Color32, max_rows: usize) {
    let galley = layout(ui, text, font, color, max_rows);
    ui.add(egui::Label::new(galley).selectable(false));
}

fn note(ui: &mut egui::Ui, palette: &theme::Palette, text: &str) {
    label(
        ui,
        text,
        theme::regular(14.0),
        palette.secondary,
        usize::MAX,
    );
}

/// Text centred line by line under a portrait. A right-to-left paragraph is
/// laid out by [`crate::bidi::layout`], anchored at its own edge.
fn centered(
    ui: &mut egui::Ui,
    text: &str,
    font: egui::FontId,
    color: egui::Color32,
    max_rows: usize,
) {
    let galley = if crate::bidi::is_rtl(text) {
        layout(ui, text, font, color, max_rows)
    } else {
        let mut job =
            egui::text::LayoutJob::simple(text.to_owned(), font, color, ui.available_width());
        job.wrap.max_rows = max_rows;
        // `reorder` cannot move an epaint cut, so only plain text gets one.
        if text.is_ascii() {
            job.wrap.overflow_character = Some(crate::bidi::ELLIPSIS);
        }
        job.halign = Align::Center;
        let mut galley = ui.painter().layout_job(job);
        crate::bidi::reorder(&mut galley);
        galley
    };
    ui.add(egui::Label::new(galley).selectable(false));
}

/// A card beneath Now playing. No provider request happens until it is clicked.
pub(super) fn card(app: &mut App, ui: &mut egui::Ui, track: &crate::api::models::Track) {
    // The tile sits where a song row's cover does, the words where its title does.
    const TILE_LEFT: f32 = 10.0;
    const TILE: f32 = 36.0;
    const WORDS_LEFT: f32 = TILE_LEFT + TILE + 6.0;
    const CHEVRON: f32 = 16.0;
    const WORDS_RIGHT: f32 = 12.0 + CHEVRON + 8.0;
    let Some(query) = Query::from_track(track) else {
        return;
    };
    let palette = app.palette;
    let title = gettext(app.locale, "Sample connections");
    let subtitle = gettext(
        app.locale,
        "Explore samples and the people behind this song",
    );
    let width = ui.available_width();
    let words_width = (width - WORDS_LEFT - WORDS_RIGHT).max(0.0);
    let line = |text: &str, font, color, rows| {
        crate::bidi::layout(
            ui.painter(),
            text,
            font,
            color,
            words_width,
            rows,
            Some(crate::bidi::ELLIPSIS),
        )
    };
    let title_galley = line(&title, theme::semibold(14.0), palette.text, 1);
    // A narrow side panel wraps the subtitle rather than cutting it short.
    let subtitle_galley = line(&subtitle, theme::regular(12.5), palette.secondary, 2);
    let words_height = title_galley.size().y + 2.0 + subtitle_galley.size().y;
    let height = theme::ROW_HEIGHT.max(words_height + 20.0);
    let (rect, response) = ui.allocate_exact_size(vec2(width, height), Sense::click());
    response.widget_info(|| {
        egui::WidgetInfo::labeled(
            egui::WidgetType::Button,
            ui.is_enabled(),
            format!("{title}, {subtitle}"),
        )
    });
    if ui.is_rect_visible(rect) {
        let hovered = response.hovered() || response.has_focus();
        let painter = ui.painter();
        painter.rect_filled(
            rect,
            theme::RADIUS,
            if hovered {
                palette.surface_hover
            } else {
                palette.surface
            },
        );
        let tile = Rect::from_min_size(
            pos2(rect.left() + TILE_LEFT, rect.center().y - TILE / 2.0),
            vec2(TILE, TILE),
        );
        painter.rect_filled(
            tile,
            4.0,
            if palette.dark {
                palette.surface_active
            } else {
                palette.surface_hover
            },
        );
        theme::paint_icon(ui, Icon::Disc, tile, 18.0, palette.accent);
        let chevron = Rect::from_center_size(
            pos2(rect.right() - 12.0 - CHEVRON / 2.0, rect.center().y),
            vec2(CHEVRON, CHEVRON),
        );
        theme::paint_icon(
            ui,
            Icon::ChevronRight,
            chevron,
            CHEVRON,
            if hovered {
                palette.text
            } else {
                palette.secondary
            },
        );
        let top = rect.center().y - words_height / 2.0;
        let title_rect = Rect::from_min_max(
            pos2(rect.left() + WORDS_LEFT, top),
            pos2(rect.left() + WORDS_LEFT + words_width, top),
        );
        let subtitle_rect = title_rect.translate(vec2(0.0, title_galley.size().y + 2.0));
        let subtitle_pos = crate::bidi::galley_pos(subtitle_rect, &subtitle_galley);
        painter.galley(
            crate::bidi::galley_pos(title_rect, &title_galley),
            title_galley,
            palette.text,
        );
        if hovered {
            painter.galley_with_override_text_color(subtitle_pos, subtitle_galley, palette.text);
        } else {
            painter.galley(subtitle_pos, subtitle_galley, palette.secondary);
        }
    }
    theme::focus_ring(ui, &response);
    if response
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .clicked()
    {
        app.actions.push(Action::SampleConnections(query));
    }
}
