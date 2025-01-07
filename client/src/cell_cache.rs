use std::cell::RefCell;
use std::fmt::Display;
use std::num::NonZeroUsize;
use std::ops::Range;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};
use std::sync::Arc;
use std::time::Duration;

use egui::mutex::{Mutex, RwLock};
use egui::widgets::TextEdit;
use egui::{Color32, Label, Response, Sense, Ui};
use ehttp::Request;
use ewebsock::{WsMessage, WsSender};
use log::{debug, trace, warn};
use lru::LruCache;
use serde_json::json;

use crate::debouncer::Debouncer;

/// The cell as it comes from the backend.
#[derive(Debug, Clone, Eq, PartialEq, serde::Deserialize)]
pub(crate) struct Cell {
    pub(crate) id: u64,
    pub(crate) raw_value: String,
    pub(crate) computed_value: String,
    pub(crate) background: i32,
}

/// A request to update a cell.
#[derive(Debug, Clone, Eq, PartialEq, serde::Serialize)]
pub(crate) struct UpdateCellRequest {
    pub(crate) id: u64,
    pub(crate) raw_value: String,
    pub(crate) background: i32,
}

impl From<&CellContent> for UpdateCellRequest {
    fn from(cell: &CellContent) -> Self {
        Self {
            id: cell.id,
            raw_value: cell.write_buffer.read().clone(),
            background: cell.background.load(Ordering::Relaxed),
        }
    }
}

/// A Cell that we currently track as part of the spreadsheet.
pub(crate) struct CellContent {
    pub(crate) id: u64,
    pub(crate) content: RwLock<String>,
    pub(crate) write_buffer: RwLock<String>,
    pub(crate) old_write_buffer: Mutex<String>,
    pub(crate) background: AtomicI32,
    pub(crate) is_editing: AtomicBool,
    debounce_bg_change: Rc<Mutex<Debouncer>>,
}

/// We convert Cells from the backend into CellContent that we can edit.
impl From<Cell> for CellContent {
    fn from(cell: Cell) -> Self {
        Self {
            id: cell.id,
            content: RwLock::new(cell.computed_value),
            write_buffer: RwLock::new(cell.raw_value.clone()),
            old_write_buffer: Mutex::new(cell.raw_value),
            is_editing: AtomicBool::new(false),
            background: AtomicI32::new(cell.background),
            debounce_bg_change: Rc::new(Mutex::new(Debouncer::new())),
        }
    }
}

impl CellContent {
    /// A new empty cell.
    pub(crate) fn empty(id: u64) -> Self {
        Self {
            id,
            write_buffer: RwLock::new(String::new()),
            old_write_buffer: Mutex::new(String::new()),
            content: RwLock::new(String::new()),
            is_editing: AtomicBool::new(false),
            background: AtomicI32::new(i32::from_le_bytes(Color32::TRANSPARENT.to_array())),
            debounce_bg_change: Rc::new(Mutex::new(Debouncer::new())),
        }
    }

    pub(crate) fn background_color(&self) -> Color32 {
        let rgba_premultiplied = i32::to_le_bytes(self.background.load(Ordering::Relaxed));
        Color32::from_rgba_premultiplied(
            rgba_premultiplied[0],
            rgba_premultiplied[1],
            rgba_premultiplied[2],
            rgba_premultiplied[3],
        )
    }

    pub(crate) fn is_editing(&self) -> bool {
        self.is_editing.load(Ordering::SeqCst)
    }

    /// We set the cell into edit mode -- if the user clicks it.
    pub(crate) fn edit(&self) {
        let mut old_value = self.old_write_buffer.lock();
        old_value.clear();
        old_value.push_str(&self.write_buffer.read());
        self.is_editing.store(true, Ordering::SeqCst);
    }

    /// We disable editing mode -- if the user clicks elsewhere.
    pub(crate) fn disable_edit(&self, revert: bool) {
        if revert {
            let old_value = self.old_write_buffer.lock();
            let mut write_buffer = self.write_buffer.write();
            write_buffer.clear();
            write_buffer.push_str(&old_value);
        }
        self.is_editing.store(false, Ordering::SeqCst);
    }

    pub(crate) fn set_background(&self, color: Color32) {
        self.background
            .store(i32::from_le_bytes(color.to_array()), Ordering::Relaxed);
        let mut debouncer = self.debounce_bg_change.lock();
        let cell_update = self.into();
        debouncer.debounce(Duration::from_millis(350), move || {
            update_cell(
                format!(
                    "{}/api/spreadsheet",
                    CellCache::API_HOST.unwrap_or("http://localhost:3000")
                ),
                cell_update,
            );
        });
    }

    pub(crate) fn save(&self) {
        let mut old_value = self.old_write_buffer.lock();
        let new_value = self.write_buffer.read();
        if *old_value != *new_value {
            update_cell(
                format!(
                    "{}/api/spreadsheet",
                    CellCache::API_HOST.unwrap_or("http://localhost:3000")
                ),
                self.into(),
            );
            old_value.clear();
            old_value.push_str(&new_value);
        }
    }

    /// We render the cell in the UI/Table.
    pub fn ui(&self, ui: &mut Ui) -> Response {
        if self.is_editing() {
            let mut content = self.write_buffer.write();
            ui.add(TextEdit::singleline(&mut *content))
        } else {
            let content = self.content.read().to_string();
            ui.add(Label::new(&content).sense(Sense::click()))
        }
    }
}

/// Sends a PATCH request to the server to update a cell.
fn update_cell(url: String, data: UpdateCellRequest) {
    let request = Request::json(url, &data).unwrap();
    ehttp::fetch(request, move |response| {
        if let Ok(response) = response {
            if !response.ok {
                warn!("POST request failed: {:?}", response.text());
            }
        } else {
            debug!("No response received");
        }
    });
}

/// Helper to display CellContent.
impl Display for CellContent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.content.read())
    }
}

pub(crate) struct Loader {
    pub(crate) is_open: AtomicBool,
    ws_sender: Mutex<WsSender>,
}

impl Loader {
    pub(crate) fn new(ws_sender: WsSender) -> Self {
        Self {
            ws_sender: Mutex::new(ws_sender),
            is_open: AtomicBool::new(false),
        }
    }

    pub(crate) fn fetch(&self, range: Range<u64>) -> bool {
        if !self.is_open.load(Ordering::Relaxed) {
            return false;
        }

        let mut sender = self.ws_sender.lock();
        sender.send(WsMessage::Text(
            json!({"from": range.start, "to": range.end}).to_string(),
        ));
        true
    }
}

/// The CellCache stores a fixed number of cells in memory.
///
/// - It fetches cells from the backend as needed.
/// - It always contains the cells that the user is currently looking at (and some more
///   since it also prefetches cells around the current view to make scrolling smooth).
/// - It debounces fetching of new rows to avoid fetching too many cells at once.
pub(crate) struct CellCache {
    cells: Rc<Mutex<LruCache<u64, Rc<CellContent>>>>,
    fetcher: Arc<Loader>,
    debouncer: Rc<RefCell<Debouncer>>,
    current_range: Option<Range<u64>>,
    prefetch_before_after_id: u64,
    max_cells: usize
}

impl CellCache {
    pub(crate) const API_HOST: Option<&'static str> = option_env!("API_HOST");

    pub fn new(fetcher: Arc<Loader>, width: usize, height: usize) -> Self {
        let prefetch_before_after_id = 100 * width as u64;
        let lru_cache_size = NonZeroUsize::new(200 * width).unwrap();

        Self {
            fetcher,
            cells: Rc::new(Mutex::new(LruCache::new(lru_cache_size))),
            debouncer: Rc::new(RefCell::new(Debouncer::new())),
            current_range: None,
            prefetch_before_after_id,
            max_cells: width * height,
        }
    }

    pub fn set(&mut self, id: u64, c: CellContent) {
        let mut cells = self.cells.lock();
        cells.push(id, Rc::new(c));
    }

    pub fn get(&mut self, id: u64) -> Rc<CellContent> {
        let mut cells = self.cells.lock();

        if let Some(c) = cells.get(&id) {
            c.clone()
        } else {
            let c = Rc::new(CellContent::empty(id));
            cells.push(id, c.clone());

            if let Some(current_range) = &self.current_range {
                if current_range.contains(&id) {
                    // Already fetching this range...
                    return c;
                }
            }

            let start = id.saturating_sub(self.prefetch_before_after_id);
            let end = std::cmp::min(id.saturating_add(self.prefetch_before_after_id), self.max_cells as u64);
            let current_range = start..end;
            self.current_range = Some(current_range.clone());
            trace!("fetching range: {:?}", current_range);
            let fetcher = self.fetcher.clone();

            let debouncer_clone = self.debouncer.clone();
            debouncer_clone
                .borrow_mut()
                .debounce(Duration::from_millis(100), move || {
                    let mut max_retry = 10;
                    while !fetcher.fetch(current_range.clone()) && max_retry > 0 {
                        max_retry -= 1;
                    }
                });

            c
        }
    }
}
