//! wsl-clip-bridge
//!
//! A stateless `xclip` shim for Claude Code on WSL. Shells out to `wl-paste`
//! for the Windows clipboard and, when the payload is a `BI_BITFIELDS` BMP
//! (which sharp/libvips refuses), decodes it via the Rust `image` crate and
//! emits a PNG. That is the only reason this tool exists: Claude Code's paste
//! pipeline otherwise silently fails on WSLg-sourced screenshots.

use std::env;
use std::io::{self, Cursor, Read, Write};
use std::process::{Command, ExitCode, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use image::ImageFormat;

const VERSION: &str = match option_env!("WSL_CLIP_BRIDGE_VERSION") {
    Some(v) => v,
    None => env!("CARGO_PKG_VERSION"),
};

/// Budget for a single `wl-paste` invocation, spawn to exit.
const WL_TIMEOUT: Duration = Duration::from_secs(5);

/// How often the child is polled for exit. Claude Code runs up to four xclip
/// calls per paste, so the poll interval bounds the latency each call adds.
const POLL_INTERVAL: Duration = Duration::from_millis(5);

/// Extra time the pipe readers get to deliver data after the child has
/// exited. EOF is immediate then, unless something else (e.g. a grandchild
/// process) still holds the child's pipe open.
const READER_GRACE: Duration = Duration::from_secs(1);

// ---------------------------------------------------------------------------
// CLI
// ---------------------------------------------------------------------------

enum Cli {
    Help,
    Version,
    Run(Request),
}

struct Request {
    mime: Option<String>,
    output: bool,
}

fn parse_args<I: Iterator<Item = String>>(mut args: I) -> Cli {
    let mut request = Request {
        mime: None,
        output: false,
    };
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-h" | "--help" => return Cli::Help,
            "-V" | "--version" => return Cli::Version,
            "-selection" => {
                args.next(); // consume value, ignore (xclip compat)
            }
            "-t" => request.mime = args.next(),
            "-o" => request.output = true,
            _ => {}
        }
    }
    Cli::Run(request)
}

fn print_help() {
    println!(
        "wsl-clip-bridge {VERSION} - Claude Code paste helper for WSL

Ships as 'xclip' on PATH. When Claude Code's paste pipeline asks for a PNG
and the Windows clipboard is a BMP, this tool decodes the BMP (including
the BI_BITFIELDS variant that sharp/libvips rejects) and emits a PNG.

USAGE:
    xclip -selection clipboard -t <MIME> -o

OPTIONS:
    -o              Output clipboard contents
    -t <MIME>       MIME type: TARGETS, text/plain, image/png, image/bmp, ...
    -selection <S>  Ignored (xclip compat)
    -h, --help      Show this help
    -V, --version   Show version

Claude Code invokes:
    xclip -selection clipboard -t TARGETS -o
    xclip -selection clipboard -t image/png -o
    xclip -selection clipboard -t image/bmp -o
    xclip -selection clipboard -t text/plain -o

Source: https://github.com/camjac251/wsl-clip-bridge"
    );
}

fn main() -> ExitCode {
    match parse_args(env::args().skip(1)) {
        Cli::Help => {
            print_help();
            ExitCode::SUCCESS
        }
        Cli::Version => {
            println!("wsl-clip-bridge {VERSION}");
            ExitCode::SUCCESS
        }
        Cli::Run(request) => match run(&request) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("xclip: {e}");
                ExitCode::FAILURE
            }
        },
    }
}

fn run(request: &Request) -> io::Result<()> {
    if !request.output {
        return Err(io::Error::other(
            "write mode (-i) is not implemented. This is a read-only Claude Code paste shim.",
        ));
    }
    match request.mime.as_deref() {
        Some("TARGETS") => print_targets(),
        Some(mime) => output(mime),
        None => output("text/plain"),
    }
}

// ---------------------------------------------------------------------------
// wl-paste runner
// ---------------------------------------------------------------------------

fn run_wl_paste(args: &[&str]) -> io::Result<Vec<u8>> {
    wl_paste_inner(args)
        .map_err(|e| io::Error::other(format!("wl-paste {} failed: {e}", args.join(" "))))
}

fn wl_paste_inner(args: &[&str]) -> io::Result<Vec<u8>> {
    let mut child = Command::new("wl-paste")
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    // Drain both pipes from dedicated threads so neither can deadlock the
    // child by filling its kernel pipe buffer.
    let stdout_rx = spawn_reader(child.stdout.take());
    let stderr_rx = spawn_reader(child.stderr.take());

    let deadline = Instant::now() + WL_TIMEOUT;
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                format!("timed out after {}s", WL_TIMEOUT.as_secs()),
            ));
        }
        thread::sleep(POLL_INTERVAL);
    };

    let stdout = stdout_rx.recv_timeout(READER_GRACE).unwrap_or_default();
    let stderr = stderr_rx.recv_timeout(READER_GRACE).unwrap_or_default();
    if status.success() {
        return Ok(stdout);
    }
    let stderr = String::from_utf8_lossy(&stderr);
    let stderr = stderr.trim();
    Err(io::Error::other(if stderr.is_empty() {
        format!("exited with {status}")
    } else {
        format!("exited with {status}: {stderr}")
    }))
}

/// Read a child pipe to EOF on a dedicated thread and hand the bytes back
/// through a channel.
fn spawn_reader<R: Read + Send + 'static>(pipe: Option<R>) -> mpsc::Receiver<Vec<u8>> {
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let mut buf = Vec::new();
        if let Some(mut pipe) = pipe {
            let _ = pipe.read_to_end(&mut buf);
        }
        let _ = tx.send(buf);
    });
    rx
}

fn wl_list_types() -> io::Result<Vec<String>> {
    let bytes = run_wl_paste(&["--list-types"])?;
    Ok(String::from_utf8_lossy(&bytes)
        .lines()
        .map(str::to_owned)
        .collect())
}

fn wl_fetch(mime: &str) -> io::Result<Vec<u8>> {
    run_wl_paste(&["-t", mime])
}

// ---------------------------------------------------------------------------
// xclip verbs
// ---------------------------------------------------------------------------

fn print_targets() -> io::Result<()> {
    let types = wl_list_types()?;
    let targets = advertised_targets(&types);
    if targets.is_empty() {
        return Err(io::Error::other("clipboard has no supported targets"));
    }
    let mut text = targets.join("\n");
    text.push('\n');
    write_stdout(text.as_bytes())
}

/// The one real contribution of this tool: advertise `image/png` when the
/// clipboard only offers a BMP, so Claude Code's paste path asks for PNG and
/// `output()` can hand back a converted one.
fn advertised_targets(types: &[String]) -> Vec<&str> {
    let has = |mime: &str| types.iter().any(|t| t == mime);
    let mut targets = Vec::new();
    if has("image/bmp") && !has("image/png") {
        targets.push("image/png");
    }
    for t in types.iter().map(String::as_str) {
        match t {
            "image/png" | "image/gif" | "image/webp" | "image/bmp" => targets.push(t),
            "image/jpeg" => {
                targets.push(t);
                targets.push("image/jpg");
            }
            _ if t.starts_with("text/") => targets.push(t),
            _ => {}
        }
    }
    targets
}

fn output(mime: &str) -> io::Result<()> {
    match mime {
        m if m.starts_with("text/") => passthrough(m),
        // Try PNG directly first. On WSLg the clipboard only advertises BMP,
        // so this call fails fast and we fall through to the BMP decoder.
        "image/png" => {
            wl_fetch("image/png").map_or_else(|_| output_bmp_as_png(), |png| write_stdout(&png))
        }
        "image/jpg" => passthrough("image/jpeg"),
        "image/jpeg" | "image/gif" | "image/webp" | "image/bmp" => passthrough(mime),
        _ => Err(io::Error::other(format!("unsupported MIME type: {mime}"))),
    }
}

fn passthrough(mime: &str) -> io::Result<()> {
    let data = wl_fetch(mime)?;
    write_stdout(&data)
}

fn output_bmp_as_png() -> io::Result<()> {
    let bmp = wl_fetch("image/bmp")?;
    let png = bmp_to_png(&bmp)?;
    write_stdout(&png)
}

fn bmp_to_png(bmp: &[u8]) -> io::Result<Vec<u8>> {
    let img = image::load_from_memory(bmp)
        .map_err(|e| io::Error::other(format!("BMP decode failed: {e}")))?;
    let mut png = Cursor::new(Vec::new());
    img.write_to(&mut png, ImageFormat::Png)
        .map_err(|e| io::Error::other(format!("PNG encode failed: {e}")))?;
    Ok(png.into_inner())
}

fn write_stdout(data: &[u8]) -> io::Result<()> {
    let mut stdout = io::stdout().lock();
    let result = stdout.write_all(data).and_then(|()| stdout.flush());
    if let Err(e) = result
        // A closed downstream pipe (e.g. piping into `head`) is not our failure.
        && e.kind() != io::ErrorKind::BrokenPipe
    {
        return Err(io::Error::other(format!("stdout write failed: {e}")));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const PNG_MAGIC: &[u8] = &[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];

    fn types(list: &[&str]) -> Vec<String> {
        list.iter().map(ToString::to_string).collect()
    }

    fn cli(args: &[&str]) -> Cli {
        parse_args(args.iter().map(ToString::to_string))
    }

    /// Build the smallest `BI_RGB` BMP we can round-trip through `image`.
    /// Two pixels wide, one tall, 24 bits per pixel, no compression. Paired
    /// with `tiny_bmp_bitfields` below, which exercises the variant `WSLg`
    /// actually delivers.
    fn tiny_bmp() -> Vec<u8> {
        let file_size: u32 = 58;
        let data_offset: u32 = 54;
        let header_size: u32 = 40;
        let width: i32 = 2;
        let height: i32 = 1;
        let planes: u16 = 1;
        let bpp: u16 = 24;
        let mut bmp = Vec::with_capacity(file_size as usize);
        bmp.extend_from_slice(b"BM");
        bmp.extend_from_slice(&file_size.to_le_bytes());
        bmp.extend_from_slice(&0u16.to_le_bytes());
        bmp.extend_from_slice(&0u16.to_le_bytes());
        bmp.extend_from_slice(&data_offset.to_le_bytes());
        bmp.extend_from_slice(&header_size.to_le_bytes());
        bmp.extend_from_slice(&width.to_le_bytes());
        bmp.extend_from_slice(&height.to_le_bytes());
        bmp.extend_from_slice(&planes.to_le_bytes());
        bmp.extend_from_slice(&bpp.to_le_bytes());
        bmp.extend_from_slice(&0u32.to_le_bytes()); // BI_RGB
        bmp.extend_from_slice(&0u32.to_le_bytes()); // image size
        bmp.extend_from_slice(&0u32.to_le_bytes()); // x ppm
        bmp.extend_from_slice(&0u32.to_le_bytes()); // y ppm
        bmp.extend_from_slice(&0u32.to_le_bytes()); // colors used
        bmp.extend_from_slice(&0u32.to_le_bytes()); // important colors
        // Two pixels stored BGR (blue, green), plus 2 pad bytes to align the
        // row to 4 bytes.
        bmp.extend_from_slice(&[255, 0, 0, 0, 255, 0, 0, 0]);
        bmp
    }

    /// Build a minimal 32-bpp `BI_BITFIELDS` BMP. This is the variant `WSLg`
    /// delivers and the only reason this tool exists; the `BI_RGB` test
    /// above only proves the wiring works. Guards against the `image` crate
    /// regressing on `BI_BITFIELDS` decode.
    fn tiny_bmp_bitfields() -> Vec<u8> {
        let header_size: u32 = 40;
        let data_offset: u32 = 14 + header_size + 12; // file header + DIB + masks
        let file_size: u32 = data_offset + 8; // + 2 px * 4 bytes
        let width: i32 = 2;
        let height: i32 = 1;
        let planes: u16 = 1;
        let bpp: u16 = 32;
        let bi_bitfields: u32 = 3;
        let r_mask: u32 = 0x00FF_0000;
        let g_mask: u32 = 0x0000_FF00;
        let b_mask: u32 = 0x0000_00FF;

        let mut bmp = Vec::with_capacity(file_size as usize);
        bmp.extend_from_slice(b"BM");
        bmp.extend_from_slice(&file_size.to_le_bytes());
        bmp.extend_from_slice(&0u16.to_le_bytes());
        bmp.extend_from_slice(&0u16.to_le_bytes());
        bmp.extend_from_slice(&data_offset.to_le_bytes());
        bmp.extend_from_slice(&header_size.to_le_bytes());
        bmp.extend_from_slice(&width.to_le_bytes());
        bmp.extend_from_slice(&height.to_le_bytes());
        bmp.extend_from_slice(&planes.to_le_bytes());
        bmp.extend_from_slice(&bpp.to_le_bytes());
        bmp.extend_from_slice(&bi_bitfields.to_le_bytes());
        bmp.extend_from_slice(&0u32.to_le_bytes()); // image size (0 ok for BI_BITFIELDS)
        bmp.extend_from_slice(&0u32.to_le_bytes()); // x ppm
        bmp.extend_from_slice(&0u32.to_le_bytes()); // y ppm
        bmp.extend_from_slice(&0u32.to_le_bytes()); // colors used
        bmp.extend_from_slice(&0u32.to_le_bytes()); // important colors
        bmp.extend_from_slice(&r_mask.to_le_bytes());
        bmp.extend_from_slice(&g_mask.to_le_bytes());
        bmp.extend_from_slice(&b_mask.to_le_bytes());
        // Two pixels (red, green) packed as little-endian u32 against the masks above.
        bmp.extend_from_slice(&0x00FF_0000u32.to_le_bytes());
        bmp.extend_from_slice(&0x0000_FF00u32.to_le_bytes());
        bmp
    }

    #[test]
    fn bmp_round_trips_to_png() {
        let png = bmp_to_png(&tiny_bmp()).expect("convert BI_RGB BMP");
        assert_eq!(&png[..8], PNG_MAGIC);
        let img = image::load_from_memory(&png).expect("decode PNG");
        let rgb = img.to_rgb8();
        assert_eq!(rgb.get_pixel(0, 0), &image::Rgb([0, 0, 255]));
        assert_eq!(rgb.get_pixel(1, 0), &image::Rgb([0, 255, 0]));
    }

    #[test]
    fn bitfields_bmp_round_trips_to_png() {
        let png = bmp_to_png(&tiny_bmp_bitfields()).expect("convert BI_BITFIELDS BMP");
        assert_eq!(&png[..8], PNG_MAGIC);
        // Decode back and check the pixels landed on the right channels, so a
        // red/blue mask mix-up cannot slip through.
        let img = image::load_from_memory(&png).expect("decode PNG");
        assert_eq!(img.width(), 2);
        assert_eq!(img.height(), 1);
        let rgb = img.to_rgb8();
        assert_eq!(rgb.get_pixel(0, 0), &image::Rgb([255, 0, 0]));
        assert_eq!(rgb.get_pixel(1, 0), &image::Rgb([0, 255, 0]));
    }

    #[test]
    fn bmp_to_png_rejects_garbage() {
        assert!(bmp_to_png(b"not a bmp").is_err());
    }

    #[test]
    fn targets_synthesizes_png_for_bmp_only_clipboard() {
        assert_eq!(
            advertised_targets(&types(&["image/bmp"])),
            ["image/png", "image/bmp"]
        );
    }

    #[test]
    fn targets_does_not_duplicate_existing_png() {
        assert_eq!(
            advertised_targets(&types(&["image/png", "image/bmp"])),
            ["image/png", "image/bmp"]
        );
    }

    #[test]
    fn targets_aliases_jpeg_as_jpg() {
        assert_eq!(
            advertised_targets(&types(&["image/jpeg"])),
            ["image/jpeg", "image/jpg"]
        );
    }

    #[test]
    fn targets_keeps_text_and_drops_unknown() {
        assert_eq!(
            advertised_targets(&types(&["text/plain;charset=utf-8", "application/x-foo"])),
            ["text/plain;charset=utf-8"]
        );
    }

    #[test]
    fn targets_empty_for_unsupported_clipboard() {
        assert!(advertised_targets(&types(&["application/x-foo"])).is_empty());
    }

    #[test]
    fn parses_claude_code_invocation() {
        let Cli::Run(request) = cli(&["-selection", "clipboard", "-t", "image/png", "-o"]) else {
            panic!("expected Cli::Run");
        };
        assert_eq!(request.mime.as_deref(), Some("image/png"));
        assert!(request.output);
    }

    #[test]
    fn parses_missing_output_flag() {
        let Cli::Run(request) = cli(&["-t", "TARGETS"]) else {
            panic!("expected Cli::Run");
        };
        assert!(!request.output);
    }

    #[test]
    fn help_and_version_flags_win() {
        assert!(matches!(cli(&["-t", "image/png", "-h"]), Cli::Help));
        assert!(matches!(cli(&["--version"]), Cli::Version));
    }
}
