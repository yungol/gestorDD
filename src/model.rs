use std::ffi::CString;
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::time::SystemTime;
use unicode_width::UnicodeWidthChar;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FileCategory {
    Video,
    Archive,
    Image,
    Audio,
    Code,
    Document,
    Other,
}

impl FileCategory {
    pub const ALL: [FileCategory; 7] = [
        FileCategory::Video,
        FileCategory::Archive,
        FileCategory::Image,
        FileCategory::Audio,
        FileCategory::Code,
        FileCategory::Document,
        FileCategory::Other,
    ];

    pub fn from_path(path: &Path) -> Self {
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .map(|s| s.to_ascii_lowercase())
            .unwrap_or_default();

        match ext.as_str() {
            "mp4" | "mkv" | "avi" | "mov" | "wmv" | "flv" | "webm" | "m4v" | "mpeg" | "mpg" => {
                FileCategory::Video
            }
            "zip" | "tar" | "gz" | "tgz" | "bz2" | "xz" | "7z" | "rar" | "iso" | "img" | "deb"
            | "rpm" | "AppImage" | "appimage" | "zst" | "dmg" => FileCategory::Archive,
            "jpg" | "jpeg" | "png" | "gif" | "webp" | "svg" | "bmp" | "tiff" | "ico" | "raw"
            | "heic" | "psd" => FileCategory::Image,
            "mp3" | "wav" | "flac" | "ogg" | "aac" | "m4a" | "wma" | "opus" => FileCategory::Audio,
            "rs" | "py" | "js" | "ts" | "tsx" | "jsx" | "c" | "cpp" | "h" | "hpp" | "go"
            | "java" | "kt" | "rb" | "php" | "html" | "css" | "scss" | "json" | "toml" | "yaml"
            | "yml" | "xml" | "sh" | "bash" | "sql" | "wasm" | "lock" => FileCategory::Code,
            "pdf" | "doc" | "docx" | "xls" | "xlsx" | "ppt" | "pptx" | "odt" | "ods" | "txt"
            | "md" | "csv" | "epub" | "rtf" => FileCategory::Document,
            _ => FileCategory::Other,
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            FileCategory::Video => "Videos",
            FileCategory::Archive => "Comprimidos/ISO",
            FileCategory::Image => "Imágenes",
            FileCategory::Audio => "Audio",
            FileCategory::Code => "Código",
            FileCategory::Document => "Docs",
            FileCategory::Other => "Otros",
        }
    }

    pub fn glyph(&self) -> &'static str {
        match self {
            FileCategory::Video => "▶",
            FileCategory::Archive => "▣",
            FileCategory::Image => "◈",
            FileCategory::Audio => "♪",
            FileCategory::Code => "⌘",
            FileCategory::Document => "≡",
            FileCategory::Other => "•",
        }
    }
}

#[derive(Debug, Clone)]
pub struct FileNode {
    pub name: String,
    pub path: PathBuf,
    pub size: u64,
    pub is_dir: bool,
    pub file_count: u64,
    pub modified: Option<SystemTime>,
    pub category: FileCategory,
    pub children: Vec<FileNode>,
}

impl FileNode {
    pub fn sort_recursive(&mut self) {
        self.children
            .sort_by(|a, b| b.size.cmp(&a.size).then_with(|| a.name.cmp(&b.name)));
        for child in &mut self.children {
            if child.is_dir {
                child.sort_recursive();
            }
        }
    }

    pub fn find_by_path(&self, target: &Path) -> Option<&FileNode> {
        if self.path == target {
            return Some(self);
        }
        if !target.starts_with(&self.path) {
            return None;
        }
        for child in &self.children {
            if target == child.path || (child.is_dir && target.starts_with(&child.path)) {
                if let Some(found) = child.find_by_path(target) {
                    return Some(found);
                }
            }
        }
        None
    }

    /// Removes a node matching `target`, propagating size & file_count reduction upward.
    /// Returns `(freed_bytes, removed_file_count)` if found.
    pub fn remove_by_path(&mut self, target: &Path) -> Option<(u64, u64)> {
        if let Some(pos) = self.children.iter().position(|c| c.path == target) {
            let removed = self.children.remove(pos);
            let removed_files = if removed.is_dir {
                removed.file_count
            } else {
                1
            };
            self.size = self.size.saturating_sub(removed.size);
            self.file_count = self.file_count.saturating_sub(removed_files);
            return Some((removed.size, removed_files));
        }

        for child in &mut self.children {
            if child.is_dir && target.starts_with(&child.path) {
                if let Some((freed, files)) = child.remove_by_path(target) {
                    self.size = self.size.saturating_sub(freed);
                    self.file_count = self.file_count.saturating_sub(files);
                    self.children
                        .sort_by(|a, b| b.size.cmp(&a.size).then_with(|| a.name.cmp(&b.name)));
                    return Some((freed, files));
                }
            }
        }
        None
    }
}

#[derive(Debug, Clone)]
pub struct HeavyFile {
    pub name: String,
    pub path: PathBuf,
    pub parent: PathBuf,
    pub size: u64,
    pub modified: Option<SystemTime>,
    pub category: FileCategory,
}

#[derive(Debug, Clone)]
pub struct DiskStats {
    pub total: u64,
    pub used: u64,
    pub available: u64,
}

impl DiskStats {
    pub fn from_path(path: &Path) -> Option<Self> {
        let c_path = CString::new(path.as_os_str().as_bytes()).ok()?;
        let mut stat: libc::statvfs = unsafe { std::mem::zeroed() };
        let res = unsafe { libc::statvfs(c_path.as_ptr(), &mut stat) };
        if res != 0 {
            return None;
        }
        let block_size = if stat.f_frsize > 0 {
            stat.f_frsize as u64
        } else {
            stat.f_bsize as u64
        };
        let total = (stat.f_blocks as u64).saturating_mul(block_size);
        let free = (stat.f_bfree as u64).saturating_mul(block_size);
        let available = (stat.f_bavail as u64).saturating_mul(block_size);
        let used = total.saturating_sub(free);

        Some(Self {
            total,
            used,
            available,
        })
    }

    pub fn used_ratio(&self) -> f64 {
        if self.total == 0 {
            0.0
        } else {
            (self.used as f64 / self.total as f64).clamp(0.0, 1.0)
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SafetyLevel {
    Safe,
    Review,
    SystemInfo,
}

impl SafetyLevel {
    pub fn label(&self) -> &'static str {
        match self {
            SafetyLevel::Safe => "SEGURA",
            SafetyLevel::Review => "REVISAR",
            SafetyLevel::SystemInfo => "SISTEMA",
        }
    }
}

#[derive(Debug, Clone)]
pub struct CleanTarget {
    pub name: String,
    pub description: String,
    pub paths: Vec<PathBuf>,
    pub size: u64,
    pub item_count: u64,
    pub safety: SafetyLevel,
    pub selected: bool,
    pub delete_contents_only: bool,
    pub use_trash: bool,
    pub command_hint: Option<String>,
}

pub fn format_bytes(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    const GB: f64 = MB * 1024.0;
    const TB: f64 = GB * 1024.0;

    let b = bytes as f64;
    if b >= TB {
        format!("{:.2} TB", b / TB)
    } else if b >= GB {
        format!("{:.2} GB", b / GB)
    } else if b >= MB {
        format!("{:.1} MB", b / MB)
    } else if b >= KB {
        format!("{:.1} KB", b / KB)
    } else {
        format!("{} B", bytes)
    }
}

pub fn format_age(modified: Option<SystemTime>) -> String {
    let Some(mtime) = modified else {
        return "desconocida".to_string();
    };
    let Ok(elapsed) = SystemTime::now().duration_since(mtime) else {
        return "reciente".to_string();
    };
    let secs = elapsed.as_secs();
    if secs < 60 {
        "hace instantes".to_string()
    } else if secs < 3600 {
        format!("hace {} min", secs / 60)
    } else if secs < 86400 {
        format!("hace {} h", secs / 3600)
    } else if secs < 86400 * 30 {
        format!("hace {} días", secs / 86400)
    } else if secs < 86400 * 365 {
        format!("hace {} meses", secs / (86400 * 30))
    } else {
        let years = secs / (86400 * 365);
        if years == 1 {
            "hace 1 año".to_string()
        } else {
            format!("hace {} años", years)
        }
    }
}

pub fn format_count(n: u64) -> String {
    let s = n.to_string();
    let bytes = s.as_bytes();
    let mut out = String::with_capacity(s.len() + s.len() / 3);
    for (i, &b) in bytes.iter().enumerate() {
        if i > 0 && (bytes.len() - i) % 3 == 0 {
            out.push('.');
        }
        out.push(b as char);
    }
    out
}

pub fn truncate_str(s: &str, max_width: usize) -> String {
    if max_width == 0 {
        return String::new();
    }
    let mut width = 0;
    let mut out = String::new();
    for ch in s.chars() {
        let cw = ch.width().unwrap_or(0);
        if width + cw > max_width {
            if max_width >= 1 {
                while width + 1 > max_width && !out.is_empty() {
                    if let Some(popped) = out.pop() {
                        width = width.saturating_sub(popped.width().unwrap_or(0));
                    }
                }
                out.push('…');
            }
            return out;
        }
        out.push(ch);
        width += cw;
    }
    out
}
