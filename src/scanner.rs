use crate::model::{CleanTarget, FileCategory, FileNode, HeavyFile, SafetyLevel};
use rayon::prelude::*;
use std::collections::HashSet;
use std::fs;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

#[derive(Debug)]
pub enum ScanMessage {
    Progress {
        files_scanned: u64,
        bytes_found: u64,
        current_path: String,
    },
    CleanerReady(Vec<CleanTarget>),
    Finished {
        root: FileNode,
        top_files: Vec<HeavyFile>,
        dev_bloat_targets: Vec<CleanTarget>,
        elapsed: Duration,
    },
    Error(String),
}

struct ScanShared {
    files_scanned: AtomicU64,
    bytes_found: AtomicU64,
    cancel: Arc<AtomicBool>,
    seen_inodes: Mutex<HashSet<(u64, u64)>>,
    heavy_files: Mutex<Vec<HeavyFile>>,
    dev_bloat: Mutex<Vec<(PathBuf, u64, String)>>,
    tx: Mutex<Sender<ScanMessage>>,
}

impl ScanShared {
    fn maybe_report(&self, path: &Path) {
        let count = self.files_scanned.load(Ordering::Relaxed);
        if count % 400 == 0 {
            let bytes = self.bytes_found.load(Ordering::Relaxed);
            if let Ok(tx) = self.tx.lock() {
                let _ = tx.send(ScanMessage::Progress {
                    files_scanned: count,
                    bytes_found: bytes,
                    current_path: path.display().to_string(),
                });
            }
        }
    }

    fn record_heavy_file(&self, file: HeavyFile) {
        if file.size < 1024 * 1024 {
            // Keep smaller files only if we have very few
            if let Ok(mut list) = self.heavy_files.lock() {
                if list.len() < 80 {
                    list.push(file);
                }
            }
            return;
        }
        if let Ok(mut list) = self.heavy_files.lock() {
            list.push(file);
            if list.len() > 260 {
                list.sort_by(|a, b| b.size.cmp(&a.size));
                list.truncate(120);
            }
        }
    }

    fn record_dev_bloat(&self, path: PathBuf, size: u64, kind: &str) {
        if size < 15 * 1024 * 1024 {
            return;
        }
        if let Ok(mut list) = self.dev_bloat.lock() {
            list.push((path, size, kind.to_string()));
        }
    }
}

fn should_skip_path(path: &Path) -> bool {
    let s = path.to_string_lossy();
    matches!(
        s.as_ref(),
        "/proc" | "/sys" | "/dev" | "/run" | "/var/run" | "/sys/fs/cgroup"
    )
}

pub fn spawn_scan(root_path: PathBuf, tx: Sender<ScanMessage>, cancel: Arc<AtomicBool>) {
    // Run cleaner target discovery in its own thread so it never blocks directory scanning
    let tx_clean = tx.clone();
    let cancel_clean = cancel.clone();
    std::thread::spawn(move || {
        if cancel_clean.load(Ordering::Relaxed) {
            return;
        }
        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/"));
        let base_targets = scan_base_clean_targets(&home);
        if !cancel_clean.load(Ordering::Relaxed) {
            let _ = tx_clean.send(ScanMessage::CleanerReady(base_targets));
        }
    });

    std::thread::spawn(move || {
        let start = std::time::Instant::now();

        let canonical = fs::canonicalize(&root_path).unwrap_or(root_path);
        if !canonical.exists() {
            let _ = tx.send(ScanMessage::Error(format!(
                "La ruta no existe: {}",
                canonical.display()
            )));
            return;
        }

        let shared = Arc::new(ScanShared {
            files_scanned: AtomicU64::new(0),
            bytes_found: AtomicU64::new(0),
            cancel: cancel.clone(),
            seen_inodes: Mutex::new(HashSet::new()),
            heavy_files: Mutex::new(Vec::new()),
            dev_bloat: Mutex::new(Vec::new()),
            tx: Mutex::new(tx.clone()),
        });

        let mut root = scan_dir_node(&canonical, 0, &shared);

        if cancel.load(Ordering::Relaxed) {
            return;
        }

        root.sort_recursive();

        let mut top_files = shared
            .heavy_files
            .lock()
            .map(|v| v.clone())
            .unwrap_or_default();
        top_files.sort_by(|a, b| b.size.cmp(&a.size));
        top_files.truncate(100);

        let mut dev_bloat = shared
            .dev_bloat
            .lock()
            .map(|v| v.clone())
            .unwrap_or_default();
        dev_bloat.sort_by(|a, b| b.1.cmp(&a.1));

        let dev_bloat_targets = build_dev_bloat_targets(&dev_bloat);

        let _ = tx.send(ScanMessage::Finished {
            root,
            top_files,
            dev_bloat_targets,
            elapsed: start.elapsed(),
        });
    });
}

fn scan_dir_node(path: &Path, depth: usize, shared: &Arc<ScanShared>) -> FileNode {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string());

    let modified = fs::symlink_metadata(path)
        .ok()
        .and_then(|m| m.modified().ok());

    if shared.cancel.load(Ordering::Relaxed) || should_skip_path(path) {
        return FileNode {
            name,
            path: path.to_path_buf(),
            size: 0,
            is_dir: true,
            file_count: 0,
            modified,
            category: FileCategory::Other,
            children: Vec::new(),
        };
    }

    let entries: Vec<PathBuf> = match fs::read_dir(path) {
        Ok(rd) => rd.filter_map(|e| e.ok().map(|e| e.path())).collect(),
        Err(_) => {
            return FileNode {
                name,
                path: path.to_path_buf(),
                size: 0,
                is_dir: true,
                file_count: 0,
                modified,
                category: FileCategory::Other,
                children: Vec::new(),
            };
        }
    };

    let process_entry = |entry_path: PathBuf| -> Option<FileNode> {
        if shared.cancel.load(Ordering::Relaxed) || should_skip_path(&entry_path) {
            return None;
        }

        let meta = fs::symlink_metadata(&entry_path).ok()?;
        let ft = meta.file_type();

        if ft.is_symlink() {
            return None;
        }

        if ft.is_dir() {
            let child = scan_dir_node(&entry_path, depth + 1, shared);
            // Detect heavy dev build/dependency folders
            if child.name == "node_modules" && child.size >= 20 * 1024 * 1024 {
                // Only report top-level node_modules (not nested inside another node_modules)
                let is_nested = entry_path
                    .parent()
                    .map(|p| p.to_string_lossy().contains("node_modules"))
                    .unwrap_or(false);
                if !is_nested {
                    shared.record_dev_bloat(entry_path.clone(), child.size, "node_modules");
                }
            } else if child.name == "target" && child.size >= 50 * 1024 * 1024 {
                let has_cargo = entry_path
                    .parent()
                    .map(|p| p.join("Cargo.toml").exists())
                    .unwrap_or(false);
                if has_cargo {
                    shared.record_dev_bloat(entry_path.clone(), child.size, "Rust target");
                }
            }
            Some(child)
        } else if ft.is_file() {
            // Avoid double-counting hard links
            if meta.nlink() > 1 {
                let key = (meta.dev(), meta.ino());
                if let Ok(mut set) = shared.seen_inodes.lock() {
                    if !set.insert(key) {
                        return None;
                    }
                }
            }

            // Use allocated disk blocks (st_blocks * 512) if reasonable, fallback to len()
            let disk_bytes = meta.blocks().saturating_mul(512);
            let size = if disk_bytes > 0 && disk_bytes <= meta.len().saturating_add(4096 * 4) {
                disk_bytes
            } else {
                meta.len()
            };

            let file_name = entry_path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            let category = FileCategory::from_path(&entry_path);
            let mtime = meta.modified().ok();

            shared.files_scanned.fetch_add(1, Ordering::Relaxed);
            shared.bytes_found.fetch_add(size, Ordering::Relaxed);
            shared.maybe_report(&entry_path);

            shared.record_heavy_file(HeavyFile {
                name: file_name.clone(),
                path: entry_path.clone(),
                parent: path.to_path_buf(),
                size,
                modified: mtime,
                category,
            });

            Some(FileNode {
                name: file_name,
                path: entry_path,
                size,
                is_dir: false,
                file_count: 1,
                modified: mtime,
                category,
                children: Vec::new(),
            })
        } else {
            None
        }
    };

    let children: Vec<FileNode> = if depth <= 2 && entries.len() > 2 {
        entries.into_par_iter().filter_map(process_entry).collect()
    } else {
        entries.into_iter().filter_map(process_entry).collect()
    };

    let mut total_size = 0u64;
    let mut total_files = 0u64;
    for c in &children {
        total_size = total_size.saturating_add(c.size);
        total_files = total_files.saturating_add(c.file_count);
    }

    FileNode {
        name,
        path: path.to_path_buf(),
        size: total_size,
        is_dir: true,
        file_count: total_files,
        modified,
        category: FileCategory::Other,
        children,
    }
}

fn dir_size_quick(path: &Path) -> (u64, u64) {
    if !path.exists() {
        return (0, 0);
    }
    let meta = match fs::symlink_metadata(path) {
        Ok(m) => m,
        Err(_) => return (0, 0),
    };
    if meta.file_type().is_symlink() {
        return (0, 0);
    }
    if meta.is_file() {
        return (meta.len(), 1);
    }
    let mut size = 0u64;
    let mut count = 0u64;
    let mut stack = vec![path.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(rd) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in rd.flatten() {
            let p = entry.path();
            let Ok(m) = fs::symlink_metadata(&p) else {
                continue;
            };
            if m.file_type().is_symlink() {
                continue;
            }
            if m.is_dir() {
                stack.push(p);
            } else if m.is_file() {
                size = size.saturating_add(m.len());
                count = count.saturating_add(1);
            }
        }
    }
    (size, count)
}

pub fn build_dev_bloat_targets(dev_bloat: &[(PathBuf, u64, String)]) -> Vec<CleanTarget> {
    dev_bloat
        .iter()
        .take(6)
        .map(|(path, size, kind)| {
            let short_parent = path
                .parent()
                .and_then(|p| p.file_name())
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| path.display().to_string());
            CleanTarget {
                name: format!("{} ({})", kind, short_parent),
                description: format!("Carpeta regenerable: {}", path.display()),
                paths: vec![path.clone()],
                size: *size,
                item_count: 1,
                safety: SafetyLevel::Review,
                selected: false,
                delete_contents_only: false,
                use_trash: false,
                command_hint: None,
            }
        })
        .collect()
}

pub fn sort_clean_targets(targets: &mut [CleanTarget]) {
    targets.sort_by(|a, b| {
        let rank = |s: SafetyLevel| match s {
            SafetyLevel::Safe => 0,
            SafetyLevel::Review => 1,
            SafetyLevel::SystemInfo => 2,
        };
        rank(a.safety)
            .cmp(&rank(b.safety))
            .then_with(|| b.size.cmp(&a.size))
    });
}

pub fn scan_base_clean_targets(home: &Path) -> Vec<CleanTarget> {
    let mut targets = Vec::new();

    // 1. Papelera de reciclaje
    let trash_files = home.join(".local/share/Trash/files");
    let trash_info = home.join(".local/share/Trash/info");
    let (trash_size, trash_count) = dir_size_quick(&trash_files);
    if trash_size > 0 || trash_count > 0 {
        targets.push(CleanTarget {
            name: "Papelera de reciclaje".into(),
            description: "Archivos ya eliminados esperando vaciado definitivo.".into(),
            paths: vec![trash_files, trash_info],
            size: trash_size,
            item_count: trash_count,
            safety: SafetyLevel::Safe,
            selected: true,
            delete_contents_only: true,
            use_trash: false,
            command_hint: None,
        });
    }

    // 2. Miniaturas (thumbnails)
    let thumbs = home.join(".cache/thumbnails");
    let (thumb_size, thumb_count) = dir_size_quick(&thumbs);
    if thumb_size > 0 {
        targets.push(CleanTarget {
            name: "Miniaturas de imágenes".into(),
            description: "Vistas previas en ~/.cache/thumbnails (se regeneran solas).".into(),
            paths: vec![thumbs],
            size: thumb_size,
            item_count: thumb_count,
            safety: SafetyLevel::Safe,
            selected: true,
            delete_contents_only: true,
            use_trash: false,
            command_hint: None,
        });
    }

    // 3. Caché de navegadores
    let browser_candidates = [
        home.join(".cache/mozilla/firefox"),
        home.join(".cache/google-chrome"),
        home.join(".cache/chromium"),
        home.join(".cache/BraveSoftware"),
        home.join(".cache/microsoft-edge"),
        home.join(".cache/vivaldi"),
    ];
    let mut browser_paths = Vec::new();
    let mut browser_size = 0u64;
    let mut browser_count = 0u64;
    for p in browser_candidates {
        if p.exists() {
            let (s, c) = dir_size_quick(&p);
            if s > 0 {
                browser_size += s;
                browser_count += c;
                browser_paths.push(p);
            }
        }
    }
    if browser_size > 0 {
        targets.push(CleanTarget {
            name: "Caché de navegadores".into(),
            description: "Temporales web (Firefox/Chrome/Brave). No borra claves ni historial."
                .into(),
            paths: browser_paths,
            size: browser_size,
            item_count: browser_count,
            safety: SafetyLevel::Safe,
            selected: true,
            delete_contents_only: true,
            use_trash: false,
            command_hint: None,
        });
    }

    // 4. Caché de Python (pip / uv)
    let py_candidates = [home.join(".cache/pip"), home.join(".cache/uv")];
    let mut py_paths = Vec::new();
    let mut py_size = 0u64;
    let mut py_count = 0u64;
    for p in py_candidates {
        if p.exists() {
            let (s, c) = dir_size_quick(&p);
            if s > 0 {
                py_size += s;
                py_count += c;
                py_paths.push(p);
            }
        }
    }
    if py_size > 0 {
        targets.push(CleanTarget {
            name: "Caché de Python (pip / uv)".into(),
            description: "Paquetes descargados en caché. Se vuelven a bajar si hace falta.".into(),
            paths: py_paths,
            size: py_size,
            item_count: py_count,
            safety: SafetyLevel::Safe,
            selected: true,
            delete_contents_only: true,
            use_trash: false,
            command_hint: None,
        });
    }

    // 5. Caché de Node (npm / yarn / pnpm)
    let node_candidates = [
        home.join(".npm/_cacache"),
        home.join(".cache/yarn"),
        home.join(".cache/pnpm"),
        home.join(".cache/node-gyp"),
    ];
    let mut node_paths = Vec::new();
    let mut node_size = 0u64;
    let mut node_count = 0u64;
    for p in node_candidates {
        if p.exists() {
            let (s, c) = dir_size_quick(&p);
            if s > 0 {
                node_size += s;
                node_count += c;
                node_paths.push(p);
            }
        }
    }
    if node_size > 0 {
        targets.push(CleanTarget {
            name: "Caché de Node (npm / yarn / pnpm)".into(),
            description: "Descargas cacheadas de paquetes JS/TS.".into(),
            paths: node_paths,
            size: node_size,
            item_count: node_count,
            safety: SafetyLevel::Safe,
            selected: true,
            delete_contents_only: true,
            use_trash: false,
            command_hint: None,
        });
    }

    // 6. Caché de Rust (cargo)
    let cargo_cache = home.join(".cargo/registry/cache");
    let (cargo_size, cargo_count) = dir_size_quick(&cargo_cache);
    if cargo_size > 0 {
        targets.push(CleanTarget {
            name: "Caché de crates Rust (Cargo)".into(),
            description: "Archivos .crate descargados en ~/.cargo/registry/cache.".into(),
            paths: vec![cargo_cache],
            size: cargo_size,
            item_count: cargo_count,
            safety: SafetyLevel::Safe,
            selected: false,
            delete_contents_only: true,
            use_trash: false,
            command_hint: None,
        });
    }

    // 7. Descargas antiguas (> 30 días)
    let dl_dirs = [home.join("Descargas"), home.join("Downloads")];
    let cutoff = SystemTime::now()
        .checked_sub(Duration::from_secs(30 * 24 * 3600))
        .unwrap_or(SystemTime::UNIX_EPOCH);
    let mut old_dl_paths = Vec::new();
    let mut old_dl_size = 0u64;
    let mut old_dl_count = 0u64;
    for d in dl_dirs {
        if let Ok(rd) = fs::read_dir(&d) {
            for entry in rd.flatten() {
                let p = entry.path();
                if let Ok(m) = fs::symlink_metadata(&p) {
                    if let Ok(mtime) = m.modified() {
                        if mtime < cutoff {
                            let (s, c) = dir_size_quick(&p);
                            old_dl_size += s;
                            old_dl_count += c.max(1);
                            old_dl_paths.push(p);
                        }
                    }
                }
            }
        }
    }
    if old_dl_size > 0 {
        targets.push(CleanTarget {
            name: "Descargas antiguas (> 30 días)".into(),
            description: "Archivos viejos en Descargas (se envían a la Papelera).".into(),
            paths: old_dl_paths,
            size: old_dl_size,
            item_count: old_dl_count,
            safety: SafetyLevel::Review,
            selected: false,
            delete_contents_only: false,
            use_trash: true,
            command_hint: None,
        });
    }

    // 8. System info: APT cache
    let apt_cache = PathBuf::from("/var/cache/apt/archives");
    let (apt_size, apt_count) = dir_size_quick(&apt_cache);
    if apt_size > 5 * 1024 * 1024 {
        targets.push(CleanTarget {
            name: "Caché de paquetes APT (sistema)".into(),
            description: "Requiere permisos admin. Ejecuta: sudo apt clean".into(),
            paths: vec![apt_cache],
            size: apt_size,
            item_count: apt_count,
            safety: SafetyLevel::SystemInfo,
            selected: false,
            delete_contents_only: false,
            use_trash: false,
            command_hint: Some("sudo apt clean && sudo apt autoremove".into()),
        });
    }

    // 9. System info: Systemd journal logs
    let journal = PathBuf::from("/var/log/journal");
    let (j_size, j_count) = dir_size_quick(&journal);
    if j_size > 20 * 1024 * 1024 {
        targets.push(CleanTarget {
            name: "Registros systemd journal (sistema)".into(),
            description: "Requiere admin. Ejecuta: sudo journalctl --vacuum-time=7d".into(),
            paths: vec![journal],
            size: j_size,
            item_count: j_count,
            safety: SafetyLevel::SystemInfo,
            selected: false,
            delete_contents_only: false,
            use_trash: false,
            command_hint: Some("sudo journalctl --vacuum-time=7d".into()),
        });
    }

    sort_clean_targets(&mut targets);
    targets
}

pub fn delete_path(path: &Path, permanent: bool) -> Result<(), String> {
    if !path.exists() && fs::symlink_metadata(path).is_err() {
        return Err("El archivo o carpeta ya no existe.".into());
    }
    if permanent {
        let meta = fs::symlink_metadata(path).map_err(|e| format!("Error al leer: {}", e))?;
        if meta.is_dir() {
            fs::remove_dir_all(path).map_err(|e| format!("No se pudo borrar: {}", e))
        } else {
            fs::remove_file(path).map_err(|e| format!("No se pudo borrar: {}", e))
        }
    } else {
        trash::delete(path).map_err(|e| format!("No se pudo mover a la papelera: {}", e))
    }
}

pub fn execute_clean_target(target: &CleanTarget) -> Result<u64, String> {
    if target.safety == SafetyLevel::SystemInfo {
        return Ok(0);
    }
    let mut freed = 0u64;
    let mut last_err: Option<String> = None;

    for path in &target.paths {
        if !path.exists() {
            continue;
        }
        if target.delete_contents_only && path.is_dir() {
            let Ok(rd) = fs::read_dir(path) else {
                continue;
            };
            for entry in rd.flatten() {
                let p = entry.path();
                let (sz, _) = dir_size_quick(&p);
                let res = if target.use_trash {
                    trash::delete(&p).map_err(|e| e.to_string())
                } else if p.is_dir() {
                    fs::remove_dir_all(&p).map_err(|e| e.to_string())
                } else {
                    fs::remove_file(&p).map_err(|e| e.to_string())
                };
                match res {
                    Ok(()) => freed = freed.saturating_add(sz),
                    Err(e) => last_err = Some(e),
                }
            }
        } else {
            let (sz, _) = dir_size_quick(path);
            let res = if target.use_trash {
                trash::delete(path).map_err(|e| e.to_string())
            } else if path.is_dir() {
                fs::remove_dir_all(path).map_err(|e| e.to_string())
            } else {
                fs::remove_file(path).map_err(|e| e.to_string())
            };
            match res {
                Ok(()) => freed = freed.saturating_add(sz),
                Err(e) => last_err = Some(e),
            }
        }
    }

    if freed == 0 {
        if let Some(err) = last_err {
            return Err(err);
        }
    }
    Ok(freed)
}
