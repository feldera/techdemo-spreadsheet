use std::ops::ControlFlow;
use std::sync::atomic::Ordering;
use std::sync::Arc;

use egui::color_picker::Alpha;
use egui::mutex::RwLock;
use egui::special_emojis::GITHUB;
use egui::{Color32, Key, OpenUrl, Pos2, Rect, RichText, ScrollArea, Sense, Vec2, Window};
use egui_extras::{Column, TableBuilder};
use ewebsock::{WsEvent, WsMessage, WsReceiver};
use log::{error, trace};
use serde_json::Deserializer;

use crate::cell_cache::{Cell, CellCache, Loader};
use crate::http::streaming_request;
use crate::reference::ReferenceWindow;

#[derive(serde::Deserialize, Default, Debug, Clone)]
pub struct Stats {
    pub filled_total: u64,
    pub filled_this_hour: u64,
    pub filled_today: u64,
    pub filled_this_week: u64,
    pub currently_active_users: u64,
}

pub struct SpreadsheetApp {
    focused_row: usize,
    focused_col: usize,
    bg_color_picked: Color32,
    last_key_time: f64,
    num_cols: usize,
    num_rows: usize,
    loader: Arc<Loader>,
    ws_receiver: WsReceiver,
    stats: Arc<RwLock<Stats>>,
    cell_cache: CellCache,
    editing_cell: Option<u64>,
    reference_open: bool,
}

impl SpreadsheetApp {
    const DEFAULT_COLS: usize = 26;
    const DEFAULT_ROWS: usize = 40_000_000; // 26*40_000_000 = 1_040_000_000 cells
    const DEFAULT_ROW_HEIGHT: f32 = 18.0;

    /// Called once before the first frame.
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        // This is also where you can customize the look and feel of egui using
        // `cc.egui_ctx.set_visuals` and `cc.egui_ctx.set_fonts`.
        egui_extras::install_image_loaders(&cc.egui_ctx);
        let server = CellCache::API_HOST.unwrap_or("http://localhost:3000");

        // Refresh stats
        let stats = Arc::new(RwLock::new(Stats::default()));
        {
            let egui_ctx = cc.egui_ctx.clone();
            let stats = stats.clone();
            let handle_chunk = Arc::new(move |current_chunk: String| {
                let stream = Deserializer::from_str(&current_chunk).into_iter::<Stats>();
                for maybe_value in stream {
                    match maybe_value {
                        Ok(value) => {
                            *stats.write() = value;
                        }
                        Err(err) => {
                            error!("an error occurred while reading stats: {err}");
                            return ControlFlow::Break(());
                        }
                    }
                }
                egui_ctx.request_repaint();
                ControlFlow::Continue(())
            });
            streaming_request(format!("{}/api/stats", server), handle_chunk);
        }

        // Change stream connection
        let (ws_sender, ws_receiver) = {
            let egui_ctx = cc.egui_ctx.clone();
            let wakeup = move || egui_ctx.request_repaint();
            let url = format!("{}/api/spreadsheet", server);
            ewebsock::connect_with_wakeup(&url, Default::default(), wakeup).unwrap()
        };
        let loader = Arc::new(Loader::new(ws_sender));

        SpreadsheetApp {
            focused_row: 0,
            focused_col: 0,
            bg_color_picked: Color32::TRANSPARENT,
            last_key_time: 0.0,
            num_cols: Self::DEFAULT_COLS,
            num_rows: Self::DEFAULT_ROWS,
            stats,
            loader: loader.clone(),
            ws_receiver,
            cell_cache: CellCache::new(loader, Self::DEFAULT_COLS, Self::DEFAULT_ROWS),
            editing_cell: None,
            reference_open: false,
        }
    }
}

impl eframe::App for SpreadsheetApp {
    /// Called each time the UI needs repainting, which may be many times per second.
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        while let Some(event) = self.ws_receiver.try_recv() {
            match event {
                WsEvent::Message(WsMessage::Text(update)) => {
                    let parsed = serde_json::from_str::<Cell>(&update);
                    match parsed {
                        Ok(cell) => {
                            self.cell_cache.set(cell.id, cell.into());
                        }
                        Err(e) => {
                            trace!("error parsing cell update: {:?} {:?}", update, e);
                        }
                    }
                }
                WsEvent::Opened => {
                    self.loader.is_open.store(true, Ordering::Relaxed);
                    self.loader.fetch(0..2600);
                }
                WsEvent::Closed => {
                    self.loader.is_open.store(false, Ordering::Relaxed);
                }
                _ => {
                    error!("unexpected event: {:?}", event);
                }
            }
        }

        egui::TopBottomPanel::top("top_panel").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    egui::widgets::global_theme_preference_buttons(ui);
                    if ui.button("📖 Read The Blog Post").clicked() {
                        ctx.output_mut(|o| {
                            o.open_url = Some(OpenUrl::new_tab(
                                "https://docs.feldera.com/use_cases/real_time_apps/part1",
                            ))
                        });
                    }
                    if ui.button("📺 Video Tutorial").clicked() {
                        ctx.output_mut(|o| {
                            o.open_url = Some(OpenUrl::new_tab(
                                "https://www.youtube.com/watch?v=ROa4duVqoOs",
                            ))
                        });
                    }
                    if ui.button(format!("{GITHUB} Fork me on Github")).clicked() {
                        ctx.output_mut(|o| {
                            o.open_url = Some(OpenUrl::new_tab(
                                "https://github.com/feldera/techdemo-spreadsheet",
                            ))
                        });
                    }
                    Window::new("Formula Reference")
                        .open(&mut self.reference_open)
                        .show(ctx, |ui| {
                            let mut rw = ReferenceWindow {};
                            rw.ui(ui);
                        });
                    if ui.button("？ Help").clicked() {
                        self.reference_open = true;
                    }
                });
            });
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading(RichText::new("Billion Cell Spreadsheet").strong());
            ui.add_space(20.0);

            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    let stats = self.stats.read().clone();

                    ui.with_layout(egui::Layout::left_to_right(egui::Align::TOP), |ui| {
                        // Active users section with icons
                        ui.vertical(|ui| {
                            ui.label(RichText::new("Currently Active Users:").strong());
                            let icon_size = Vec2::splat(10.0);
                            ui.horizontal(|ui| {
                                for _ in 0..stats.currently_active_users.min(10) {
                                    ui.painter().circle_filled(
                                        Pos2::new(
                                            ui.cursor().min.x + icon_size.x,
                                            ui.cursor().center().y,
                                        ),
                                        icon_size.x / 2.0,
                                        Color32::LIGHT_GREEN,
                                    );
                                    ui.add_space(12.0);
                                }
                                if stats.currently_active_users > 10 {
                                    ui.label(format!(
                                        "+{} more",
                                        stats.currently_active_users - 10
                                    ));
                                }
                            });
                        });

                        ui.separator();

                        // Meter for total filled cells
                        ui.vertical(|ui| {
                            ui.label(RichText::new("Cells With Content:").strong());
                            let max_cells = (SpreadsheetApp::DEFAULT_COLS as u64
                                * SpreadsheetApp::DEFAULT_ROWS as u64)
                                as f64;
                            let filled_ratio = (stats.filled_total as f64 / max_cells) as f32;
                            let filled_color = if filled_ratio < 0.5 {
                                Color32::from_rgb(100, 150, 250)
                            } else {
                                Color32::from_rgb(250, 100, 100)
                            };
                            ui.painter().rect_filled(
                                Rect::from_min_size(
                                    ui.cursor().min,
                                    Vec2::new(filled_ratio * 150.0, 15.0),
                                ),
                                4.0,
                                filled_color,
                            );
                            ui.label(format!("{}/{}", stats.filled_total, max_cells));
                            ui.label(format!("{}%", filled_ratio * 100.0));
                        });

                        ui.separator();

                        ui.horizontal(|ui| {
                            ui.label(RichText::new("Cells Edited This Hour: ").strong());
                            ui.label(format!("{}", stats.filled_this_hour));
                        });

                        ui.horizontal(|ui| {
                            ui.label(RichText::new("Cells Edited Today: ").strong());
                            ui.label(format!("{}", stats.filled_today));
                        });

                        ui.horizontal(|ui| {
                            ui.label(RichText::new("Cells Edited This Week: ").strong());
                            ui.label(format!("{}", stats.filled_this_week));
                        });
                    });

                    ui.add_space(50.0);

                    ui.with_layout(
                        egui::Layout::right_to_left(egui::Align::TOP).with_main_wrap(true),
                        |ui| {
                            ui.vertical(|ui| {
                                ui.heading("Built with");

                                ui.horizontal(|ui| {
                                    ui.add(
                                        egui::Hyperlink::from_label_and_url(
                                            format!("{GITHUB} feldera"),
                                            "https://github.com/feldera/feldera",
                                        )
                                            .open_in_new_tab(true),
                                    );
                                    ui.add_space(10.0);
                                    ui.add(
                                        egui::Hyperlink::from_label_and_url(
                                            format!("{GITHUB} axum"),
                                            "https://github.com/tokio-rs/axum",
                                        )
                                            .open_in_new_tab(true),
                                    );
                                    ui.add_space(10.0);
                                    ui.add(
                                        egui::Hyperlink::from_label_and_url(
                                            format!("{GITHUB} egui"),
                                            "https://github.com/emilk/egui",
                                        )
                                            .open_in_new_tab(true),
                                    );
                                    ui.add_space(10.0);
                                    ui.add(
                                        egui::Hyperlink::from_label_and_url(
                                            format!("{GITHUB} XLFormula Engine"),
                                            "https://github.com/jiradaherbst/XLFormula-Engine",
                                        )
                                            .open_in_new_tab(true),
                                    );
                                    ui.add_space(10.0);
                                });
                            });
                        },
                    );
                });
            });

            ui.add_space(20.0);

            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    let original_spacing = {
                        let style = ui.style_mut();
                        let original_spacing = style.spacing.item_spacing;
                        style.spacing.item_spacing.x = 2.0;
                        original_spacing
                    };

                    ui.label("Set Background Color");
                    ui.colored_label(Color32::LIGHT_BLUE, RichText::new("[?]")).on_hover_text(
                        "By default colors are at 0 alpha (fully transparent).\nMove the bottom slider in the widget to decrease the transparency if yo want\nto set a color on a new transparent cell.",
                    );
                    let style = ui.style_mut();
                    style.spacing.item_spacing = original_spacing;
                });

                let id = self.focused_row as u64 * self.num_cols as u64 + self.focused_col as u64;
                let cell = self.cell_cache.get(id);
                let color_response = egui::widgets::color_picker::color_edit_button_srgba(
                    ui,
                    &mut self.bg_color_picked,
                    Alpha::BlendOrAdditive,
                );
                if color_response.changed() {
                    cell.set_background(self.bg_color_picked);
                }
            });

            ScrollArea::horizontal().show(ui, |ui| {
                TableBuilder::new(ui)
                    .striped(true)
                    .resizable(true)
                    .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
                    .column(Column::remainder())
                    .columns(Column::initial(100.0).at_least(25.0).resizable(true).clip(true), self.num_cols)
                    .header(Self::DEFAULT_ROW_HEIGHT + 3.0, |mut header| {
                        let col_idx_to_label = |idx: usize| {
                            if idx < 26 {
                                format!("{}", (b'A' + idx as u8) as char)
                            } else {
                                format!(
                                    "{}{}",
                                    (b'A' + (idx / 26 - 1) as u8) as char,
                                    (b'A' + (idx % 26) as u8) as char
                                )
                            }
                        };

                        header.col(|ui| {
                            ui.strong("");
                        });

                        for col_index in 0..self.num_cols {
                            header.col(|ui| {
                                ui.strong(col_idx_to_label(col_index));
                            });
                        }
                    })
                    .body(|body| {
                        body.rows(Self::DEFAULT_ROW_HEIGHT, self.num_rows, |mut row| {
                            let row_index = row.index();
                            row.col(|ui| {
                                ui.strong(row_index.to_string());
                            });

                            for col_index in 0..self.num_cols {
                                let id = row_index as u64 * self.num_cols as u64 + col_index as u64;
                                let cell = self.cell_cache.get(id);
                                row.col(|ui| {
                                    let has_focus = row_index == self.focused_row
                                        && col_index == self.focused_col;
                                    let rect = ui.available_rect_before_wrap();
                                    let resp = ui.interact(
                                        ui.available_rect_before_wrap(),
                                        ui.make_persistent_id(id),
                                        Sense::click(),
                                    );
                                    ui.painter().rect_filled(rect, 0.0, cell.background_color());
                                    let cell_response = cell.ui(ui);

                                    // Adjust cell focus based on the new coordinates
                                    if has_focus {
                                        ui.painter().rect_stroke(
                                            rect,
                                            0.0,
                                            egui::Stroke::new(1.0, Color32::LIGHT_BLUE),
                                        );
                                    }

                                    ui.input(|i| {
                                        const KEY_DELAY: f64 = 0.01;
                                        let now = i.time;
                                        i.events.iter().for_each(|i| {
                                            if let egui::Event::Key { key, pressed, .. } = i {
                                                if now - self.last_key_time > KEY_DELAY && *pressed
                                                {
                                                    match key {
                                                        Key::Escape => {
                                                            if self.editing_cell.is_some() {
                                                                cell.disable_edit(true);
                                                            }
                                                        }
                                                        Key::Enter => {
                                                            self.focused_row = (self.focused_row
                                                                + 1)
                                                                .min(self.num_rows - 1);
                                                            self.last_key_time = now;
                                                        }
                                                        Key::ArrowDown => {
                                                            if self.editing_cell.is_none() {
                                                                self.focused_row =
                                                                    (self.focused_row + 1)
                                                                        .min(self.num_rows - 1);
                                                                self.last_key_time = now;
                                                            }
                                                        }
                                                        Key::ArrowUp => {
                                                            if self.editing_cell.is_none() {
                                                                self.focused_row = self
                                                                    .focused_row
                                                                    .saturating_sub(1);
                                                                self.last_key_time = now;
                                                            }
                                                        }
                                                        Key::ArrowRight => {
                                                            if self.editing_cell.is_none() {
                                                                self.focused_col =
                                                                    (self.focused_col + 1)
                                                                        .min(self.num_cols - 1);
                                                                self.last_key_time = now;
                                                            }
                                                        }
                                                        Key::ArrowLeft => {
                                                            if self.editing_cell.is_none() {
                                                                self.focused_col = self
                                                                    .focused_col
                                                                    .saturating_sub(1);
                                                                self.last_key_time = now;
                                                            }
                                                        }
                                                        Key::PageDown => {
                                                            if self.editing_cell.is_none() {
                                                                self.focused_row =
                                                                    (self.focused_row + 10)
                                                                        .min(self.num_rows - 1);
                                                                self.last_key_time = now;
                                                            }
                                                        }
                                                        Key::PageUp => {
                                                            if self.editing_cell.is_none() {
                                                                self.focused_row = self
                                                                    .focused_row
                                                                    .saturating_sub(10);
                                                                self.last_key_time = now;
                                                            }
                                                        }
                                                        _ => {}
                                                    }
                                                }
                                            }
                                        });
                                    });

                                    // Set focus on the cell
                                    if resp.clicked()
                                        || (cell_response.clicked() && !cell_response.has_focus())
                                    {
                                        self.focused_row = row_index;
                                        self.focused_col = col_index;
                                        self.bg_color_picked = cell.background_color();
                                    }

                                    // Done with editing
                                    if self.editing_cell.is_some() && cell_response.lost_focus() {
                                        cell.disable_edit(false);
                                        cell.save();
                                        self.editing_cell = None;
                                    }

                                    // Edit the current cell
                                    if self.editing_cell.is_none()
                                        && (resp.double_clicked()
                                        || cell_response.double_clicked()
                                        || (resp.has_focus()
                                        && ui.input(|i| i.key_pressed(Key::Enter))))
                                    {
                                        cell_response.request_focus();
                                        cell.edit();
                                        self.editing_cell = Some(id);
                                    }
                                });
                            }
                        });
                    });
            });
        });
    }
}
