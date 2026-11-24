use crate::model::{
    format_bytes, CleanTarget, DiskStats, FileCategory, FileNode, HeavyFile, SafetyLevel,
};
use crate::scanner::{
    delete_path, execute_clean_target, sort_clean_targets, spawn_scan, ScanMessage,
};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Rect;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::sync::Arc;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActiveTab {
    Explorer,
    Cleaner,
    TopFiles,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortMode {
    SizeDesc,
    NameAsc,
    CountDesc,
}

impl SortMode {
    pub fn next(self) -> Self {
        match self {
            SortMode::SizeDesc => SortMode::NameAsc,
            SortMode::NameAsc => SortMode::CountDesc,
            SortMode::CountDesc => SortMode::SizeDesc,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            SortMode::SizeDesc => "Tamaño ↓",
            SortMode::NameAsc => "Nombre A-Z",
            SortMode::CountDesc => "Archivos ↓",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HitAction {
    Tab(ActiveTab),
    Rescan,
    ToggleHelp,
    Quit,
    GoUp,
    Breadcrumb(PathBuf),
    CycleSort,
    ToggleFilter,
    ClearFilter,
    ExplorerRow(usize), // index into visible_entries
    ActionOpen,
    ActionOpenExternal,
    ActionTrash,
    ActionDeletePerm,
    CleanerRow(usize),
    CleanerSelectSafe,
    CleanerSelectNone,
    CleanerRun,
    TopCategory(Option<FileCategory>),
    TopRow(usize),
    TopJumpFolder,
    TopTrash,
    ModalConfirm,
    ModalCancel,
}

#[derive(Debug, Clone)]
pub struct HitRegion {
    pub rect: Rect,
    pub action: HitAction,
}

#[derive(Debug, Clone)]
pub enum ModalState {
    None,
    ConfirmDelete {
        path: PathBuf,
        name: String,
        size: u64,
        permanent: bool,
    },
    ConfirmClean {
        count: usize,
        total_bytes: u64,
    },
    Help,
}

#[derive(Debug, Clone)]
pub struct Toast {
    pub message: String,
    pub is_error: bool,
    pub expires_at: Instant,
}

#[derive(Debug, Clone)]
pub struct ExplorerEntry {
    pub is_parent_link: bool,
    pub name: String,
    pub path: PathBuf,
    pub size: u64,
    pub is_dir: bool,
    pub file_count: u64,
    pub modified: Option<std::time::SystemTime>,
    pub category: FileCategory,
}

pub struct App {
    pub root_path: PathBuf,
    pub current_dir: PathBuf,
    pub root_node: Option<FileNode>,
    pub disk_stats: Option<DiskStats>,

    pub active_tab: ActiveTab,
    pub sort_mode: SortMode,

    // Scanning state
    pub is_scanning: bool,
    pub scan_files: u64,
    pub scan_bytes: u64,
    pub scan_current_path: String,
    pub last_scan_elapsed: Option<Duration>,
    scan_rx: Option<Receiver<ScanMessage>>,
    scan_cancel: Arc<AtomicBool>,

    // Explorer view state
    pub explorer_selected: usize,
    pub explorer_scroll: usize,
    pub filter_text: String,
    pub filter_editing: bool,

    // Cleaner view state
    pub clean_targets: Vec<CleanTarget>,
    pub cleaner_selected: usize,
    pub cleaner_scroll: usize,
    pub total_freed_session: u64,

    // Top files view state
    pub top_files: Vec<HeavyFile>,
    pub top_category_filter: Option<FileCategory>,
    pub top_selected: usize,
    pub top_scroll: usize,

    // UI interaction
    pub modal: ModalState,
    pub toast: Option<Toast>,
    pub mouse_pos: Option<(u16, u16)>,
    pub hit_regions: Vec<HitRegion>,
    pub modal_hit_regions: Vec<HitRegion>,
    last_click: Option<(Instant, HitAction)>,
    pub tick_count: u64,
    pub should_quit: bool,
}

impl App {
    pub fn new(root_path: PathBuf) -> Self {
        let canonical = std::fs::canonicalize(&root_path).unwrap_or(root_path);
        let disk_stats = DiskStats::from_path(&canonical);
        let mut app = Self {
            root_path: canonical.clone(),
            current_dir: canonical,
            root_node: None,
            disk_stats,
            active_tab: ActiveTab::Explorer,
            sort_mode: SortMode::SizeDesc,
            is_scanning: false,
            scan_files: 0,
            scan_bytes: 0,
            scan_current_path: String::new(),
            last_scan_elapsed: None,
            scan_rx: None,
            scan_cancel: Arc::new(AtomicBool::new(false)),
            explorer_selected: 0,
            explorer_scroll: 0,
            filter_text: String::new(),
            filter_editing: false,
            clean_targets: Vec::new(),
            cleaner_selected: 0,
            cleaner_scroll: 0,
            total_freed_session: 0,
            top_files: Vec::new(),
            top_category_filter: None,
            top_selected: 0,
            top_scroll: 0,
            modal: ModalState::None,
            toast: None,
            mouse_pos: None,
            hit_regions: Vec::new(),
            modal_hit_regions: Vec::new(),
            last_click: None,
            tick_count: 0,
            should_quit: false,
        };
        app.start_scan();
        app
    }

    pub fn start_scan(&mut self) {
        self.scan_cancel.store(true, Ordering::Relaxed);
        let cancel = Arc::new(AtomicBool::new(false));
        self.scan_cancel = cancel.clone();

        let (tx, rx) = mpsc::channel();
        self.scan_rx = Some(rx);
        self.is_scanning = true;
        self.scan_files = 0;
        self.scan_bytes = 0;
        self.scan_current_path = self.root_path.display().to_string();
        self.disk_stats = DiskStats::from_path(&self.root_path);

        spawn_scan(self.root_path.clone(), tx, cancel);
    }

    pub fn on_tick(&mut self) {
        self.tick_count = self.tick_count.wrapping_add(1);

        if let Some(toast) = &self.toast {
            if Instant::now() >= toast.expires_at {
                self.toast = None;
            }
        }

        let mut messages = Vec::new();
        if let Some(rx) = &self.scan_rx {
            while let Ok(msg) = rx.try_recv() {
                messages.push(msg);
            }
        }

        for msg in messages {
            match msg {
                ScanMessage::Progress {
                    files_scanned,
                    bytes_found,
                    current_path,
                } => {
                    self.scan_files = files_scanned;
                    self.scan_bytes = bytes_found;
                    self.scan_current_path = current_path;
                }
                ScanMessage::CleanerReady(mut targets) => {
                    // Merge any dev bloat targets already added
                    let existing_dev: Vec<CleanTarget> = self
                        .clean_targets
                        .iter()
                        .filter(|t| t.description.starts_with("Carpeta regenerable:"))
                        .cloned()
                        .collect();
                    targets.extend(existing_dev);
                    sort_clean_targets(&mut targets);
                    self.clean_targets = targets;
                    self.clamp_selections();
                }
                ScanMessage::Finished {
                    root,
                    top_files,
                    dev_bloat_targets,
                    elapsed,
                } => {
                    let total = root.size;
                    let files = root.file_count;
                    self.root_node = Some(root);
                    self.top_files = top_files;
                    // Replace previous dev bloat entries and merge new ones
                    self.clean_targets
                        .retain(|t| !t.description.starts_with("Carpeta regenerable:"));
                    self.clean_targets.extend(dev_bloat_targets);
                    sort_clean_targets(&mut self.clean_targets);
                    self.is_scanning = false;
                    self.last_scan_elapsed = Some(elapsed);
                    self.disk_stats = DiskStats::from_path(&self.root_path);
                    if !self.current_dir.starts_with(&self.root_path) {
                        self.current_dir = self.root_path.clone();
                    }
                    self.clamp_selections();
                    self.show_toast(
                        format!(
                            "Escaneo listo: {} ({} archivos) en {:.1}s",
                            format_bytes(total),
                            crate::model::format_count(files),
                            elapsed.as_secs_f64()
                        ),
                        false,
                    );
                }
                ScanMessage::Error(err) => {
                    self.is_scanning = false;
                    self.show_toast(err, true);
                }
            }
        }
    }

    pub fn show_toast(&mut self, msg: impl Into<String>, is_error: bool) {
        self.toast = Some(Toast {
            message: msg.into(),
            is_error,
            expires_at: Instant::now() + Duration::from_secs(4),
        });
    }

    pub fn current_node(&self) -> Option<&FileNode> {
        let root = self.root_node.as_ref()?;
        root.find_by_path(&self.current_dir).or(Some(root))
    }

    pub fn explorer_entries(&self) -> Vec<ExplorerEntry> {
        let Some(node) = self.current_node() else {
            return Vec::new();
        };

        let mut list = Vec::new();
        if node.path != self.root_path {
            if let Some(parent) = node.path.parent() {
                list.push(ExplorerEntry {
                    is_parent_link: true,
                    name: ".. (Subir carpeta)".into(),
                    path: parent.to_path_buf(),
                    size: 0,
                    is_dir: true,
                    file_count: 0,
                    modified: None,
                    category: FileCategory::Other,
                });
            }
        }

        let needle = self.filter_text.trim().to_lowercase();
        let mut children: Vec<&FileNode> = node
            .children
            .iter()
            .filter(|c| needle.is_empty() || c.name.to_lowercase().contains(&needle))
            .collect();

        match self.sort_mode {
            SortMode::SizeDesc => {
                children.sort_by(|a, b| b.size.cmp(&a.size).then_with(|| a.name.cmp(&b.name)));
            }
            SortMode::NameAsc => {
                children.sort_by(|a, b| {
                    b.is_dir
                        .cmp(&a.is_dir)
                        .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
                });
            }
            SortMode::CountDesc => {
                children.sort_by(|a, b| {
                    b.file_count
                        .cmp(&a.file_count)
                        .then_with(|| b.size.cmp(&a.size))
                });
            }
        }

        for c in children {
            list.push(ExplorerEntry {
                is_parent_link: false,
                name: c.name.clone(),
                path: c.path.clone(),
                size: c.size,
                is_dir: c.is_dir,
                file_count: c.file_count,
                modified: c.modified,
                category: c.category,
            });
        }
        list
    }

    pub fn filtered_top_files(&self) -> Vec<HeavyFile> {
        let needle = self.filter_text.trim().to_lowercase();
        self.top_files
            .iter()
            .filter(|f| {
                self.top_category_filter
                    .map(|cat| f.category == cat)
                    .unwrap_or(true)
            })
            .filter(|f| {
                needle.is_empty()
                    || f.name.to_lowercase().contains(&needle)
                    || f.path.to_string_lossy().to_lowercase().contains(&needle)
            })
            .cloned()
            .collect()
    }

    pub fn selected_explorer_entry(&self) -> Option<ExplorerEntry> {
        let entries = self.explorer_entries();
        entries.get(self.explorer_selected).cloned()
    }

    pub fn selected_clean_bytes(&self) -> (usize, u64) {
        let mut count = 0;
        let mut bytes = 0;
        for t in &self.clean_targets {
            if t.selected && t.safety != SafetyLevel::SystemInfo {
                count += 1;
                bytes += t.size;
            }
        }
        (count, bytes)
    }

    pub fn total_reclaimable_bytes(&self) -> u64 {
        self.clean_targets
            .iter()
            .filter(|t| t.safety != SafetyLevel::SystemInfo)
            .map(|t| t.size)
            .sum()
    }

    pub fn clamp_selections(&mut self) {
        let exp_len = self.explorer_entries().len();
        if exp_len == 0 {
            self.explorer_selected = 0;
            self.explorer_scroll = 0;
        } else if self.explorer_selected >= exp_len {
            self.explorer_selected = exp_len - 1;
        }

        let clean_len = self.clean_targets.len();
        if clean_len == 0 {
            self.cleaner_selected = 0;
            self.cleaner_scroll = 0;
        } else if self.cleaner_selected >= clean_len {
            self.cleaner_selected = clean_len - 1;
        }

        let top_len = self.filtered_top_files().len();
        if top_len == 0 {
            self.top_selected = 0;
            self.top_scroll = 0;
        } else if self.top_selected >= top_len {
            self.top_selected = top_len - 1;
        }
    }

    pub fn is_hovered(&self, rect: Rect) -> bool {
        let Some((x, y)) = self.mouse_pos else {
            return false;
        };
        x >= rect.x
            && x < rect.x.saturating_add(rect.width)
            && y >= rect.y
            && y < rect.y.saturating_add(rect.height)
    }

    pub fn navigate_to(&mut self, path: PathBuf) {
        if path.starts_with(&self.root_path) {
            self.current_dir = path;
            self.explorer_selected = 0;
            self.explorer_scroll = 0;
            self.filter_text.clear();
            self.filter_editing = false;
        }
    }

    pub fn go_up(&mut self) {
        if self.current_dir != self.root_path {
            if let Some(parent) = self.current_dir.parent().map(|p| p.to_path_buf()) {
                let prev_dir = self.current_dir.clone();
                self.current_dir = parent;
                self.filter_text.clear();
                self.filter_editing = false;
                let entries = self.explorer_entries();
                if let Some(idx) = entries.iter().position(|e| e.path == prev_dir) {
                    self.explorer_selected = idx;
                } else {
                    self.explorer_selected = 0;
                }
                self.explorer_scroll = 0;
            }
        }
    }

    pub fn open_selected_explorer(&mut self) {
        let Some(entry) = self.selected_explorer_entry() else {
            return;
        };
        if entry.is_parent_link {
            self.go_up();
        } else if entry.is_dir {
            self.navigate_to(entry.path);
        } else {
            self.open_external(&entry.path);
        }
    }

    pub fn open_external(&mut self, path: &Path) {
        #[cfg(target_os = "macos")]
        let (cmd_name, mut cmd, display_target) = {
            let mut c = Command::new("open");
            if path.is_dir() {
                c.arg(path);
            } else {
                c.arg("-R").arg(path);
            }
            ("open", c, path)
        };

        #[cfg(not(target_os = "macos"))]
        let (cmd_name, mut cmd, display_target) = {
            let target = if path.is_dir() {
                path
            } else {
                path.parent().unwrap_or(path)
            };
            let mut c = Command::new("xdg-open");
            c.arg(target);
            ("xdg-open", c, target)
        };

        match cmd
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
        {
            Ok(_) => self.show_toast(
                format!("Abriendo en gestor de archivos: {}", display_target.display()),
                false,
            ),
            Err(e) => self.show_toast(format!("No se pudo abrir {}: {}", cmd_name, e), true),
        }
    }

    pub fn request_delete_explorer(&mut self, permanent: bool) {
        let Some(entry) = self.selected_explorer_entry() else {
            return;
        };
        if entry.is_parent_link {
            return;
        }
        self.modal = ModalState::ConfirmDelete {
            path: entry.path,
            name: entry.name,
            size: entry.size,
            permanent,
        };
    }

    pub fn request_delete_top(&mut self, permanent: bool) {
        let files = self.filtered_top_files();
        let Some(f) = files.get(self.top_selected).cloned() else {
            return;
        };
        self.modal = ModalState::ConfirmDelete {
            path: f.path.clone(),
            name: f.name.clone(),
            size: f.size,
            permanent,
        };
    }

    pub fn request_clean_selected(&mut self) {
        let (count, total_bytes) = self.selected_clean_bytes();
        if count == 0 {
            self.show_toast("Selecciona al menos un elemento para limpiar.", true);
            return;
        }
        self.modal = ModalState::ConfirmClean { count, total_bytes };
    }

    pub fn confirm_modal(&mut self) {
        let modal = std::mem::replace(&mut self.modal, ModalState::None);
        match modal {
            ModalState::ConfirmDelete {
                path,
                name,
                size,
                permanent,
            } => match delete_path(&path, permanent) {
                Ok(()) => {
                    if let Some(root) = &mut self.root_node {
                        root.remove_by_path(&path);
                    }
                    self.top_files.retain(|f| f.path != path && !f.path.starts_with(&path));
                    self.total_freed_session = self.total_freed_session.saturating_add(size);
                    self.disk_stats = DiskStats::from_path(&self.root_path);
                    self.clamp_selections();
                    let verb = if permanent {
                        "Eliminado definitivamente"
                    } else {
                        "Enviado a la papelera"
                    };
                    self.show_toast(format!("{}: {} ({})", verb, name, format_bytes(size)), false);
                }
                Err(err) => {
                    self.show_toast(err, true);
                }
            },
            ModalState::ConfirmClean { .. } => {
                let mut total_freed = 0u64;
                let mut cleaned_count = 0usize;
                let mut errors = 0usize;
                let mut cleaned_paths = Vec::new();

                for target in &mut self.clean_targets {
                    if target.selected && target.safety != SafetyLevel::SystemInfo {
                        match execute_clean_target(target) {
                            Ok(freed) => {
                                total_freed = total_freed.saturating_add(freed);
                                target.size = target.size.saturating_sub(freed);
                                if target.size == 0 {
                                    target.selected = false;
                                }
                                cleaned_count += 1;
                                if !target.delete_contents_only {
                                    cleaned_paths.extend(target.paths.clone());
                                }
                            }
                            Err(_) => {
                                errors += 1;
                            }
                        }
                    }
                }

                if let Some(root) = &mut self.root_node {
                    for p in &cleaned_paths {
                        root.remove_by_path(p);
                    }
                }
                self.clean_targets.retain(|t| t.size > 0 || t.safety == SafetyLevel::SystemInfo);
                self.total_freed_session = self.total_freed_session.saturating_add(total_freed);
                self.disk_stats = DiskStats::from_path(&self.root_path);
                self.clamp_selections();

                if errors > 0 {
                    self.show_toast(
                        format!(
                            "Liberados {} ({} categorías, {} con avisos de permisos)",
                            format_bytes(total_freed),
                            cleaned_count,
                            errors
                        ),
                        false,
                    );
                } else {
                    self.show_toast(
                        format!(
                            "¡Limpieza completada! Se liberaron {} en {} categorías.",
                            format_bytes(total_freed),
                            cleaned_count
                        ),
                        false,
                    );
                }
            }
            ModalState::Help | ModalState::None => {}
        }
    }

    pub fn handle_key(&mut self, key: KeyEvent) {
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            self.should_quit = true;
            return;
        }

        // Modal keyboard handling
        match &self.modal {
            ModalState::ConfirmDelete { .. } | ModalState::ConfirmClean { .. } => {
                match key.code {
                    KeyCode::Enter | KeyCode::Char('s') | KeyCode::Char('S') | KeyCode::Char('y') => {
                        self.confirm_modal();
                    }
                    KeyCode::Esc | KeyCode::Char('n') | KeyCode::Char('N') => {
                        self.modal = ModalState::None;
                    }
                    _ => {}
                }
                return;
            }
            ModalState::Help => {
                match key.code {
                    KeyCode::Esc | KeyCode::Enter | KeyCode::Char('?') | KeyCode::Char('q') => {
                        self.modal = ModalState::None;
                    }
                    _ => {}
                }
                return;
            }
            ModalState::None => {}
        }

        // Filter input mode
        if self.filter_editing {
            match key.code {
                KeyCode::Esc => {
                    self.filter_text.clear();
                    self.filter_editing = false;
                    self.clamp_selections();
                }
                KeyCode::Enter => {
                    self.filter_editing = false;
                }
                KeyCode::Backspace => {
                    self.filter_text.pop();
                    self.clamp_selections();
                }
                KeyCode::Char(c) => {
                    self.filter_text.push(c);
                    self.explorer_selected = 0;
                    self.top_selected = 0;
                    self.clamp_selections();
                }
                _ => {}
            }
            return;
        }

        // Global shortcuts
        match key.code {
            KeyCode::Char('q') => {
                self.should_quit = true;
                return;
            }
            KeyCode::Char('?') => {
                self.modal = ModalState::Help;
                return;
            }
            KeyCode::Char('1') => {
                self.active_tab = ActiveTab::Explorer;
                return;
            }
            KeyCode::Char('2') => {
                self.active_tab = ActiveTab::Cleaner;
                return;
            }
            KeyCode::Char('3') => {
                self.active_tab = ActiveTab::TopFiles;
                return;
            }
            KeyCode::Tab => {
                self.active_tab = match self.active_tab {
                    ActiveTab::Explorer => ActiveTab::Cleaner,
                    ActiveTab::Cleaner => ActiveTab::TopFiles,
                    ActiveTab::TopFiles => ActiveTab::Explorer,
                };
                return;
            }
            KeyCode::Char('r') => {
                self.start_scan();
                self.show_toast("Re-escaneando disco...", false);
                return;
            }
            KeyCode::Char('/') => {
                if self.active_tab != ActiveTab::Cleaner {
                    self.filter_editing = true;
                }
                return;
            }
            KeyCode::Esc => {
                if !self.filter_text.is_empty() {
                    self.filter_text.clear();
                    self.clamp_selections();
                }
                return;
            }
            _ => {}
        }

        // Tab-specific navigation
        match self.active_tab {
            ActiveTab::Explorer => match key.code {
                KeyCode::Down | KeyCode::Char('j') => self.move_explorer(1),
                KeyCode::Up | KeyCode::Char('k') => self.move_explorer(-1),
                KeyCode::PageDown => self.move_explorer(10),
                KeyCode::PageUp => self.move_explorer(-10),
                KeyCode::Home => self.explorer_selected = 0,
                KeyCode::End => {
                    let len = self.explorer_entries().len();
                    if len > 0 {
                        self.explorer_selected = len - 1;
                    }
                }
                KeyCode::Enter | KeyCode::Right | KeyCode::Char('l') => {
                    self.open_selected_explorer()
                }
                KeyCode::Backspace | KeyCode::Left | KeyCode::Char('h') => self.go_up(),
                KeyCode::Char('s') => {
                    self.sort_mode = self.sort_mode.next();
                    self.show_toast(format!("Ordenado por: {}", self.sort_mode.label()), false);
                }
                KeyCode::Char('d') | KeyCode::Delete => self.request_delete_explorer(false),
                KeyCode::Char('D') => self.request_delete_explorer(true),
                KeyCode::Char('o') => {
                    if let Some(entry) = self.selected_explorer_entry() {
                        if !entry.is_parent_link {
                            self.open_external(&entry.path);
                        }
                    }
                }
                _ => {}
            },
            ActiveTab::Cleaner => match key.code {
                KeyCode::Down | KeyCode::Char('j') => self.move_cleaner(1),
                KeyCode::Up | KeyCode::Char('k') => self.move_cleaner(-1),
                KeyCode::Char(' ') | KeyCode::Enter => self.toggle_clean_item(self.cleaner_selected),
                KeyCode::Char('a') => self.select_safe_clean_items(),
                KeyCode::Char('n') => self.clear_clean_selection(),
                KeyCode::Char('c') => self.request_clean_selected(),
                _ => {}
            },
            ActiveTab::TopFiles => match key.code {
                KeyCode::Down | KeyCode::Char('j') => self.move_top(1),
                KeyCode::Up | KeyCode::Char('k') => self.move_top(-1),
                KeyCode::PageDown => self.move_top(10),
                KeyCode::PageUp => self.move_top(-10),
                KeyCode::Enter => self.jump_to_selected_top_folder(),
                KeyCode::Char('d') | KeyCode::Delete => self.request_delete_top(false),
                KeyCode::Char('D') => self.request_delete_top(true),
                _ => {}
            },
        }
    }

    pub fn handle_mouse(&mut self, event: MouseEvent) {
        self.mouse_pos = Some((event.column, event.row));

        match event.kind {
            MouseEventKind::Moved => {}
            MouseEventKind::ScrollDown => {
                if matches!(self.modal, ModalState::None) {
                    match self.active_tab {
                        ActiveTab::Explorer => self.move_explorer(3),
                        ActiveTab::Cleaner => self.move_cleaner(2),
                        ActiveTab::TopFiles => self.move_top(3),
                    }
                }
            }
            MouseEventKind::ScrollUp => {
                if matches!(self.modal, ModalState::None) {
                    match self.active_tab {
                        ActiveTab::Explorer => self.move_explorer(-3),
                        ActiveTab::Cleaner => self.move_cleaner(-2),
                        ActiveTab::TopFiles => self.move_top(-3),
                    }
                }
            }
            MouseEventKind::Down(MouseButton::Left) => {
                let (col, row) = (event.column, event.row);

                // If modal is open, only modal hit regions are active
                if !matches!(self.modal, ModalState::None) {
                    if let Some(action) = find_hit(&self.modal_hit_regions, col, row) {
                        self.trigger_action(action, false);
                    } else if matches!(self.modal, ModalState::Help) {
                        self.modal = ModalState::None;
                    }
                    return;
                }

                if let Some(action) = find_hit(&self.hit_regions, col, row) {
                    let is_double = self
                        .last_click
                        .as_ref()
                        .map(|(t, prev)| {
                            t.elapsed() < Duration::from_millis(420) && prev == &action
                        })
                        .unwrap_or(false);
                    self.last_click = Some((Instant::now(), action.clone()));
                    self.trigger_action(action, is_double);
                } else {
                    self.filter_editing = false;
                }
            }
            MouseEventKind::Down(MouseButton::Right) => {
                if matches!(self.modal, ModalState::None) && self.active_tab == ActiveTab::Explorer
                {
                    self.go_up();
                }
            }
            _ => {}
        }
    }

    fn trigger_action(&mut self, action: HitAction, is_double: bool) {
        match action {
            HitAction::Tab(tab) => {
                self.active_tab = tab;
                self.filter_editing = false;
            }
            HitAction::Rescan => {
                self.start_scan();
                self.show_toast("Re-escaneando disco...", false);
            }
            HitAction::ToggleHelp => {
                self.modal = if matches!(self.modal, ModalState::Help) {
                    ModalState::None
                } else {
                    ModalState::Help
                };
            }
            HitAction::Quit => {
                self.should_quit = true;
            }
            HitAction::GoUp => self.go_up(),
            HitAction::Breadcrumb(path) => self.navigate_to(path),
            HitAction::CycleSort => {
                self.sort_mode = self.sort_mode.next();
                self.show_toast(format!("Ordenado por: {}", self.sort_mode.label()), false);
            }
            HitAction::ToggleFilter => {
                self.filter_editing = !self.filter_editing;
            }
            HitAction::ClearFilter => {
                self.filter_text.clear();
                self.filter_editing = false;
                self.clamp_selections();
            }
            HitAction::ExplorerRow(idx) => {
                self.filter_editing = false;
                let entries = self.explorer_entries();
                if idx < entries.len() {
                    self.explorer_selected = idx;
                    if is_double || entries[idx].is_parent_link {
                        self.open_selected_explorer();
                    }
                }
            }
            HitAction::ActionOpen => self.open_selected_explorer(),
            HitAction::ActionOpenExternal => {
                if let Some(entry) = self.selected_explorer_entry() {
                    if !entry.is_parent_link {
                        self.open_external(&entry.path);
                    }
                } else {
                    let cur = self.current_dir.clone();
                    self.open_external(&cur);
                }
            }
            HitAction::ActionTrash => self.request_delete_explorer(false),
            HitAction::ActionDeletePerm => self.request_delete_explorer(true),
            HitAction::CleanerRow(idx) => {
                if idx < self.clean_targets.len() {
                    self.cleaner_selected = idx;
                    self.toggle_clean_item(idx);
                }
            }
            HitAction::CleanerSelectSafe => self.select_safe_clean_items(),
            HitAction::CleanerSelectNone => self.clear_clean_selection(),
            HitAction::CleanerRun => self.request_clean_selected(),
            HitAction::TopCategory(cat) => {
                self.top_category_filter = cat;
                self.top_selected = 0;
                self.top_scroll = 0;
            }
            HitAction::TopRow(idx) => {
                let len = self.filtered_top_files().len();
                if idx < len {
                    self.top_selected = idx;
                    if is_double {
                        self.jump_to_selected_top_folder();
                    }
                }
            }
            HitAction::TopJumpFolder => self.jump_to_selected_top_folder(),
            HitAction::TopTrash => self.request_delete_top(false),
            HitAction::ModalConfirm => self.confirm_modal(),
            HitAction::ModalCancel => self.modal = ModalState::None,
        }
    }

    fn move_explorer(&mut self, delta: isize) {
        let len = self.explorer_entries().len();
        if len == 0 {
            return;
        }
        self.explorer_selected =
            (self.explorer_selected as isize + delta).clamp(0, (len - 1) as isize) as usize;
    }

    fn move_cleaner(&mut self, delta: isize) {
        let len = self.clean_targets.len();
        if len == 0 {
            return;
        }
        self.cleaner_selected =
            (self.cleaner_selected as isize + delta).clamp(0, (len - 1) as isize) as usize;
    }

    fn move_top(&mut self, delta: isize) {
        let len = self.filtered_top_files().len();
        if len == 0 {
            return;
        }
        self.top_selected =
            (self.top_selected as isize + delta).clamp(0, (len - 1) as isize) as usize;
    }

    fn toggle_clean_item(&mut self, idx: usize) {
        let Some(target) = self.clean_targets.get_mut(idx) else {
            return;
        };
        if target.safety == SafetyLevel::SystemInfo {
            let hint = target
                .command_hint
                .clone()
                .unwrap_or_else(|| "Requiere permisos sudo en terminal".into());
            self.show_toast(format!("Comando sugerido: {}", hint), false);
            return;
        }
        target.selected = !target.selected;
    }

    fn select_safe_clean_items(&mut self) {
        for t in &mut self.clean_targets {
            t.selected = t.safety == SafetyLevel::Safe;
        }
    }

    fn clear_clean_selection(&mut self) {
        for t in &mut self.clean_targets {
            t.selected = false;
        }
    }

    fn jump_to_selected_top_folder(&mut self) {
        let files = self.filtered_top_files();
        let Some(file) = files.get(self.top_selected).cloned() else {
            return;
        };
        let parent = file.parent.clone();
        let target_path = file.path.clone();
        self.active_tab = ActiveTab::Explorer;
        self.navigate_to(parent);
        let entries = self.explorer_entries();
        if let Some(idx) = entries.iter().position(|e| e.path == target_path) {
            self.explorer_selected = idx;
        }
    }
}

fn find_hit(regions: &[HitRegion], col: u16, row: u16) -> Option<HitAction> {
    // Search in reverse so later-drawn elements take precedence
    for r in regions.iter().rev() {
        if col >= r.rect.x
            && col < r.rect.x.saturating_add(r.rect.width)
            && row >= r.rect.y
            && row < r.rect.y.saturating_add(r.rect.height)
        {
            return Some(r.action.clone());
        }
    }
    None
}
