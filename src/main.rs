use std::collections::HashSet;
use std::env;
use std::fs::{self, File};
use std::io::{self, Cursor, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};
use std::time::{Duration, SystemTime};

use image::ImageFormat;
use image::imageops::FilterType;
use serde::Deserialize;

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

#[derive(Debug)]
struct Args {
    #[allow(dead_code)] // Keep for xclip compatibility
    selection: String,
    mime_type: Option<String>,
    mode_output: bool,
    input_file: Option<String>,
}

fn parse_args() -> Args {
    let mut selection = String::from("clipboard");
    let mut mime_type: Option<String> = None;
    let mut mode_output = false;
    let mut input_file: Option<String> = None;

    let mut it = env::args().skip(1).peekable();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "-selection" => {
                if let Some(val) = it.next() {
                    selection = val;
                }
            }
            "-t" => {
                if let Some(val) = it.next() {
                    mime_type = Some(val);
                }
            }
            "-o" => {
                mode_output = true;
            }
            "-i" => {
                // optional filename after -i if next isn't a flag
                if let Some(peek) = it.peek()
                    && !peek.starts_with('-')
                {
                    input_file = it.next();
                }
            }
            _ => {
                // ignore other args; compatibility shim
            }
        }
    }

    Args {
        selection,
        mime_type,
        mode_output,
        input_file,
    }
}

fn get_storage_directory() -> PathBuf {
    // For WSL, prefer ~/.cache as it's more reliable and predictable
    // WSL's /run/user/ isn't always tmpfs and may not exist

    // First try XDG_CACHE_HOME if set
    if let Ok(xdg_cache) = env::var("XDG_CACHE_HOME")
        && !xdg_cache.trim().is_empty()
    {
        return PathBuf::from(xdg_cache).join("wsl-clip-bridge");
    }

    // Use ~/.cache (most reliable for WSL)
    if let Ok(home) = env::var("HOME") {
        return PathBuf::from(home).join(".cache").join("wsl-clip-bridge");
    }

    // Fall back to /tmp with UID for isolation
    let uid = env::var("UID").unwrap_or_else(|_| "unknown".to_string());
    PathBuf::from(format!("/tmp/wsl-clip-bridge-{uid}"))
}

fn get_image_path() -> PathBuf {
    get_storage_directory().join("image.bin")
}

fn get_image_format_path() -> PathBuf {
    get_storage_directory().join("image.format")
}

fn get_text_path() -> PathBuf {
    get_storage_directory().join("text.txt")
}

fn ensure_storage_directory() -> io::Result<()> {
    let dir = get_storage_directory();
    if !dir.exists() {
        fs::create_dir_all(&dir)?;
        // restrict perms to user on unix
        #[cfg(unix)]
        {
            let _ = fs::set_permissions(&dir, fs::Permissions::from_mode(0o700));
        }
    }
    Ok(())
}

fn is_file_non_empty(path: &Path) -> bool {
    fs::metadata(path).is_ok_and(|m| m.is_file() && m.len() > 0)
}

fn print_targets() {
    let ttl = load_ttl();
    let mut printed = HashSet::new();

    // Check file-based targets (existing logic)
    let image_path = get_image_path();
    if is_file_fresh(&image_path, ttl) {
        if let Ok(format) = fs::read_to_string(get_image_format_path()) {
            let format = format.trim();
            println!("{format}");
            printed.insert(format.to_string());
            // Also output jpg alias for jpeg
            if format == "image/jpeg" {
                println!("image/jpg");
                printed.insert("image/jpg".to_string());
            }
        }
    } else if image_path.exists() {
        // Clean up expired image files
        let _ = fs::remove_file(&image_path);
        let _ = fs::remove_file(get_image_format_path());
    }

    // Add wl-clipboard targets
    if wl_clipboard_available()
        && let Ok(types) = get_wl_clipboard_types()
    {
        for typ in types {
            match typ.as_str() {
                "image/bmp" => {
                    // Only advertise PNG conversion for BMP
                    if !printed.contains("image/png") {
                        println!("image/png");
                        printed.insert("image/png".to_string());
                    }
                }
                "image/png" | "image/jpeg" | "image/gif" | "image/webp" => {
                    if !printed.contains(&typ) {
                        println!("{typ}");
                        printed.insert(typ.clone());
                        if typ == "image/jpeg" && !printed.contains("image/jpg") {
                            println!("image/jpg");
                            printed.insert("image/jpg".to_string());
                        }
                    }
                }
                t if t.starts_with("text/") => {
                    if printed.insert(typ.clone()) {
                        println!("{typ}");
                    }
                }
                _ => {}
            }
        }
    }

    // Text targets (existing logic)
    let text_path = get_text_path();
    if is_file_fresh(&text_path, ttl) && !printed.contains("text/plain;charset=utf-8") {
        println!("text/plain;charset=utf-8");
        println!("STRING");
    } else if text_path.exists() && !is_file_fresh(&text_path, ttl) {
        // Clean up expired text file
        let _ = fs::remove_file(&text_path);
    }
}

fn output_type(mime: &str) -> io::Result<i32> {
    match mime {
        m if m.starts_with("text/plain") => {
            let text_path = get_text_path();
            let ttl = load_ttl();
            if is_file_fresh(&text_path, ttl) {
                let mut file = File::open(text_path)?;
                let mut buffer = Vec::new();
                file.read_to_end(&mut buffer)?;
                io::stdout().write_all(&buffer)?;
                return Ok(0);
            }

            // Clean up expired file
            if text_path.exists() {
                let _ = fs::remove_file(&text_path);
            }

            // Try wl-clipboard if available
            if wl_clipboard_available()
                && let Ok(types) = get_wl_clipboard_types()
                && types.iter().any(|t| t.starts_with("text/"))
                && let Ok(data) = fetch_from_wl_clipboard("text/plain")
            {
                io::stdout().write_all(&data)?;
                return Ok(0);
            }
            Ok(1)
        }
        "image/png" | "image/jpeg" | "image/jpg" | "image/gif" | "image/webp" => {
            let image_path = get_image_path();
            let ttl = load_ttl();

            // Priority 1: Check wl-clipboard first (always has the latest)
            if wl_clipboard_available()
                && let Ok(types) = get_wl_clipboard_types()
            {
                // Direct format available?
                if types.contains(&mime.to_string())
                    && let Ok(data) = fetch_from_wl_clipboard(mime)
                {
                    // Apply downscaling if configured
                    let config = load_config();
                    let max_dim = config.and_then(|c| c.max_image_dimension);
                    let processed = downscale_image_if_needed(&data, mime, max_dim);

                    io::stdout().write_all(&processed)?;
                    return Ok(0);
                }

                // Special case: BMP → PNG conversion ONLY
                if mime == "image/png"
                    && types.contains(&"image/bmp".to_string())
                    && let Ok(bmp_data) = fetch_from_wl_clipboard("image/bmp")
                {
                    // Convert BMP to PNG
                    let img = image::load_from_memory(&bmp_data)
                        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

                    let mut output = Cursor::new(Vec::new());
                    img.write_to(&mut output, ImageFormat::Png)
                        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
                    let png_data = output.into_inner();

                    // Apply downscaling
                    let config = load_config();
                    let max_dim = config.as_ref().and_then(|c| c.max_image_dimension);
                    let processed = downscale_image_if_needed(&png_data, "image/png", max_dim);

                    // Cache if configured
                    if config
                        .as_ref()
                        .is_none_or(|c| c.cache_wl_images.unwrap_or(true))
                    {
                        ensure_storage_directory()?;
                        fs::write(&image_path, &processed)?;
                        fs::write(get_image_format_path(), "image/png")?;
                        #[cfg(unix)]
                        {
                            let _ =
                                fs::set_permissions(&image_path, fs::Permissions::from_mode(0o600));
                        }
                    }

                    io::stdout().write_all(&processed)?;
                    return Ok(0);
                }
            }

            // Priority 2: Fall back to cached file if still fresh
            if is_file_fresh(&image_path, ttl)
                && let Ok(stored_format) = fs::read_to_string(get_image_format_path())
            {
                let stored_format = stored_format.trim();
                let matches =
                    mime == stored_format || (mime == "image/jpg" && stored_format == "image/jpeg");
                if matches {
                    let mut file = File::open(&image_path)?;
                    let mut buffer = Vec::new();
                    file.read_to_end(&mut buffer)?;
                    io::stdout().write_all(&buffer)?;
                    return Ok(0);
                }
            }

            // Clean up expired files
            if image_path.exists() && !is_file_fresh(&image_path, ttl) {
                let _ = fs::remove_file(&image_path);
                let _ = fs::remove_file(get_image_format_path());
            }

            Ok(1)
        }
        _ => Ok(1),
    }
}

fn validate_file_access(path: &Path) -> io::Result<()> {
    // Check file size limit
    let config = load_config();
    if let Some(cfg) = config {
        // Check file size
        if let Some(max_mb) = cfg.max_file_size_mb
            && max_mb > 0
            && let Ok(metadata) = fs::metadata(path)
        {
            let max_bytes = max_mb * 1024 * 1024;
            if metadata.len() > max_bytes {
                eprintln!("Error: File exceeds maximum size of {max_mb}MB");
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "File too large",
                ));
            }
        }

        // Security: Check allowed directories if configured
        // If allowed_directories is set, only those paths are permitted (recursively)
        // If not set or empty, all paths are allowed
        if let Some(allowed_dirs) = &cfg.allowed_directories
            && !allowed_dirs.is_empty()
        {
            let canonical_path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
            let is_allowed = allowed_dirs
                .iter()
                .any(|dir| canonical_path.starts_with(PathBuf::from(dir)));

            if !is_allowed {
                eprintln!(
                    "Error: Access denied - file '{}' is not in allowed directories: {:?}",
                    canonical_path.display(),
                    allowed_dirs
                );
                return Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "File not in allowed directories",
                ));
            }
        }
        // No restrictions if allowed_directories is not configured
    }
    Ok(())
}

#[allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]
fn downscale_image_if_needed(data: &[u8], mime: &str, max_dim: Option<u32>) -> Vec<u8> {
    // If no max dimension configured, return original
    let max_dim = match max_dim {
        Some(d) if d > 0 => d,
        _ => return data.to_vec(),
    };

    // Try to load the image
    let Ok(img) = image::load_from_memory(data) else {
        return data.to_vec(); // If can't load, return original
    };

    let (width, height) = (img.width(), img.height());
    let max_current = width.max(height);

    // Only downscale if exceeds max dimension
    if max_current <= max_dim {
        return data.to_vec();
    }

    // Calculate new dimensions preserving aspect ratio
    let scale = max_dim as f32 / max_current as f32;
    let new_width = (width as f32 * scale) as u32;
    let new_height = (height as f32 * scale) as u32;

    // Resize using Lanczos3 (best quality for screenshots with text)
    let resized = img.resize_exact(new_width, new_height, FilterType::Lanczos3);

    // Encode back to original format
    let format = match mime {
        "image/png" => ImageFormat::Png,
        "image/jpeg" | "image/jpg" => ImageFormat::Jpeg,
        "image/gif" => ImageFormat::Gif,
        "image/webp" => ImageFormat::WebP,
        _ => return data.to_vec(), // Unknown format, return original
    };

    let mut output = Cursor::new(Vec::new());
    if resized.write_to(&mut output, format).is_err() {
        return data.to_vec(); // If encoding fails, return original
    }

    output.into_inner()
}

#[allow(clippy::too_many_lines)]
fn input_type(mime: &str, file: Option<&String>) -> io::Result<i32> {
    ensure_storage_directory()?;
    match mime {
        m if m.starts_with("text/plain") => {
            let text_path = get_text_path();
            if let Some(path_str) = file {
                let path = Path::new(path_str);
                validate_file_access(path)?;
                fs::copy(path, &text_path)?;
            } else {
                let mut buffer = Vec::new();
                io::stdin().read_to_end(&mut buffer)?;
                let mut file = File::create(&text_path)?;
                file.write_all(&buffer)?;
            }
            // restrict perms to user on unix
            #[cfg(unix)]
            {
                let _ = fs::set_permissions(&text_path, fs::Permissions::from_mode(0o600));
            }
            Ok(0)
        }
        "image/png" | "image/jpeg" | "image/jpg" | "image/gif" | "image/webp" => {
            let image_path = get_image_path();
            let format_path = get_image_format_path();

            // Read the image data
            let mut img_data = Vec::new();
            if let Some(path_str) = file {
                let path = Path::new(path_str);
                validate_file_access(path)?;
                let mut f = File::open(path)?;
                f.read_to_end(&mut img_data)?;
            } else {
                // Check stdin size limit
                let config = load_config();
                let max_bytes = config
                    .and_then(|c| c.max_file_size_mb)
                    .map_or(100 * 1024 * 1024, |mb| mb * 1024 * 1024); // Default 100MB

                let mut limited_reader = io::stdin().take(max_bytes + 1);
                limited_reader.read_to_end(&mut img_data)?;

                if img_data.len() > max_bytes.try_into().unwrap_or(usize::MAX) {
                    eprintln!("Error: Input exceeds maximum size");
                    return Ok(1);
                }
            }

            // Optionally downscale based on config
            let config = load_config();
            let max_dim = config.and_then(|c| c.max_image_dimension);
            let processed_data = downscale_image_if_needed(&img_data, mime, max_dim);

            // Write the (possibly downscaled) image
            let mut file = File::create(&image_path)?;
            file.write_all(&processed_data)?;

            // Store the format (normalize jpg to jpeg)
            let format = if mime == "image/jpg" {
                "image/jpeg"
            } else {
                mime
            };
            fs::write(&format_path, format)?;

            #[cfg(unix)]
            {
                let _ = fs::set_permissions(&image_path, fs::Permissions::from_mode(0o600));
                let _ = fs::set_permissions(&format_path, fs::Permissions::from_mode(0o600));
            }
            Ok(0)
        }
        _ => {
            // Reject unsupported formats
            eprintln!(
                "Error: Unsupported format '{mime}'. Only PNG, JPEG, GIF, and WebP are supported."
            );
            Ok(1)
        }
    }
}

fn main() -> ExitCode {
    let args = parse_args();

    // Output mode handling
    if args.mode_output {
        let code = match args.mime_type.as_deref() {
            Some("TARGETS") => {
                print_targets();
                0
            }
            None => output_type("text/plain").unwrap_or(1), // Default to text/plain
            Some(m) => output_type(m).unwrap_or(1),
        };
        return ExitCode::from(code.try_into().unwrap_or(1));
    }

    // input mode: default type to text/plain if none provided
    let mime = args.mime_type.as_deref().unwrap_or("text/plain");
    let code = input_type(mime, args.input_file.as_ref()).unwrap_or(1);
    ExitCode::from(code.try_into().unwrap_or(1))
}

// Config & TTL handling
#[derive(Debug, Deserialize, Default)]
struct BridgeConfig {
    #[serde(default)]
    ttl_secs: Option<u64>,
    #[serde(default)]
    max_image_dimension: Option<u32>,
    #[serde(default)]
    max_file_size_mb: Option<u64>,
    #[serde(default)]
    allowed_directories: Option<Vec<String>>,

    // wl-clipboard integration options
    #[serde(default)]
    clipboard_mode: Option<String>, // "auto", "file_only"
    #[serde(default)]
    cache_wl_images: Option<bool>, // Cache converted BMP→PNG
}

fn config_dir() -> PathBuf {
    if let Ok(xdg) = env::var("XDG_CONFIG_HOME")
        && !xdg.trim().is_empty()
    {
        return PathBuf::from(xdg).join("wsl-clip-bridge");
    }
    env::var("HOME").map_or_else(
        |_| PathBuf::from("/tmp").join("wsl-clip-bridge"),
        |h| PathBuf::from(h).join(".config").join("wsl-clip-bridge"),
    )
}

fn config_path() -> PathBuf {
    if let Ok(p) = env::var("WSL_CLIP_BRIDGE_CONFIG")
        && !p.trim().is_empty()
    {
        return PathBuf::from(p);
    }
    config_dir().join("config.toml")
}

fn load_config() -> Option<BridgeConfig> {
    let path = config_path();
    if !path.exists() {
        // attempt to create default config file
        if let Some(dir) = path.parent() {
            let _ = fs::create_dir_all(dir);
            #[cfg(unix)]
            {
                let _ = fs::set_permissions(dir, fs::Permissions::from_mode(0o700));
            }
        }
        let default = "# WSL Clip Bridge Configuration\n\n# Clipboard data TTL in seconds (default: 300)\nttl_secs = 300\n\n# Maximum image dimension for automatic downscaling\n# Set to 1568 for optimal Claude API performance\n# Set to 0 to disable downscaling\nmax_image_dimension = 1568\n\n# Maximum file size in MB (default: 100)\nmax_file_size_mb = 100\n\n# Clipboard integration mode\n# \"auto\" = Check files first, then wl-clipboard (default)\n# \"file_only\" = Only use file-based clipboard (ShareX mode)\nclipboard_mode = \"auto\"\n\n# Cache images from wl-clipboard for faster subsequent access\ncache_wl_images = true\n\n# Security: Directory access restrictions\n# If not configured, all paths are allowed\n# To restrict access to specific directories (and their subdirectories):\n#\n# allowed_directories = [\n#   \"/mnt/c/Users/YOUR_USERNAME/Documents/ShareX\",\n#   \"/home/YOUR_USERNAME\",\n#   \"/tmp\"\n# ]\n";
        let _ = fs::write(&path, default);
        #[cfg(unix)]
        {
            let _ = fs::set_permissions(&path, fs::Permissions::from_mode(0o600));
        }
        return None;
    }
    fs::read_to_string(&path)
        .ok()
        .and_then(|s| toml::from_str::<BridgeConfig>(&s).ok())
}

fn load_ttl() -> Duration {
    // Env var override in seconds
    if let Ok(v) = env::var("WSL_CLIP_BRIDGE_TTL_SECS")
        && let Ok(secs) = v.trim().parse::<u64>()
    {
        return Duration::from_secs(secs.min(86_400));
    }
    // TOML config: $XDG_CONFIG_HOME/wsl-clip-bridge/config.toml
    if let Some(cfg) = load_config()
        && let Some(secs) = cfg.ttl_secs
    {
        return Duration::from_secs(secs.min(86_400));
    }
    // default 5 minutes
    Duration::from_secs(300)
}

fn is_file_fresh(path: &Path, ttl: Duration) -> bool {
    fs::metadata(path)
        .and_then(|metadata| metadata.modified())
        .is_ok_and(|modified_time| {
            SystemTime::now()
                .duration_since(modified_time)
                .is_ok_and(|elapsed| elapsed <= ttl && is_file_non_empty(path))
        })
}

// wl-clipboard integration functions

/// Default timeout for wl-paste commands (2 seconds)
const WL_CLIPBOARD_TIMEOUT: Duration = Duration::from_secs(2);

/// Run a command with a timeout, killing it if it takes too long
fn run_command_with_timeout(mut cmd: Command, timeout: Duration) -> io::Result<std::process::Output> {
    let mut child = cmd.spawn()?;

    let start = std::time::Instant::now();
    loop {
        match child.try_wait()? {
            Some(status) => {
                // Process finished, collect output
                let mut stdout = Vec::new();
                let mut stderr = Vec::new();
                if let Some(mut out) = child.stdout.take() {
                    out.read_to_end(&mut stdout)?;
                }
                if let Some(mut err) = child.stderr.take() {
                    err.read_to_end(&mut stderr)?;
                }
                return Ok(std::process::Output {
                    status,
                    stdout,
                    stderr,
                });
            }
            None => {
                if start.elapsed() > timeout {
                    // Timeout exceeded, kill the process
                    let _ = child.kill();
                    let _ = child.wait(); // Reap the zombie
                    return Err(io::Error::new(
                        io::ErrorKind::TimedOut,
                        "Command timed out",
                    ));
                }
                // Sleep briefly before checking again
                std::thread::sleep(Duration::from_millis(50));
            }
        }
    }
}

fn wl_clipboard_available() -> bool {
    // Check config first
    let config = load_config();
    if let Some(cfg) = config
        && let Some(mode) = cfg.clipboard_mode.as_deref()
        && mode == "file_only"
    {
        return false;
    }

    // Auto-detect wl-paste availability
    Command::new("which")
        .arg("wl-paste")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn get_wl_clipboard_types() -> io::Result<Vec<String>> {
    let mut cmd = Command::new("wl-paste");
    cmd.arg("--list-types")
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());

    let output = run_command_with_timeout(cmd, WL_CLIPBOARD_TIMEOUT)?;

    if output.status.success() {
        let types = String::from_utf8_lossy(&output.stdout);
        Ok(types.lines().map(String::from).collect())
    } else {
        Ok(vec![])
    }
}

fn fetch_from_wl_clipboard(mime_type: &str) -> io::Result<Vec<u8>> {
    let mut cmd = Command::new("wl-paste");
    cmd.arg("-t")
        .arg(mime_type)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());

    let output = run_command_with_timeout(cmd, WL_CLIPBOARD_TIMEOUT)?;

    if output.status.success() {
        Ok(output.stdout)
    } else {
        Err(io::Error::other("Failed to fetch from clipboard"))
    }
}
