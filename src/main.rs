mod app;
mod model;
mod scanner;
mod ui;

use app::App;
use crossterm::event::{
    self, DisableMouseCapture, EnableMouseCapture, Event, KeyEventKind,
};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use std::io;
use std::path::PathBuf;
use std::time::{Duration, Instant};

fn parse_target_path() -> Result<Option<PathBuf>, String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if let Some(first) = args.first() {
        match first.as_str() {
            "-h" | "--help" => {
                println!("⚡ GestorDD — Gestor interactivo de espacio en disco (TUI con mouse)");
                println!();
                println!("Uso:");
                println!("  gestordd [ruta]");
                println!();
                println!("Ejemplos:");
                println!("  gestordd             Analiza tu carpeta personal ($HOME)");
                println!("  gestordd .           Analiza la carpeta actual");
                println!("  gestordd /           Analiza todo el disco raíz");
                return Ok(None);
            }
            "-v" | "--version" => {
                println!("gestordd {}", env!("CARGO_PKG_VERSION"));
                return Ok(None);
            }
            path_arg => {
                let p = PathBuf::from(path_arg);
                if !p.exists() {
                    return Err(format!("La ruta '{}' no existe.", path_arg));
                }
                return Ok(Some(p));
            }
        }
    }

    Ok(Some(
        dirs::home_dir().unwrap_or_else(|| PathBuf::from(".")),
    ))
}

fn restore_terminal() {
    let _ = disable_raw_mode();
    let _ = execute!(io::stdout(), LeaveAlternateScreen, DisableMouseCapture);
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let target_path = match parse_target_path() {
        Ok(Some(p)) => p,
        Ok(None) => return Ok(()),
        Err(msg) => {
            eprintln!("Error: {}", msg);
            std::process::exit(1);
        }
    };

    // Ensure terminal is cleanly restored even if a panic occurs
    let original_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        restore_terminal();
        original_hook(info);
    }));

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    terminal.clear()?;

    let mut app = App::new(target_path);
    let tick_rate = Duration::from_millis(80);
    let mut last_tick = Instant::now();

    let run_res: io::Result<()> = (|| {
        loop {
            terminal.draw(|frame| ui::render(frame, &mut app))?;

            let timeout = tick_rate.saturating_sub(last_tick.elapsed());
            if event::poll(timeout)? {
                match event::read()? {
                    Event::Key(key) if key.kind == KeyEventKind::Press => {
                        app.handle_key(key);
                    }
                    Event::Mouse(mouse) => {
                        app.handle_mouse(mouse);
                    }
                    Event::Resize(_, _) => {}
                    _ => {}
                }
            }

            if last_tick.elapsed() >= tick_rate {
                app.on_tick();
                last_tick = Instant::now();
            }

            if app.should_quit {
                break;
            }
        }
        Ok(())
    })();

    restore_terminal();
    terminal.show_cursor()?;

    if let Err(err) = run_res {
        eprintln!("Error en la interfaz: {}", err);
        std::process::exit(1);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::ActiveTab;
    use crate::model::{format_bytes, truncate_str, FileCategory, FileNode};
    use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
    use ratatui::backend::TestBackend;
    use std::fs;

    #[test]
    fn test_format_bytes_and_truncate() {
        assert_eq!(format_bytes(512), "512 B");
        assert_eq!(format_bytes(2048), "2.0 KB");
        assert_eq!(format_bytes(5 * 1024 * 1024), "5.0 MB");
        assert_eq!(format_bytes(3 * 1024 * 1024 * 1024), "3.00 GB");
        assert_eq!(truncate_str("hola mundo", 6), "hola …");
        assert_eq!(truncate_str("corto", 10), "corto");
    }

    #[test]
    fn test_tree_removal_updates_parent_sizes() {
        let mut root = FileNode {
            name: "root".into(),
            path: PathBuf::from("/tmp/root"),
            size: 300,
            is_dir: true,
            file_count: 2,
            modified: None,
            category: FileCategory::Other,
            children: vec![
                FileNode {
                    name: "sub".into(),
                    path: PathBuf::from("/tmp/root/sub"),
                    size: 200,
                    is_dir: true,
                    file_count: 1,
                    modified: None,
                    category: FileCategory::Other,
                    children: vec![FileNode {
                        name: "big.bin".into(),
                        path: PathBuf::from("/tmp/root/sub/big.bin"),
                        size: 200,
                        is_dir: false,
                        file_count: 1,
                        modified: None,
                        category: FileCategory::Other,
                        children: vec![],
                    }],
                },
                FileNode {
                    name: "small.txt".into(),
                    path: PathBuf::from("/tmp/root/small.txt"),
                    size: 100,
                    is_dir: false,
                    file_count: 1,
                    modified: None,
                    category: FileCategory::Document,
                    children: vec![],
                },
            ],
        };

        let removed = root.remove_by_path(&PathBuf::from("/tmp/root/sub/big.bin"));
        assert_eq!(removed, Some((200, 1)));
        assert_eq!(root.size, 100);
        assert_eq!(root.file_count, 1);
    }

    #[test]
    fn test_tui_render_and_mouse_tab_clicks() {
        let tmp = std::env::temp_dir().join("gestordd_test_dir");
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(tmp.join("videos")).unwrap();
        fs::write(tmp.join("videos/clip.mp4"), vec![0u8; 4096]).unwrap();
        fs::write(tmp.join("notas.txt"), b"hola").unwrap();

        let mut app = App::new(tmp.clone());
        // Wait briefly for background scan of tiny temp folder to complete
        for _ in 0..40 {
            app.on_tick();
            if !app.is_scanning && app.root_node.is_some() {
                break;
            }
            std::thread::sleep(Duration::from_millis(25));
        }
        assert!(app.root_node.is_some());

        let backend = TestBackend::new(120, 36);
        let mut terminal = Terminal::new(backend).unwrap();

        // Render Explorer tab
        terminal.draw(|f| ui::render(f, &mut app)).unwrap();
        assert!(!app.hit_regions.is_empty());

        // Find the Cleaner tab hit region and click it with mouse!
        let cleaner_rect = app
            .hit_regions
            .iter()
            .find(|r| r.action == crate::app::HitAction::Tab(ActiveTab::Cleaner))
            .expect("Cleaner tab should be clickable")
            .rect;

        app.handle_mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: cleaner_rect.x + 1,
            row: cleaner_rect.y,
            modifiers: KeyModifiers::NONE,
        });
        assert_eq!(app.active_tab, ActiveTab::Cleaner);

        // Render Cleaner tab & click TopFiles tab
        terminal.draw(|f| ui::render(f, &mut app)).unwrap();
        let top_rect = app
            .hit_regions
            .iter()
            .find(|r| r.action == crate::app::HitAction::Tab(ActiveTab::TopFiles))
            .expect("TopFiles tab should be clickable")
            .rect;
        app.handle_mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: top_rect.x + 1,
            row: top_rect.y,
            modifiers: KeyModifiers::NONE,
        });
        assert_eq!(app.active_tab, ActiveTab::TopFiles);

        // Render TopFiles & open Help modal via mouse click
        terminal.draw(|f| ui::render(f, &mut app)).unwrap();
        let help_rect = app
            .hit_regions
            .iter()
            .find(|r| r.action == crate::app::HitAction::ToggleHelp)
            .expect("Help button should be clickable")
            .rect;
        app.handle_mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: help_rect.x + 1,
            row: help_rect.y,
            modifiers: KeyModifiers::NONE,
        });
        assert!(matches!(app.modal, crate::app::ModalState::Help));
        terminal.draw(|f| ui::render(f, &mut app)).unwrap();

        // Clean up temp folder
        let _ = fs::remove_dir_all(&tmp);
    }
}
