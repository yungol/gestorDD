use crate::app::{ActiveTab, App, HitAction, HitRegion, ModalState};
use crate::model::{
    format_age, format_bytes, format_count, truncate_str, FileCategory, SafetyLevel,
};
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph, Wrap};
use ratatui::Frame;
use unicode_width::UnicodeWidthStr;

// Modern vibrant terminal palette (Tokyo Night / Cyber Slate)
const BG_BASE: Color = Color::Rgb(15, 19, 28);
const BG_PANEL: Color = Color::Rgb(22, 27, 40);
const BG_ELEVATED: Color = Color::Rgb(31, 38, 56);
const BG_HOVER: Color = Color::Rgb(42, 52, 76);
const BG_SELECTED: Color = Color::Rgb(48, 66, 108);

const BORDER_DIM: Color = Color::Rgb(54, 66, 94);
const BORDER_ACCENT: Color = Color::Rgb(86, 204, 242);

const FG_PRIMARY: Color = Color::Rgb(235, 240, 250);
const FG_SECONDARY: Color = Color::Rgb(156, 170, 198);
const FG_MUTED: Color = Color::Rgb(96, 110, 138);

const ACCENT_CYAN: Color = Color::Rgb(86, 212, 255);
const ACCENT_GREEN: Color = Color::Rgb(74, 222, 128);
const ACCENT_AMBER: Color = Color::Rgb(251, 191, 36);
const ACCENT_RED: Color = Color::Rgb(248, 113, 113);
const ACCENT_VIOLET: Color = Color::Rgb(167, 139, 250);
const ACCENT_PINK: Color = Color::Rgb(244, 114, 182);
const ACCENT_BLUE: Color = Color::Rgb(96, 165, 250);

const CHART_COLORS: [Color; 6] = [
    ACCENT_CYAN,
    ACCENT_VIOLET,
    ACCENT_PINK,
    ACCENT_AMBER,
    ACCENT_GREEN,
    FG_MUTED,
];

const SPINNER: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

fn category_color(cat: FileCategory) -> Color {
    match cat {
        FileCategory::Video => ACCENT_PINK,
        FileCategory::Archive => ACCENT_AMBER,
        FileCategory::Image => ACCENT_VIOLET,
        FileCategory::Audio => ACCENT_GREEN,
        FileCategory::Code => ACCENT_CYAN,
        FileCategory::Document => ACCENT_BLUE,
        FileCategory::Other => FG_SECONDARY,
    }
}

fn usage_color(ratio: f64) -> Color {
    if ratio >= 0.85 {
        ACCENT_RED
    } else if ratio >= 0.65 {
        ACCENT_AMBER
    } else if ratio >= 0.35 {
        ACCENT_CYAN
    } else {
        ACCENT_GREEN
    }
}

fn bar_color(ratio: f64) -> Color {
    if ratio >= 0.50 {
        ACCENT_RED
    } else if ratio >= 0.20 {
        ACCENT_AMBER
    } else if ratio >= 0.06 {
        ACCENT_CYAN
    } else {
        ACCENT_GREEN
    }
}

fn register_hit(regions: &mut Vec<HitRegion>, rect: Rect, action: HitAction) {
    if rect.width > 0 && rect.height > 0 {
        regions.push(HitRegion { rect, action });
    }
}

fn render_pill_button(
    frame: &mut Frame,
    rect: Rect,
    label: &str,
    fg: Color,
    bg: Color,
    hover_bg: Color,
    is_hovered: bool,
    regions: &mut Vec<HitRegion>,
    action: HitAction,
) {
    if rect.width == 0 || rect.height == 0 {
        return;
    }
    let style = Style::default()
        .fg(fg)
        .bg(if is_hovered { hover_bg } else { bg })
        .add_modifier(Modifier::BOLD);
    let p = Paragraph::new(Line::from(Span::styled(
        truncate_str(label, rect.width as usize),
        style,
    )))
    .alignment(Alignment::Center)
    .style(style);
    frame.render_widget(p, rect);
    register_hit(regions, rect, action);
}

pub fn render(frame: &mut Frame, app: &mut App) {
    app.hit_regions.clear();
    app.modal_hit_regions.clear();

    let area = frame.area();
    // Fill entire terminal with sleek dark slate canvas
    frame.render_widget(Block::default().style(Style::default().bg(BG_BASE)), area);

    let main_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Header: brand + clickable tabs + actions
            Constraint::Length(2), // Disk summary bar + live scan indicator
            Constraint::Min(8),    // Main content
            Constraint::Length(2), // Footer / shortcuts / toast
        ])
        .split(area);

    render_header(frame, main_chunks[0], app);
    render_disk_bar(frame, main_chunks[1], app);

    match app.active_tab {
        ActiveTab::Explorer => render_explorer(frame, main_chunks[2], app),
        ActiveTab::Cleaner => render_cleaner(frame, main_chunks[2], app),
        ActiveTab::TopFiles => render_top_files(frame, main_chunks[2], app),
    }

    render_footer(frame, main_chunks[3], app);

    // Modals rendered on top
    match app.modal.clone() {
        ModalState::None => {}
        ModalState::ConfirmDelete {
            path,
            name,
            size,
            permanent,
        } => render_confirm_delete_modal(frame, area, app, &path, &name, size, permanent),
        ModalState::ConfirmClean { count, total_bytes } => {
            render_confirm_clean_modal(frame, area, app, count, total_bytes)
        }
        ModalState::Help => render_help_modal(frame, area, app),
    }
}

fn render_header(frame: &mut Frame, area: Rect, app: &mut App) {
    let block = Block::default()
        .borders(Borders::BOTTOM)
        .border_style(Style::default().fg(BORDER_DIM))
        .style(Style::default().bg(BG_PANEL));
    frame.render_widget(block, area);

    let row = Rect {
        x: area.x + 1,
        y: area.y + 1,
        width: area.width.saturating_sub(2),
        height: 1,
    };

    let mut x = row.x;

    // Brand badge
    let brand = " ⚡ GestorDD ";
    let brand_w = brand.width() as u16;
    if x + brand_w <= row.right() {
        let r = Rect::new(x, row.y, brand_w, 1);
        frame.render_widget(
            Paragraph::new(Span::styled(
                brand,
                Style::default()
                    .fg(BG_BASE)
                    .bg(ACCENT_CYAN)
                    .add_modifier(Modifier::BOLD),
            )),
            r,
        );
        x += brand_w + 2;
    }

    // Clickable Tabs
    let tabs = [
        (ActiveTab::Explorer, " [1] ▸ Explorador ", ACCENT_CYAN),
        (ActiveTab::Cleaner, " [2] ✦ Limpieza ", ACCENT_GREEN),
        (ActiveTab::TopFiles, " [3] ▲ Top Pesados ", ACCENT_PINK),
    ];

    for (tab, label, accent) in tabs {
        let w = label.width() as u16;
        if x + w > row.right().saturating_sub(26) {
            break;
        }
        let rect = Rect::new(x, row.y, w, 1);
        let active = app.active_tab == tab;
        let hovered = app.is_hovered(rect);
        let (fg, bg) = if active {
            (BG_BASE, accent)
        } else if hovered {
            (FG_PRIMARY, BG_HOVER)
        } else {
            (FG_SECONDARY, BG_ELEVATED)
        };
        let style = Style::default().fg(fg).bg(bg).add_modifier(if active {
            Modifier::BOLD
        } else {
            Modifier::empty()
        });
        frame.render_widget(Paragraph::new(Span::styled(label, style)), rect);
        register_hit(&mut app.hit_regions, rect, HitAction::Tab(tab));
        x += w + 1;
    }

    // Right-aligned action buttons
    let right_buttons = [
        (" ✕ Salir ", ACCENT_RED, HitAction::Quit),
        (" ? Ayuda ", ACCENT_VIOLET, HitAction::ToggleHelp),
        (" ↻ Escanear ", ACCENT_AMBER, HitAction::Rescan),
    ];

    let mut rx = row.right();
    for (label, color, action) in right_buttons {
        let w = label.width() as u16;
        if rx < row.x + w + 1 || rx - w < x {
            break;
        }
        rx -= w;
        let rect = Rect::new(rx, row.y, w, 1);
        let hovered = app.is_hovered(rect);
        render_pill_button(
            frame,
            rect,
            label,
            if hovered { BG_BASE } else { color },
            BG_ELEVATED,
            color,
            hovered,
            &mut app.hit_regions,
            action,
        );
        rx = rx.saturating_sub(1);
    }
}

fn render_disk_bar(frame: &mut Frame, area: Rect, app: &App) {
    let inner = Rect {
        x: area.x + 1,
        y: area.y,
        width: area.width.saturating_sub(2),
        height: area.height,
    };

    if let Some(stats) = &app.disk_stats {
        let ratio = stats.used_ratio();
        let color = usage_color(ratio);
        let pct = ratio * 100.0;

        let bar_width = 24usize;
        let filled = ((ratio * bar_width as f64).round() as usize).min(bar_width);
        let empty = bar_width - filled;

        let mut spans = vec![
            Span::styled(" DISCO ", Style::default().fg(BG_BASE).bg(color).add_modifier(Modifier::BOLD)),
            Span::raw(" "),
            Span::styled("█".repeat(filled), Style::default().fg(color)),
            Span::styled("░".repeat(empty), Style::default().fg(BORDER_DIM)),
            Span::styled(
                format!(" {:.1}%", pct),
                Style::default().fg(color).add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!(
                    "  Usado: {} / {}  │  Libre: ",
                    format_bytes(stats.used),
                    format_bytes(stats.total)
                ),
                Style::default().fg(FG_SECONDARY),
            ),
            Span::styled(
                format_bytes(stats.available),
                Style::default()
                    .fg(ACCENT_GREEN)
                    .add_modifier(Modifier::BOLD),
            ),
        ];

        if app.total_freed_session > 0 {
            spans.push(Span::styled("  │  ", Style::default().fg(BORDER_DIM)));
            spans.push(Span::styled(
                format!("✦ Liberado hoy: {}", format_bytes(app.total_freed_session)),
                Style::default()
                    .fg(ACCENT_GREEN)
                    .add_modifier(Modifier::BOLD),
            ));
        }

        frame.render_widget(
            Paragraph::new(Line::from(spans)),
            Rect::new(inner.x, inner.y, inner.width, 1),
        );
    }

    // Second line: live scan progress OR scanned root summary
    let status_rect = Rect::new(inner.x, inner.y + 1, inner.width, 1);
    if app.is_scanning {
        let spin = SPINNER[(app.tick_count as usize) % SPINNER.len()];
        let prefix = format!(
            "{} Escaneando... {} archivos · {} · ",
            spin,
            format_count(app.scan_files),
            format_bytes(app.scan_bytes)
        );
        let rem = (inner.width as usize).saturating_sub(prefix.width());
        let line = Line::from(vec![
            Span::styled(
                format!("{} Escaneando... ", spin),
                Style::default()
                    .fg(ACCENT_AMBER)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!(
                    "{} archivos · {} ",
                    format_count(app.scan_files),
                    format_bytes(app.scan_bytes)
                ),
                Style::default().fg(ACCENT_CYAN),
            ),
            Span::styled(
                truncate_str(&app.scan_current_path, rem),
                Style::default().fg(FG_MUTED),
            ),
        ]);
        frame.render_widget(Paragraph::new(line), status_rect);
    } else if let Some(root) = &app.root_node {
        let elapsed_str = app
            .last_scan_elapsed
            .map(|d| format!(" en {:.1}s", d.as_secs_f64()))
            .unwrap_or_default();
        let line = Line::from(vec![
            Span::styled("✓ Analizado: ", Style::default().fg(ACCENT_GREEN)),
            Span::styled(
                root.path.display().to_string(),
                Style::default().fg(FG_PRIMARY).add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!(
                    "  ({} en {} archivos{})",
                    format_bytes(root.size),
                    format_count(root.file_count),
                    elapsed_str
                ),
                Style::default().fg(FG_MUTED),
            ),
        ]);
        frame.render_widget(Paragraph::new(line), status_rect);
    }
}

fn render_explorer(frame: &mut Frame, area: Rect, app: &mut App) {
    // If initial scan hasn't finished yet and root_node is None, show engaging live progress card
    if app.root_node.is_none() {
        render_scanning_splash(frame, area, app);
        return;
    }

    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Breadcrumb + controls bar
            Constraint::Min(6),    // Left list + Right inspector
        ])
        .split(area);

    render_explorer_toolbar(frame, vertical[0], app);

    let wide = area.width >= 96;
    if wide {
        let split = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(63), Constraint::Percentage(37)])
            .split(vertical[1]);
        render_explorer_list(frame, split[0], app);
        render_explorer_inspector(frame, split[1], app);
    } else {
        let split = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Percentage(65), Constraint::Percentage(35)])
            .split(vertical[1]);
        render_explorer_list(frame, split[0], app);
        render_explorer_inspector(frame, split[1], app);
    }
}

fn render_scanning_splash(frame: &mut Frame, area: Rect, app: &App) {
    let box_w = area.width.min(64);
    let box_h = 9u16.min(area.height);
    let rect = centered_rect_fixed(box_w, box_h, area);

    let spin = SPINNER[(app.tick_count as usize) % SPINNER.len()];
    let block = Block::default()
        .title(Span::styled(
            format!(" {} Analizando tu disco duro ", spin),
            Style::default()
                .fg(ACCENT_CYAN)
                .add_modifier(Modifier::BOLD),
        ))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(ACCENT_CYAN))
        .style(Style::default().bg(BG_PANEL));

    let inner = block.inner(rect);
    frame.render_widget(block, rect);

    let lines = vec![
        Line::from(""),
        Line::from(vec![
            Span::styled("  Carpeta raíz: ", Style::default().fg(FG_SECONDARY)),
            Span::styled(
                truncate_str(&app.root_path.display().to_string(), inner.width.saturating_sub(18) as usize),
                Style::default().fg(FG_PRIMARY).add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(vec![
            Span::styled("  Descubierto:  ", Style::default().fg(FG_SECONDARY)),
            Span::styled(
                format_bytes(app.scan_bytes),
                Style::default()
                    .fg(ACCENT_GREEN)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("  ({} archivos)", format_count(app.scan_files)),
                Style::default().fg(ACCENT_AMBER),
            ),
        ]),
        Line::from(""),
        Line::from(Span::styled(
            format!(
                "  {}",
                truncate_str(&app.scan_current_path, inner.width.saturating_sub(4) as usize)
            ),
            Style::default().fg(FG_MUTED),
        )),
        Line::from(""),
        Line::from(Span::styled(
            "  Tip: Ya puedes hacer clic en [2] Limpieza mientras termina.",
            Style::default().fg(ACCENT_VIOLET),
        )),
    ];

    frame.render_widget(Paragraph::new(lines), inner);
}

fn render_explorer_toolbar(frame: &mut Frame, area: Rect, app: &mut App) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(BORDER_DIM))
        .style(Style::default().bg(BG_PANEL));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let mut x = inner.x + 1;

    // Clickable [↑ Subir] button
    let can_up = app.current_dir != app.root_path;
    let up_label = " ↑ Subir ";
    let up_w = up_label.width() as u16;
    let up_rect = Rect::new(x, inner.y, up_w, 1);
    let up_hover = can_up && app.is_hovered(up_rect);
    render_pill_button(
        frame,
        up_rect,
        up_label,
        if !can_up {
            FG_MUTED
        } else if up_hover {
            BG_BASE
        } else {
            ACCENT_CYAN
        },
        BG_ELEVATED,
        ACCENT_CYAN,
        up_hover,
        &mut app.hit_regions,
        HitAction::GoUp,
    );
    x += up_w + 1;

    // Right side controls: Sort button & Filter box
    let sort_label = format!(" ⇅ {} ", app.sort_mode.label());
    let sort_w = sort_label.width() as u16;
    let filter_w = 22u16.min(inner.width / 3);

    let sort_x = inner.right().saturating_sub(sort_w + 1);
    let filter_x = sort_x.saturating_sub(filter_w + 1);

    // Sort button
    if sort_x > x + 6 {
        let sort_rect = Rect::new(sort_x, inner.y, sort_w, 1);
        let hovered = app.is_hovered(sort_rect);
        render_pill_button(
            frame,
            sort_rect,
            &sort_label,
            if hovered { BG_BASE } else { ACCENT_VIOLET },
            BG_ELEVATED,
            ACCENT_VIOLET,
            hovered,
            &mut app.hit_regions,
            HitAction::CycleSort,
        );
    }

    // Filter box
    if filter_x > x + 6 {
        let filter_rect = Rect::new(filter_x, inner.y, filter_w, 1);
        let hovered = app.is_hovered(filter_rect);
        let display = if app.filter_editing {
            format!(" / {}▌", app.filter_text)
        } else if !app.filter_text.is_empty() {
            format!(" / {} ✕", app.filter_text)
        } else {
            " / Filtrar... ".to_string()
        };
        let style = Style::default()
            .fg(if app.filter_editing {
                ACCENT_AMBER
            } else if !app.filter_text.is_empty() {
                ACCENT_CYAN
            } else {
                FG_MUTED
            })
            .bg(if hovered || app.filter_editing {
                BG_HOVER
            } else {
                BG_ELEVATED
            });
        frame.render_widget(
            Paragraph::new(truncate_str(&display, filter_w as usize)).style(style),
            filter_rect,
        );
        let action = if !app.filter_editing && !app.filter_text.is_empty() {
            HitAction::ClearFilter
        } else {
            HitAction::ToggleFilter
        };
        register_hit(&mut app.hit_regions, filter_rect, action);
    }

    // Clickable Breadcrumbs in remaining space
    let max_crumb_x = filter_x.saturating_sub(2);
    let mut segments: Vec<(String, std::path::PathBuf)> = Vec::new();
    let mut cur = app.current_dir.clone();
    loop {
        let label = if cur == app.root_path {
            format!(
                "⌂ {}",
                cur.file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "/".into())
            )
        } else {
            cur.file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| "/".into())
        };
        segments.push((label, cur.clone()));
        if cur == app.root_path {
            break;
        }
        match cur.parent() {
            Some(p) if p.starts_with(&app.root_path) => cur = p.to_path_buf(),
            _ => break,
        }
    }
    segments.reverse();

    // Keep trailing segments that fit
    let mut visible_start = 0;
    let avail = max_crumb_x.saturating_sub(x) as usize;
    let total_w: usize = segments.iter().map(|(s, _)| s.width() + 3).sum();
    if total_w > avail && segments.len() > 1 {
        let mut acc = 0;
        visible_start = segments.len() - 1;
        for i in (0..segments.len()).rev() {
            let sw = segments[i].0.width() + 3;
            if acc + sw <= avail {
                acc += sw;
                visible_start = i;
            } else {
                break;
            }
        }
    }

    for (idx, (label, path)) in segments.iter().enumerate().skip(visible_start) {
        if idx > visible_start {
            let sep_rect = Rect::new(x, inner.y, 3.min(max_crumb_x.saturating_sub(x)), 1);
            frame.render_widget(
                Paragraph::new(Span::styled(" › ", Style::default().fg(FG_MUTED))),
                sep_rect,
            );
            x += 3;
        }
        let is_last = idx + 1 == segments.len();
        let seg_text = format!(" {} ", label);
        let w = (seg_text.width() as u16).min(max_crumb_x.saturating_sub(x));
        if w == 0 {
            break;
        }
        let rect = Rect::new(x, inner.y, w, 1);
        let hovered = app.is_hovered(rect);
        let style = Style::default()
            .fg(if is_last {
                FG_PRIMARY
            } else if hovered {
                ACCENT_CYAN
            } else {
                FG_SECONDARY
            })
            .bg(if hovered { BG_HOVER } else { BG_PANEL })
            .add_modifier(if is_last {
                Modifier::BOLD
            } else {
                Modifier::empty()
            });
        frame.render_widget(
            Paragraph::new(truncate_str(&seg_text, w as usize)).style(style),
            rect,
        );
        register_hit(&mut app.hit_regions, rect, HitAction::Breadcrumb(path.clone()));
        x += w;
    }
}

fn render_explorer_list(frame: &mut Frame, area: Rect, app: &mut App) {
    let current_size = app.current_node().map(|n| n.size).unwrap_or(0);
    let entries = app.explorer_entries();

    let title = format!(
        " Contenido ({} elementos · {}) ",
        entries.iter().filter(|e| !e.is_parent_link).count(),
        format_bytes(current_size)
    );
    let block = Block::default()
        .title(Span::styled(
            title,
            Style::default()
                .fg(ACCENT_CYAN)
                .add_modifier(Modifier::BOLD),
        ))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(BORDER_ACCENT))
        .style(Style::default().bg(BG_PANEL));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    if inner.height < 2 {
        return;
    }

    // Column header row
    let header_rect = Rect::new(inner.x, inner.y, inner.width, 1);
    let bar_col_w = if inner.width >= 68 { 14usize } else { 8usize };
    let size_col_w = 10usize;
    let pct_col_w = 7usize;
    let name_col_w = (inner.width as usize).saturating_sub(bar_col_w + size_col_w + pct_col_w + 5);

    let header_line = Line::from(vec![
        Span::styled(
            format!("  {:<width$}", "Nombre", width = name_col_w),
            Style::default().fg(FG_MUTED).add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("{:<width$}", "Uso", width = bar_col_w + 1),
            Style::default().fg(FG_MUTED).add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("{:>width$}", "Tamaño", width = size_col_w),
            Style::default().fg(FG_MUTED).add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("{:>width$}", "%", width = pct_col_w),
            Style::default().fg(FG_MUTED).add_modifier(Modifier::BOLD),
        ),
    ]);
    frame.render_widget(
        Paragraph::new(header_line).style(Style::default().bg(BG_ELEVATED)),
        header_rect,
    );

    let list_y = inner.y + 1;
    let visible_rows = inner.height.saturating_sub(1) as usize;

    if entries.is_empty() {
        frame.render_widget(
            Paragraph::new(Span::styled(
                "  (Carpeta vacía o sin coincidencias con el filtro)",
                Style::default().fg(FG_MUTED),
            )),
            Rect::new(inner.x, list_y + 1, inner.width, 1),
        );
        return;
    }

    // Adjust scroll offset to keep selected row visible
    if app.explorer_selected < app.explorer_scroll {
        app.explorer_scroll = app.explorer_selected;
    } else if app.explorer_selected >= app.explorer_scroll + visible_rows {
        app.explorer_scroll = app.explorer_selected + 1 - visible_rows;
    }

    for (row_offset, idx) in (app.explorer_scroll..entries.len())
        .take(visible_rows)
        .enumerate()
    {
        let entry = &entries[idx];
        let row_rect = Rect::new(inner.x, list_y + row_offset as u16, inner.width, 1);
        let selected = idx == app.explorer_selected;
        let hovered = app.is_hovered(row_rect);

        let bg = if selected {
            BG_SELECTED
        } else if hovered {
            BG_HOVER
        } else if row_offset % 2 == 1 {
            Color::Rgb(20, 25, 37)
        } else {
            BG_PANEL
        };

        let ratio = if current_size > 0 && !entry.is_parent_link {
            (entry.size as f64 / current_size as f64).clamp(0.0, 1.0)
        } else {
            0.0
        };

        let (glyph, glyph_color, name_color) = if entry.is_parent_link {
            ("↑", ACCENT_CYAN, ACCENT_CYAN)
        } else if entry.is_dir {
            ("▸", ACCENT_AMBER, FG_PRIMARY)
        } else {
            (
                entry.category.glyph(),
                category_color(entry.category),
                FG_SECONDARY,
            )
        };

        let display_name = truncate_str(&entry.name, name_col_w.saturating_sub(2));
        let pad_len = name_col_w
            .saturating_sub(2)
            .saturating_sub(display_name.width());

        let bar_filled = ((ratio * bar_col_w as f64).round() as usize).min(bar_col_w);
        let bar_empty = bar_col_w - bar_filled;
        let b_color = bar_color(ratio);

        let (size_str, pct_str) = if entry.is_parent_link {
            ("".to_string(), "".to_string())
        } else {
            (format_bytes(entry.size), format!("{:5.1}%", ratio * 100.0))
        };

        let pointer = if selected { "▌" } else { " " };
        let line = Line::from(vec![
            Span::styled(pointer, Style::default().fg(ACCENT_CYAN).bg(bg)),
            Span::styled(format!("{} ", glyph), Style::default().fg(glyph_color).bg(bg)),
            Span::styled(
                format!("{}{}", display_name, " ".repeat(pad_len)),
                Style::default()
                    .fg(name_color)
                    .bg(bg)
                    .add_modifier(if selected || entry.is_dir {
                        Modifier::BOLD
                    } else {
                        Modifier::empty()
                    }),
            ),
            Span::raw(" "),
            if entry.is_parent_link {
                Span::styled(" ".repeat(bar_col_w), Style::default().bg(bg))
            } else {
                Span::styled("█".repeat(bar_filled), Style::default().fg(b_color).bg(bg))
            },
            if entry.is_parent_link {
                Span::styled("", Style::default().bg(bg))
            } else {
                Span::styled("░".repeat(bar_empty), Style::default().fg(BORDER_DIM).bg(bg))
            },
            Span::styled(
                format!(" {:>width$}", size_str, width = size_col_w),
                Style::default()
                    .fg(if selected { FG_PRIMARY } else { b_color })
                    .bg(bg)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("{:>width$}", pct_str, width = pct_col_w),
                Style::default().fg(FG_MUTED).bg(bg),
            ),
        ]);

        frame.render_widget(Paragraph::new(line).style(Style::default().bg(bg)), row_rect);
        register_hit(&mut app.hit_regions, row_rect, HitAction::ExplorerRow(idx));
    }
}

fn render_explorer_inspector(frame: &mut Frame, area: Rect, app: &mut App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(10), // Visual distribution bar + legend
            Constraint::Min(9),     // Selected item details + clickable action buttons
        ])
        .split(area);

    // 1. Visual storage breakdown of current folder
    let map_block = Block::default()
        .title(Span::styled(
            " ◈ Mapa visual de esta carpeta ",
            Style::default()
                .fg(ACCENT_VIOLET)
                .add_modifier(Modifier::BOLD),
        ))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(BORDER_DIM))
        .style(Style::default().bg(BG_PANEL));

    let map_inner = map_block.inner(chunks[0]);
    frame.render_widget(map_block, chunks[0]);

    if let Some(node) = app.current_node() {
        let total = node.size.max(1);
        let bar_w = map_inner.width.saturating_sub(2) as usize;
        let top5: Vec<_> = node.children.iter().take(5).collect();
        let top5_sum: u64 = top5.iter().map(|c| c.size).sum();
        let rest = node.size.saturating_sub(top5_sum);

        let mut bar_spans = vec![Span::raw(" ")];
        let mut used_chars = 0usize;
        for (i, c) in top5.iter().enumerate() {
            let ratio = c.size as f64 / total as f64;
            let mut seg = (ratio * bar_w as f64).round() as usize;
            if seg == 0 && c.size > 0 && used_chars < bar_w {
                seg = 1;
            }
            seg = seg.min(bar_w.saturating_sub(used_chars));
            used_chars += seg;
            bar_spans.push(Span::styled(
                "█".repeat(seg),
                Style::default().fg(CHART_COLORS[i % CHART_COLORS.len()]),
            ));
        }
        if used_chars < bar_w {
            bar_spans.push(Span::styled(
                "▒".repeat(bar_w - used_chars),
                Style::default().fg(if rest > 0 { FG_MUTED } else { BORDER_DIM }),
            ));
        }

        let mut lines = vec![Line::from(bar_spans), Line::from("")];
        let name_max = map_inner.width.saturating_sub(21) as usize;
        for (i, c) in top5.iter().enumerate() {
            let pct = (c.size as f64 / total as f64) * 100.0;
            lines.push(Line::from(vec![
                Span::styled(" ● ", Style::default().fg(CHART_COLORS[i])),
                Span::styled(
                    format!("{:<width$}", truncate_str(&c.name, name_max), width = name_max),
                    Style::default().fg(FG_PRIMARY),
                ),
                Span::styled(
                    format!(" {:>9}", format_bytes(c.size)),
                    Style::default()
                        .fg(CHART_COLORS[i])
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(format!(" {:>5.1}%", pct), Style::default().fg(FG_MUTED)),
            ]));
        }
        if rest > 0 {
            let pct = (rest as f64 / total as f64) * 100.0;
            lines.push(Line::from(vec![
                Span::styled(" ● ", Style::default().fg(FG_MUTED)),
                Span::styled(
                    format!("{:<width$}", "Otros elementos", width = name_max),
                    Style::default().fg(FG_MUTED),
                ),
                Span::styled(
                    format!(" {:>9}", format_bytes(rest)),
                    Style::default().fg(FG_SECONDARY),
                ),
                Span::styled(format!(" {:>5.1}%", pct), Style::default().fg(FG_MUTED)),
            ]));
        }

        frame.render_widget(Paragraph::new(lines), map_inner);
    }

    // 2. Selected item inspector + Clickable action buttons
    let det_block = Block::default()
        .title(Span::styled(
            " ◉ Inspector y Acciones ",
            Style::default()
                .fg(ACCENT_GREEN)
                .add_modifier(Modifier::BOLD),
        ))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(BORDER_DIM))
        .style(Style::default().bg(BG_PANEL));

    let det_inner = det_block.inner(chunks[1]);
    frame.render_widget(det_block, chunks[1]);

    if det_inner.height < 4 {
        return;
    }

    let det_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(3),    // Metadata details
            Constraint::Length(1), // Action row 1
            Constraint::Length(1), // Spacer
            Constraint::Length(1), // Action row 2
        ])
        .split(det_inner);

    let selected = app.selected_explorer_entry();
    let max_val_w = det_layout[0].width.saturating_sub(14) as usize;

    let info_lines = if let Some(entry) = &selected {
        if entry.is_parent_link {
            vec![
                Line::from(Span::styled(
                    "  Haz clic para subir al directorio padre.",
                    Style::default().fg(FG_SECONDARY),
                )),
                Line::from(""),
                Line::from(Span::styled(
                    "  Tip: Clic derecho en cualquier parte también sube.",
                    Style::default().fg(FG_MUTED),
                )),
            ]
        } else {
            let kind_str = if entry.is_dir {
                format!("Carpeta ({} archivos)", format_count(entry.file_count))
            } else {
                format!("Archivo · {}", entry.category.label())
            };
            vec![
                Line::from(vec![
                    Span::styled("  Nombre:   ", Style::default().fg(FG_MUTED)),
                    Span::styled(
                        truncate_str(&entry.name, max_val_w),
                        Style::default().fg(FG_PRIMARY).add_modifier(Modifier::BOLD),
                    ),
                ]),
                Line::from(vec![
                    Span::styled("  Tamaño:   ", Style::default().fg(FG_MUTED)),
                    Span::styled(
                        format_bytes(entry.size),
                        Style::default()
                            .fg(ACCENT_CYAN)
                            .add_modifier(Modifier::BOLD),
                    ),
                ]),
                Line::from(vec![
                    Span::styled("  Tipo:     ", Style::default().fg(FG_MUTED)),
                    Span::styled(kind_str, Style::default().fg(FG_SECONDARY)),
                ]),
                Line::from(vec![
                    Span::styled("  Modific.: ", Style::default().fg(FG_MUTED)),
                    Span::styled(format_age(entry.modified), Style::default().fg(FG_SECONDARY)),
                ]),
                Line::from(vec![
                    Span::styled("  Ruta:     ", Style::default().fg(FG_MUTED)),
                    Span::styled(
                        truncate_str(&entry.path.display().to_string(), max_val_w),
                        Style::default().fg(FG_MUTED),
                    ),
                ]),
            ]
        }
    } else {
        vec![Line::from(Span::styled(
            "  Selecciona un elemento de la lista.",
            Style::default().fg(FG_MUTED),
        ))]
    };

    frame.render_widget(Paragraph::new(info_lines).wrap(Wrap { trim: false }), det_layout[0]);

    // Clickable Action Buttons (Row 1: Open + Open External)
    let btn_row1 = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(1),
            Constraint::Percentage(48),
            Constraint::Length(1),
            Constraint::Percentage(48),
            Constraint::Length(1),
        ])
        .split(det_layout[1]);

    let is_dir = selected.as_ref().map(|e| e.is_dir).unwrap_or(false);
    let open_label = if is_dir { "▸ Entrar" } else { "▸ Abrir" };
    let h1 = app.is_hovered(btn_row1[1]);
    render_pill_button(
        frame,
        btn_row1[1],
        open_label,
        if h1 { BG_BASE } else { ACCENT_CYAN },
        BG_ELEVATED,
        ACCENT_CYAN,
        h1,
        &mut app.hit_regions,
        HitAction::ActionOpen,
    );

    let h2 = app.is_hovered(btn_row1[3]);
    render_pill_button(
        frame,
        btn_row1[3],
        "↗ Ver en Carpeta",
        if h2 { BG_BASE } else { ACCENT_VIOLET },
        BG_ELEVATED,
        ACCENT_VIOLET,
        h2,
        &mut app.hit_regions,
        HitAction::ActionOpenExternal,
    );

    // Clickable Action Buttons (Row 2: Trash + Permanent Delete)
    let btn_row2 = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(1),
            Constraint::Percentage(48),
            Constraint::Length(1),
            Constraint::Percentage(48),
            Constraint::Length(1),
        ])
        .split(det_layout[3]);

    let h3 = app.is_hovered(btn_row2[1]);
    render_pill_button(
        frame,
        btn_row2[1],
        "⌫ A la Papelera",
        if h3 { BG_BASE } else { ACCENT_AMBER },
        BG_ELEVATED,
        ACCENT_AMBER,
        h3,
        &mut app.hit_regions,
        HitAction::ActionTrash,
    );

    let h4 = app.is_hovered(btn_row2[3]);
    render_pill_button(
        frame,
        btn_row2[3],
        "✕ Borrar def.",
        if h4 { BG_BASE } else { ACCENT_RED },
        BG_ELEVATED,
        ACCENT_RED,
        h4,
        &mut app.hit_regions,
        HitAction::ActionDeletePerm,
    );
}

fn render_cleaner(frame: &mut Frame, area: Rect, app: &mut App) {
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(4), // Cleaner summary banner
            Constraint::Min(6),    // Clean targets interactive checklist
            Constraint::Length(3), // Bottom action buttons
        ])
        .split(area);

    let (sel_count, sel_bytes) = app.selected_clean_bytes();
    let total_rec = app.total_reclaimable_bytes();

    // 1. Summary Banner
    let banner_block = Block::default()
        .title(Span::styled(
            " ✦ Asistente de Limpieza Inteligente ",
            Style::default()
                .fg(ACCENT_GREEN)
                .add_modifier(Modifier::BOLD),
        ))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(ACCENT_GREEN))
        .style(Style::default().bg(BG_PANEL));
    let b_inner = banner_block.inner(layout[0]);
    frame.render_widget(banner_block, layout[0]);

    let banner_lines = vec![
        Line::from(vec![
            Span::styled("  Espacio recuperable detectado: ", Style::default().fg(FG_SECONDARY)),
            Span::styled(
                format_bytes(total_rec),
                Style::default()
                    .fg(ACCENT_GREEN)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("   │   Seleccionado para limpiar: ", Style::default().fg(FG_SECONDARY)),
            Span::styled(
                format!("{} ({} categorías)", format_bytes(sel_bytes), sel_count),
                Style::default()
                    .fg(ACCENT_AMBER)
                    .add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(Span::styled(
            "  Haz clic en cada fila para marcar/desmarcar. Los elementos [SEGURA] no afectan tus archivos personales.",
            Style::default().fg(FG_MUTED),
        )),
    ];
    frame.render_widget(Paragraph::new(banner_lines), b_inner);

    // 2. Checklist
    let list_block = Block::default()
        .title(Span::styled(
            format!(" Categorías detectadas ({}) ", app.clean_targets.len()),
            Style::default()
                .fg(ACCENT_CYAN)
                .add_modifier(Modifier::BOLD),
        ))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(BORDER_DIM))
        .style(Style::default().bg(BG_PANEL));
    let l_inner = list_block.inner(layout[1]);
    frame.render_widget(list_block, layout[1]);

    if app.clean_targets.is_empty() {
        frame.render_widget(
            Paragraph::new(Span::styled(
                "  ¡Tu equipo está súper limpio! No se detectaron cachés grandes ni basura.",
                Style::default().fg(ACCENT_GREEN),
            )),
            Rect::new(l_inner.x, l_inner.y + 1, l_inner.width, 1),
        );
    } else {
        // Each item uses 2 lines (Title + Description)
        let items_per_page = (l_inner.height as usize / 2).max(1);
        if app.cleaner_selected < app.cleaner_scroll {
            app.cleaner_scroll = app.cleaner_selected;
        } else if app.cleaner_selected >= app.cleaner_scroll + items_per_page {
            app.cleaner_scroll = app.cleaner_selected + 1 - items_per_page;
        }

        for (slot, idx) in (app.cleaner_scroll..app.clean_targets.len())
            .take(items_per_page)
            .enumerate()
        {
            let target = &app.clean_targets[idx];
            let y = l_inner.y + (slot * 2) as u16;
            let row_rect = Rect::new(l_inner.x, y, l_inner.width, 2.min(l_inner.bottom() - y));
            let selected_row = idx == app.cleaner_selected;
            let hovered = app.is_hovered(row_rect);

            let bg = if selected_row {
                BG_SELECTED
            } else if hovered {
                BG_HOVER
            } else if slot % 2 == 1 {
                Color::Rgb(20, 25, 37)
            } else {
                BG_PANEL
            };

            let (badge_fg, badge_bg) = match target.safety {
                SafetyLevel::Safe => (BG_BASE, ACCENT_GREEN),
                SafetyLevel::Review => (BG_BASE, ACCENT_AMBER),
                SafetyLevel::SystemInfo => (BG_BASE, ACCENT_BLUE),
            };

            let checkbox = if target.safety == SafetyLevel::SystemInfo {
                " [i] "
            } else if target.selected {
                " [✓] "
            } else {
                " [ ] "
            };
            let cb_color = if target.selected {
                ACCENT_GREEN
            } else {
                FG_MUTED
            };

            let size_str = format!("{:>10}", format_bytes(target.size));
            let name_w = (l_inner.width as usize).saturating_sub(30);

            let line1 = Line::from(vec![
                Span::styled(
                    checkbox,
                    Style::default()
                        .fg(cb_color)
                        .bg(bg)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!(" {} ", target.safety.label()),
                    Style::default()
                        .fg(badge_fg)
                        .bg(badge_bg)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled("  ", Style::default().bg(bg)),
                Span::styled(
                    format!(
                        "{:<width$}",
                        truncate_str(&target.name, name_w),
                        width = name_w
                    ),
                    Style::default()
                        .fg(FG_PRIMARY)
                        .bg(bg)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    size_str,
                    Style::default()
                        .fg(if target.selected {
                            ACCENT_GREEN
                        } else {
                            ACCENT_CYAN
                        })
                        .bg(bg)
                        .add_modifier(Modifier::BOLD),
                ),
            ]);

            let desc_max = l_inner.width.saturating_sub(8) as usize;
            let desc_with_count = if target.item_count > 1 {
                format!(
                    "({} elementos) {}",
                    format_count(target.item_count),
                    target.description
                )
            } else {
                target.description.clone()
            };
            let line2 = Line::from(vec![
                Span::styled("      ", Style::default().bg(bg)),
                Span::styled(
                    truncate_str(&desc_with_count, desc_max),
                    Style::default().fg(FG_MUTED).bg(bg),
                ),
            ]);

            frame.render_widget(
                Paragraph::new(vec![line1, line2]).style(Style::default().bg(bg)),
                row_rect,
            );
            register_hit(&mut app.hit_regions, row_rect, HitAction::CleanerRow(idx));
        }
    }

    // 3. Bottom Action Buttons Bar
    let act_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(BORDER_DIM))
        .style(Style::default().bg(BG_PANEL));
    let a_inner = act_block.inner(layout[2]);
    frame.render_widget(act_block, layout[2]);

    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(22),
            Constraint::Length(2),
            Constraint::Length(18),
            Constraint::Min(2),
            Constraint::Length(36),
            Constraint::Length(1),
        ])
        .split(a_inner);

    let h_safe = app.is_hovered(cols[1]);
    render_pill_button(
        frame,
        cols[1],
        " ✓ Marcar seguros ",
        if h_safe { BG_BASE } else { ACCENT_CYAN },
        BG_ELEVATED,
        ACCENT_CYAN,
        h_safe,
        &mut app.hit_regions,
        HitAction::CleanerSelectSafe,
    );

    let h_none = app.is_hovered(cols[3]);
    render_pill_button(
        frame,
        cols[3],
        " ☐ Desmarcar ",
        if h_none { BG_BASE } else { FG_SECONDARY },
        BG_ELEVATED,
        FG_SECONDARY,
        h_none,
        &mut app.hit_regions,
        HitAction::CleanerSelectNone,
    );

    let run_label = if sel_count > 0 {
        format!(" ✦ Limpiar ahora ({}) ", format_bytes(sel_bytes))
    } else {
        " ✦ Selecciona qué limpiar ".to_string()
    };
    let h_run = sel_count > 0 && app.is_hovered(cols[5]);
    render_pill_button(
        frame,
        cols[5],
        &run_label,
        if sel_count > 0 { BG_BASE } else { FG_MUTED },
        if sel_count > 0 {
            ACCENT_GREEN
        } else {
            BG_ELEVATED
        },
        ACCENT_CYAN,
        h_run,
        &mut app.hit_regions,
        HitAction::CleanerRun,
    );
}

fn render_top_files(frame: &mut Frame, area: Rect, app: &mut App) {
    if app.root_node.is_none() {
        render_scanning_splash(frame, area, app);
        return;
    }

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Category filter pills
            Constraint::Min(6),    // Ranked files list
            Constraint::Length(3), // Action bar
        ])
        .split(area);

    // 1. Clickable Category Filter Chips
    let chip_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(BORDER_DIM))
        .style(Style::default().bg(BG_PANEL));
    let c_inner = chip_block.inner(layout[0]);
    frame.render_widget(chip_block, layout[0]);

    let mut x = c_inner.x + 1;
    let mut chips: Vec<(Option<FileCategory>, String, Color)> =
        vec![(None, " ★ Todos ".to_string(), ACCENT_CYAN)];
    for cat in FileCategory::ALL {
        chips.push((
            Some(cat),
            format!(" {} {} ", cat.glyph(), cat.label()),
            category_color(cat),
        ));
    }

    for (cat_opt, label, color) in chips {
        let w = label.width() as u16;
        if x + w > c_inner.right() {
            break;
        }
        let rect = Rect::new(x, c_inner.y, w, 1);
        let active = app.top_category_filter == cat_opt;
        let hovered = app.is_hovered(rect);
        let (fg, bg) = if active {
            (BG_BASE, color)
        } else if hovered {
            (FG_PRIMARY, BG_HOVER)
        } else {
            (color, BG_ELEVATED)
        };
        frame.render_widget(
            Paragraph::new(Span::styled(
                label,
                Style::default()
                    .fg(fg)
                    .bg(bg)
                    .add_modifier(if active { Modifier::BOLD } else { Modifier::empty() }),
            )),
            rect,
        );
        register_hit(&mut app.hit_regions, rect, HitAction::TopCategory(cat_opt));
        x += w + 1;
    }

    // 2. Ranked List
    let files = app.filtered_top_files();
    let max_size = files.first().map(|f| f.size).unwrap_or(1).max(1);
    let list_block = Block::default()
        .title(Span::styled(
            format!(" ▲ Archivos individuales más pesados ({}) ", files.len()),
            Style::default()
                .fg(ACCENT_PINK)
                .add_modifier(Modifier::BOLD),
        ))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(ACCENT_PINK))
        .style(Style::default().bg(BG_PANEL));
    let l_inner = list_block.inner(layout[1]);
    frame.render_widget(list_block, layout[1]);

    let visible_rows = l_inner.height as usize;
    if files.is_empty() {
        frame.render_widget(
            Paragraph::new(Span::styled(
                "  No hay archivos en esta categoría.",
                Style::default().fg(FG_MUTED),
            )),
            Rect::new(l_inner.x, l_inner.y + 1, l_inner.width, 1),
        );
    } else {
        if app.top_selected < app.top_scroll {
            app.top_scroll = app.top_selected;
        } else if app.top_selected >= app.top_scroll + visible_rows {
            app.top_scroll = app.top_selected + 1 - visible_rows;
        }

        let bar_w = if l_inner.width >= 90 { 12usize } else { 8usize };
        let show_age = l_inner.width >= 106;
        let age_w = if show_age { 14usize } else { 0usize };
        let fixed_w = bar_w + 22 + age_w;
        let name_w = ((l_inner.width as usize).saturating_sub(fixed_w) * 45) / 100;
        let path_w = (l_inner.width as usize).saturating_sub(fixed_w + name_w);

        for (slot, idx) in (app.top_scroll..files.len())
            .take(visible_rows)
            .enumerate()
        {
            let f = &files[idx];
            let rect = Rect::new(l_inner.x, l_inner.y + slot as u16, l_inner.width, 1);
            let selected = idx == app.top_selected;
            let hovered = app.is_hovered(rect);

            let bg = if selected {
                BG_SELECTED
            } else if hovered {
                BG_HOVER
            } else if slot % 2 == 1 {
                Color::Rgb(20, 25, 37)
            } else {
                BG_PANEL
            };

            let ratio = (f.size as f64 / max_size as f64).clamp(0.0, 1.0);
            let filled = ((ratio * bar_w as f64).round() as usize).min(bar_w);
            let empty = bar_w - filled;
            let c_color = category_color(f.category);

            let rel_parent = f
                .parent
                .strip_prefix(&app.root_path)
                .map(|p| format!("/{}", p.display()))
                .unwrap_or_else(|_| f.parent.display().to_string());

            let name_disp = truncate_str(&f.name, name_w);
            let name_pad = name_w.saturating_sub(name_disp.width());
            let path_disp = truncate_str(&rel_parent, path_w);
            let path_pad = path_w.saturating_sub(path_disp.width());

            let mut spans = vec![
                Span::styled(
                    if selected { "▌" } else { " " },
                    Style::default().fg(ACCENT_PINK).bg(bg),
                ),
                Span::styled(
                    format!("{:>2}. ", idx + 1),
                    Style::default().fg(FG_MUTED).bg(bg),
                ),
                Span::styled(
                    format!("{} ", f.category.glyph()),
                    Style::default().fg(c_color).bg(bg),
                ),
                Span::styled(
                    format!("{}{}", name_disp, " ".repeat(name_pad)),
                    Style::default()
                        .fg(FG_PRIMARY)
                        .bg(bg)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(" ", Style::default().bg(bg)),
                Span::styled(
                    format!("{}{}", path_disp, " ".repeat(path_pad)),
                    Style::default().fg(FG_MUTED).bg(bg),
                ),
                Span::styled(" ", Style::default().bg(bg)),
            ];
            if show_age {
                let age_disp = truncate_str(&format_age(f.modified), age_w - 1);
                spans.push(Span::styled(
                    format!("{:>width$} ", age_disp, width = age_w - 1),
                    Style::default().fg(FG_MUTED).bg(bg),
                ));
            }
            spans.push(Span::styled(
                "█".repeat(filled),
                Style::default().fg(c_color).bg(bg),
            ));
            spans.push(Span::styled(
                "░".repeat(empty),
                Style::default().fg(BORDER_DIM).bg(bg),
            ));
            spans.push(Span::styled(
                format!(" {:>10}", format_bytes(f.size)),
                Style::default()
                    .fg(ACCENT_AMBER)
                    .bg(bg)
                    .add_modifier(Modifier::BOLD),
            ));

            let line = Line::from(spans);

            frame.render_widget(Paragraph::new(line).style(Style::default().bg(bg)), rect);
            register_hit(&mut app.hit_regions, rect, HitAction::TopRow(idx));
        }
    }

    // 3. Action bar
    let act_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(BORDER_DIM))
        .style(Style::default().bg(BG_PANEL));
    let a_inner = act_block.inner(layout[2]);
    frame.render_widget(act_block, layout[2]);

    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(24),
            Constraint::Length(2),
            Constraint::Length(24),
            Constraint::Min(1),
        ])
        .split(a_inner);

    let h_jump = app.is_hovered(cols[1]);
    render_pill_button(
        frame,
        cols[1],
        " ▸ Ir a su carpeta ",
        if h_jump { BG_BASE } else { ACCENT_CYAN },
        BG_ELEVATED,
        ACCENT_CYAN,
        h_jump,
        &mut app.hit_regions,
        HitAction::TopJumpFolder,
    );

    let h_trash = app.is_hovered(cols[3]);
    render_pill_button(
        frame,
        cols[3],
        " ⌫ Enviar a Papelera ",
        if h_trash { BG_BASE } else { ACCENT_AMBER },
        BG_ELEVATED,
        ACCENT_AMBER,
        h_trash,
        &mut app.hit_regions,
        HitAction::TopTrash,
    );
}

fn render_footer(frame: &mut Frame, area: Rect, app: &App) {
    let block = Block::default()
        .borders(Borders::TOP)
        .border_style(Style::default().fg(BORDER_DIM))
        .style(Style::default().bg(BG_PANEL));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if let Some(toast) = &app.toast {
        let color = if toast.is_error {
            ACCENT_RED
        } else {
            ACCENT_GREEN
        };
        let badge = if toast.is_error { " ! AVISO " } else { " ✓ LISTO " };
        let line = Line::from(vec![
            Span::raw(" "),
            Span::styled(
                badge,
                Style::default()
                    .fg(BG_BASE)
                    .bg(color)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw("  "),
            Span::styled(
                &toast.message,
                Style::default().fg(FG_PRIMARY).add_modifier(Modifier::BOLD),
            ),
        ]);
        frame.render_widget(Paragraph::new(line), inner);
        return;
    }

    let hints = match app.active_tab {
        ActiveTab::Explorer => {
            " Mouse: Clic selecciona · Doble clic entra · Rueda scroll · Clic der. sube   │   Teclas: ↑↓ Enter ⌫(Subir) /Filtrar d(Papelera) r(Escanear) q(Salir)"
        }
        ActiveTab::Cleaner => {
            " Mouse: Clic marca/desmarca categoría · Botón verde ejecuta limpieza   │   Teclas: ↑↓ Espacio a(Seguros) n(Ninguno) c(Limpiar) q(Salir)"
        }
        ActiveTab::TopFiles => {
            " Mouse: Clic en categoría filtra · Doble clic abre carpeta del archivo   │   Teclas: ↑↓ Enter(Ir a carpeta) d(Papelera) /Filtrar q(Salir)"
        }
    };

    frame.render_widget(
        Paragraph::new(Span::styled(
            truncate_str(hints, inner.width as usize),
            Style::default().fg(FG_MUTED),
        )),
        inner,
    );
}

fn render_confirm_delete_modal(
    frame: &mut Frame,
    area: Rect,
    app: &mut App,
    path: &std::path::Path,
    name: &str,
    size: u64,
    permanent: bool,
) {
    let modal_rect = centered_rect_fixed(area.width.min(62), 11, area);
    frame.render_widget(Clear, modal_rect);

    let accent = if permanent { ACCENT_RED } else { ACCENT_AMBER };
    let title = if permanent {
        " ✕ Confirmar eliminación DEFINITIVA "
    } else {
        " ⌫ Mover a la Papelera "
    };

    let block = Block::default()
        .title(Span::styled(
            title,
            Style::default().fg(accent).add_modifier(Modifier::BOLD),
        ))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(accent))
        .style(Style::default().bg(BG_ELEVATED));
    let inner = block.inner(modal_rect);
    frame.render_widget(block, modal_rect);

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(5), Constraint::Length(1), Constraint::Length(1)])
        .split(inner);

    let warn = if permanent {
        "⚠ Esta acción NO se puede deshacer."
    } else {
        "Podrás restaurarlo desde la Papelera de Linux Mint."
    };

    let max_w = layout[0].width.saturating_sub(4) as usize;
    let lines = vec![
        Line::from(""),
        Line::from(vec![
            Span::styled("  Elemento: ", Style::default().fg(FG_SECONDARY)),
            Span::styled(
                truncate_str(name, max_w.saturating_sub(10)),
                Style::default().fg(FG_PRIMARY).add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(vec![
            Span::styled("  Tamaño:   ", Style::default().fg(FG_SECONDARY)),
            Span::styled(
                format_bytes(size),
                Style::default().fg(accent).add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(vec![
            Span::styled("  Ruta:     ", Style::default().fg(FG_SECONDARY)),
            Span::styled(
                truncate_str(&path.display().to_string(), max_w.saturating_sub(10)),
                Style::default().fg(FG_MUTED),
            ),
        ]),
        Line::from(""),
        Line::from(Span::styled(
            format!("  {}", warn),
            Style::default().fg(accent),
        )),
    ];
    frame.render_widget(Paragraph::new(lines), layout[0]);

    let btns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(3),
            Constraint::Percentage(42),
            Constraint::Min(2),
            Constraint::Percentage(42),
            Constraint::Length(3),
        ])
        .split(layout[1]);

    let h_ok = app.is_hovered(btns[1]);
    render_pill_button(
        frame,
        btns[1],
        if permanent {
            " ✓ Sí, borrar "
        } else {
            " ✓ Sí, a Papelera "
        },
        BG_BASE,
        accent,
        FG_PRIMARY,
        h_ok,
        &mut app.modal_hit_regions,
        HitAction::ModalConfirm,
    );

    let h_cancel = app.is_hovered(btns[3]);
    render_pill_button(
        frame,
        btns[3],
        " ✕ Cancelar (Esc) ",
        if h_cancel { BG_BASE } else { FG_PRIMARY },
        BG_PANEL,
        FG_SECONDARY,
        h_cancel,
        &mut app.modal_hit_regions,
        HitAction::ModalCancel,
    );
}

fn render_confirm_clean_modal(
    frame: &mut Frame,
    area: Rect,
    app: &mut App,
    count: usize,
    total_bytes: u64,
) {
    let modal_rect = centered_rect_fixed(area.width.min(58), 10, area);
    frame.render_widget(Clear, modal_rect);

    let block = Block::default()
        .title(Span::styled(
            " ✦ Confirmar Limpieza Rápida ",
            Style::default()
                .fg(ACCENT_GREEN)
                .add_modifier(Modifier::BOLD),
        ))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(ACCENT_GREEN))
        .style(Style::default().bg(BG_ELEVATED));
    let inner = block.inner(modal_rect);
    frame.render_widget(block, modal_rect);

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(4), Constraint::Length(1), Constraint::Length(1)])
        .split(inner);

    let lines = vec![
        Line::from(""),
        Line::from(vec![
            Span::styled("  Categorías seleccionadas: ", Style::default().fg(FG_SECONDARY)),
            Span::styled(
                format!("{}", count),
                Style::default().fg(FG_PRIMARY).add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(vec![
            Span::styled("  Espacio a liberar:        ", Style::default().fg(FG_SECONDARY)),
            Span::styled(
                format_bytes(total_bytes),
                Style::default()
                    .fg(ACCENT_GREEN)
                    .add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(""),
        Line::from(Span::styled(
            "  ¿Deseas ejecutar la limpieza ahora?",
            Style::default().fg(FG_PRIMARY),
        )),
    ];
    frame.render_widget(Paragraph::new(lines), layout[0]);

    let btns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(3),
            Constraint::Percentage(42),
            Constraint::Min(2),
            Constraint::Percentage(42),
            Constraint::Length(3),
        ])
        .split(layout[1]);

    let h_ok = app.is_hovered(btns[1]);
    render_pill_button(
        frame,
        btns[1],
        " ✦ Sí, limpiar ",
        BG_BASE,
        ACCENT_GREEN,
        ACCENT_CYAN,
        h_ok,
        &mut app.modal_hit_regions,
        HitAction::ModalConfirm,
    );

    let h_cancel = app.is_hovered(btns[3]);
    render_pill_button(
        frame,
        btns[3],
        " ✕ Cancelar ",
        if h_cancel { BG_BASE } else { FG_PRIMARY },
        BG_PANEL,
        FG_SECONDARY,
        h_cancel,
        &mut app.modal_hit_regions,
        HitAction::ModalCancel,
    );
}

fn render_help_modal(frame: &mut Frame, area: Rect, app: &mut App) {
    let modal_rect = centered_rect_fixed(area.width.min(68), 18.min(area.height), area);
    frame.render_widget(Clear, modal_rect);

    let block = Block::default()
        .title(Span::styled(
            " ? Ayuda — GestorDD ",
            Style::default()
                .fg(ACCENT_VIOLET)
                .add_modifier(Modifier::BOLD),
        ))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(ACCENT_VIOLET))
        .style(Style::default().bg(BG_ELEVATED));
    let inner = block.inner(modal_rect);
    frame.render_widget(block, modal_rect);

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(8), Constraint::Length(1)])
        .split(inner);

    let lines = vec![
        Line::from(Span::styled(
            "  CONTROL CON MOUSE (100% clickeable)",
            Style::default()
                .fg(ACCENT_CYAN)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from("  • Clic en pestañas, ruta superior, botones y filtros."),
        Line::from("  • Clic en una fila para ver detalles; doble clic para entrar."),
        Line::from("  • Clic derecho en el Explorador sube a la carpeta padre."),
        Line::from("  • Rueda del mouse desplaza cualquier lista."),
        Line::from(""),
        Line::from(Span::styled(
            "  ATAJOS DE TECLADO",
            Style::default()
                .fg(ACCENT_GREEN)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from("  • 1 / 2 / 3 / Tab   Cambiar entre Explorador, Limpieza y Top Pesados"),
        Line::from("  • ↑ ↓ / j k         Moverse por la lista"),
        Line::from("  • Enter / →         Entrar a carpeta o marcar en Limpieza"),
        Line::from("  • Backspace / ←     Subir a carpeta anterior"),
        Line::from("  • /                 Filtrar por nombre en tiempo real"),
        Line::from("  • d / Shift+D       Enviar a Papelera / Borrar definitivo"),
        Line::from("  • o / s / r / q     Abrir en carpeta / Ordenar / Re-escanear / Salir"),
    ];
    frame.render_widget(
        Paragraph::new(lines)
            .style(Style::default().fg(FG_PRIMARY))
            .wrap(Wrap { trim: false }),
        layout[0],
    );

    let btn_rect = centered_rect_fixed(20, 1, layout[1]);
    let hovered = app.is_hovered(btn_rect);
    render_pill_button(
        frame,
        btn_rect,
        " ✓ Entendido ",
        BG_BASE,
        ACCENT_VIOLET,
        ACCENT_CYAN,
        hovered,
        &mut app.modal_hit_regions,
        HitAction::ModalCancel,
    );
}

fn centered_rect_fixed(width: u16, height: u16, area: Rect) -> Rect {
    let w = width.min(area.width);
    let h = height.min(area.height);
    let x = area.x + (area.width.saturating_sub(w)) / 2;
    let y = area.y + (area.height.saturating_sub(h)) / 2;
    Rect::new(x, y, w, h)
}
