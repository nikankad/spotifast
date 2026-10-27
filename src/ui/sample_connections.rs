//! A small, on-demand view of a recording's sample relationships.

use egui::{CornerRadius, Frame, Margin, Stroke};

use crate::app::App;
use crate::i18n::gettext;
use crate::model::{Action, Loadable};
use crate::sample_connections::images::Target;
use crate::sample_connections::{Found, Query, Recording, View};
use crate::theme;

pub(super) fn show(app: &mut App, ctx: &egui::Context, view: &View) {
    let palette = app.palette;
    let locale = app.locale;
    let frame = Frame::new()
        .fill(palette.overlay)
        .stroke(Stroke::new(1.0, palette.outline))
        .corner_radius(CornerRadius::same(theme::RADIUS + 4))
        .inner_margin(Margin::same(24));
    let response = egui::Modal::new(egui::Id::new("sample-connections"))
        .frame(frame)
        .backdrop_color(egui::Color32::from_black_alpha(if palette.dark { 150 } else { 80 }))
        .show(ctx, |ui| {
            ui.set_width((ctx.content_rect().width() - 80.0).clamp(240.0, 740.0));
            ui.horizontal(|ui| {
                if theme::icon_button(ui, theme::Icon::ChevronLeft, 20.0, palette.secondary, palette.text, &gettext(locale, "Back")).clicked() {
                    app.actions.push(Action::CloseDialog);
                }
                theme::text(ui, gettext(locale, "Sample connections"), theme::bold(22.0), palette.text);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if theme::icon_button(ui, theme::Icon::X, 20.0, palette.secondary, palette.text, &gettext(locale, "Close")).clicked() {
                        app.actions.push(Action::CloseDialog);
                    }
                });
            });
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                let cover = match &view.query { Query::Song { cover, .. } => cover.as_deref(), _ => None };
                if let Some(cover) = cover {
                    super::widgets::cover(ui, &palette, Some(cover), 64.0, 6.0, theme::Icon::Music);
                } else if let Loadable::Loaded(Found::Connections(c)) = &view.result {
                    picture(app, ui, view, Target::Recording(c.recording.id.clone()), 64.0, false);
                } else {
                    super::widgets::cover(ui, &palette, None, 64.0, 6.0, theme::Icon::Music);
                }
                ui.vertical(|ui| {
                    ui.add_space(6.0);
                    label(ui, view.query.title(), 20.0, palette.text);
                    label(ui, view.query.artist(), 14.0, palette.secondary);
                });
            });
            ui.add_space(16.0);
            egui::ScrollArea::vertical()
                .id_salt("sample-relationships")
                .auto_shrink([false, false])
                .min_scrolled_height((ctx.content_rect().height() - 290.0).max(120.0))
                .max_height((ctx.content_rect().height() - 290.0).max(120.0))
                .show(ui, |ui| match &view.result {
                    Loadable::NotLoaded | Loadable::Loading => {
                        ui.horizontal(|ui| {
                            ui.spinner();
                            ui.label(gettext(locale, "Looking up sample information…"));
                        });
                    }
                    Loadable::Failed(_) => {
                        label(ui, &gettext(locale, "Couldn't load sample information. MusicBrainz may be busy. Please try again later."), 14.0, palette.secondary);
                        ui.add_space(8.0);
                        if theme::pill_button(ui, &palette, &gettext(locale, "Try again"), false).clicked() {
                            app.actions.push(Action::SampleConnections(view.query.clone()));
                        }
                    }
                    Loadable::Loaded(Found::Candidates(candidates)) => {
                        if candidates.is_empty() {
                            label(ui, &gettext(locale, "No matching recording found on MusicBrainz."), 14.0, palette.secondary);
                        } else {
                            label(ui, &gettext(locale, "Choose the recording that matches your song. Edits and remixes can have different samples."), 14.0, palette.secondary);
                            ui.add_space(12.0);
                            for recording in candidates {
                                row(app, ui, view, recording, true);
                            }
                        }
                    }
                    Loadable::Loaded(Found::Connections(connections)) => {
                        if !connections.recording.disambiguation.is_empty() {
                            label(ui, &connections.recording.disambiguation, 13.0, palette.secondary);
                            ui.add_space(8.0);
                        }
                        contributors(app, ui, view, &connections.contributors);
                        for (heading, recordings) in [
                            (gettext(locale, "Samples from"), &connections.samples_from),
                            (gettext(locale, "Sampled by"), &connections.sampled_by),
                        ] {
                            theme::text(ui, format!("{heading} ({})", recordings.len()), theme::bold(16.0), palette.text);
                            ui.add_space(8.0);
                            if recordings.is_empty() {
                                label(ui, &gettext(locale, "No sample information found."), 14.0, palette.secondary);
                                ui.add_space(12.0);
                            }
                            for recording in recordings {
                                row(app, ui, view, recording, false);
                            }
                            ui.add_space(12.0);
                        }
                        ui.hyperlink_to(gettext(locale, "View recording on MusicBrainz"), connections.recording.source_url());
                    }
                });
            ui.add_space(12.0);
            ui.separator();
            label(ui, &gettext(locale, "Community metadata from MusicBrainz. Coverage varies by recording."), 12.0, palette.secondary);

        });
    if response.should_close() {
        app.actions.push(Action::CloseDialog);
    }
}

fn row(app: &mut App, ui: &mut egui::Ui, view: &View, recording: &Recording, candidate: bool) {
    let palette = app.palette;
    let locale = app.locale;
    ui.push_id(&recording.id, |ui| {
        ui.horizontal(|ui| {
            picture(
                app,
                ui,
                view,
                Target::Recording(recording.id.clone()),
                48.0,
                false,
            );
            ui.vertical(|ui| {
                label(ui, &recording.title, 16.0, palette.text);
                label(ui, &recording.artist, 13.0, palette.secondary);
            });
        });
        let mut detail = recording.disambiguation.clone();
        if let Some(length) = recording.duration_ms {
            if !detail.is_empty() {
                detail.push_str(" · ");
            }
            detail.push_str(&crate::util::format_duration_ms(length));
        }
        if !detail.is_empty() {
            label(ui, &detail, 12.0, palette.secondary);
        }
        ui.horizontal_wrapped(|ui| {
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
                && theme::pill_button(ui, &palette, &gettext(locale, "Find on Spotify"), false)
                    .clicked()
            {
                app.actions.push(Action::CloseDialog);
                app.actions.push(Action::Search(recording.spotify_query()));
            }
        });
        ui.add_space(10.0);
        ui.separator();
        ui.add_space(6.0);
    });
}

fn contributors(
    app: &mut App,
    ui: &mut egui::Ui,
    view: &View,
    credits: &[crate::sample_connections::Contributor],
) {
    let palette = app.palette;
    let locale = app.locale;
    theme::text(
        ui,
        gettext(locale, "Contributors"),
        theme::bold(18.0),
        palette.text,
    );
    ui.add_space(16.0);
    if credits.is_empty() {
        label(
            ui,
            &gettext(locale, "No contributor information found."),
            14.0,
            palette.secondary,
        );
    }
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = egui::vec2(16.0, 18.0);
        for credit in credits {
            ui.allocate_ui_with_layout(
                egui::vec2(144.0, 150.0),
                egui::Layout::top_down(egui::Align::Center),
                |ui| {
                    let response =
                        picture(app, ui, view, Target::Artist(credit.id.clone()), 72.0, true);
                    response.widget_info(|| {
                        egui::WidgetInfo::labeled(egui::WidgetType::Link, true, &credit.name)
                    });
                    if response.on_hover_text(&credit.name).clicked() {
                        ui.ctx()
                            .open_url(egui::OpenUrl::new_tab(credit.source_url()));
                    }
                    label(ui, &credit.name, 15.0, palette.text);
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
                    label(ui, &roles, 12.0, palette.secondary);
                },
            );
        }
    });
    ui.add_space(24.0);
}

/// Resolving image URLs is an action, applied after drawing like any other request.
/// Offscreen rows do not start network work. Each target is requested once per view.
fn picture(
    app: &mut App,
    ui: &mut egui::Ui,
    view: &View,
    target: Target,
    size: f32,
    round: bool,
) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(
        egui::Vec2::splat(size),
        if round {
            egui::Sense::click()
        } else {
            egui::Sense::hover()
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
        if round {
            theme::Icon::User
        } else {
            theme::Icon::Music
        },
        Some(app.backend.art()),
    );
    if let Some(picture) = picture {
        response.on_hover_ui(|ui| {
            let text = crate::bidi::layout(
                ui.painter(),
                &picture.credit,
                theme::regular(13.0),
                app.palette.text,
                ui.available_width(),
                usize::MAX,
                None,
            );
            ui.hyperlink_to(text, &picture.source);
        })
    } else {
        response
    }
}

fn label(ui: &mut egui::Ui, text: &str, size: f32, color: egui::Color32) {
    let galley = crate::bidi::layout(
        ui.painter(),
        text,
        theme::regular(size),
        color,
        ui.available_width(),
        usize::MAX,
        None,
    );
    ui.add(egui::Label::new(galley));
}

/// No provider request happens until this card is clicked.
pub(super) fn card(app: &mut App, ui: &mut egui::Ui, track: &crate::api::models::Track) {
    let Some(query) = Query::from_track(track) else {
        return;
    };
    let palette = app.palette;
    let title = gettext(app.locale, "Sample connections");
    let subtitle = gettext(
        app.locale,
        "Explore samples and the people behind this song",
    );
    let text = format!("{title}  ›\n{subtitle}");
    let galley = crate::bidi::layout(
        ui.painter(),
        &text,
        theme::semibold(14.0),
        palette.text,
        ui.available_width() - 24.0,
        usize::MAX,
        None,
    );
    let response = ui.add_sized(
        egui::vec2(ui.available_width(), 76.0),
        egui::Button::new(galley)
            .fill(palette.surface)
            .corner_radius(12),
    );
    if response.clicked() {
        app.actions.push(Action::SampleConnections(query));
    }
}
