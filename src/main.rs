//! wsl-clip-bridge
//!
//! A stateless `xclip` shim for Claude Code on WSL. Shells out to `wl-paste`
//! for the Windows clipboard and, when the payload is a `BI_BITFIELDS` BMP
//! (which sharp/libvips refuses), decodes it via the Rust `image` crate and
//! emits a PNG. That is the only reason this tool exists: Claude Code's paste
//! pipeline otherwise silently fails on WSLg-sourced screenshots.

use std::env;
use std::io::{self, Cursor, Read, Write};
use std::process::{Command, ExitCode};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use image::ImageFormat;

const VERSION: &str = match option_env!("WSL_CLIP_BRIDGE_VERSION") {
    Some(v) => v,
    None => env!("CARGO_PKG_VERSION"),
};

const WL_TIMEOUT: Duration = Duration::from_secs(5);

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

struct Args {
    mime: Option<String>,
    output: bool,
}

fn parse_args() -> Args {
    let mut args = Args {
        mime: None,
        output: false,
    };
    let mut it = env::args().skip(1);
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "-h" | "--help" => {
                print_help();
                std::process::exit(0);
            }
            "-V" | "--version" => {
                println!("wsl-clip-bridge {VERSION}");
                std::process::exit(0);
            }
            "-selection" => {
                it.next(); // consume value, ignore
            }
            "-t" => {
                args.mime = it.next();
            }
            "-o" => {
                args.output = true;
            }
            _ => {}
        }
    }
    args
}

fn main() -> ExitCode {
    let args = parse_args();
    if !args.output {
        eprintln!(
            "xclip: write mode (-i) is not implemented. This is a read-only Claude Code paste shim."
        );
        return ExitCode::from(1);
    }
    let code = match args.mime.as_deref() {
        Some("TARGETS") => print_targets(),
        Some(m) => output(m),
        None => output("text/plain"),
    };
    ExitCode::from(u8::try_from(code).unwrap_or(1))
}

// ---------------------------------------------------------------------------
// wl-paste runner
// ---------------------------------------------------------------------------

fn run_wl_paste(extra_args: &[&str]) -> io::Result<Vec<u8>> {
    let mut cmd = Command::new("wl-paste");
    cmd.args(extra_args)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());

    let mut child = cmd.spawn()?;
    let child_stdout = child.stdout.take();
    let child_stderr = child.stderr.take();

    // Drain both pipes from dedicated threads so neither can deadlock the
    // child by filling its kernel pipe buffer.
    let (stdout_tx, stdout_rx) = mpsc::channel();
    let (stderr_tx, stderr_rx) = mpsc::channel();
    let stdout_reader = thread::spawn(move || {
        let mut buf = Vec::new();
        if let Some(mut out) = child_stdout {
            let _ = out.read_to_end(&mut buf);
        }
        let _ = stdout_tx.send(buf);
    });
    let stderr_reader = thread::spawn(move || {
        let mut buf = Vec::new();
        if let Some(mut err) = child_stderr {
            let _ = err.read_to_end(&mut buf);
        }
        let _ = stderr_tx.send(buf);
    });

    let start = Instant::now();
    loop {
        if let Some(status) = child.try_wait()? {
            let _ = stdout_reader.join();
            let _ = stderr_reader.join();
            let stdout = stdout_rx.recv().unwrap_or_default();
            let stderr = stderr_rx.recv().unwrap_or_default();
            if status.success() {
                return Ok(stdout);
            }
            let stderr_text = String::from_utf8_lossy(&stderr);
            let trimmed = stderr_text.trim();
            return Err(io::Error::other(if trimmed.is_empty() {
                format!("wl-paste exited with {status}")
            } else {
                format!("wl-paste exited with {status}: {trimmed}")
            }));
        }
        if start.elapsed() > WL_TIMEOUT {
            let _ = child.kill();
            let _ = child.wait();
            let _ = stdout_reader.join();
            let _ = stderr_reader.join();
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "wl-paste timed out after 5s",
            ));
        }
        thread::sleep(Duration::from_millis(50));
    }
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

fn print_targets() -> i32 {
    let types = match wl_list_types() {
        Ok(t) => t,
        Err(e) => {
            eprintln!("xclip: wl-paste --list-types failed: {e}");
            return 1;
        }
    };
    let has_bmp = types.iter().any(|t| t == "image/bmp");
    let has_png = types.iter().any(|t| t == "image/png");
    let mut count: usize = 0;

    // The one real contribution: advertise image/png when the clipboard only
    // has a BMP, so Claude Code's paste path tries the PNG MIME first and we
    // can hand back a converted PNG from output().
    if has_bmp && !has_png {
        println!("image/png");
        count += 1;
    }

    for t in &types {
        match t.as_str() {
            "image/png" | "image/jpeg" | "image/gif" | "image/webp" | "image/bmp" => {
                println!("{t}");
                count += 1;
                if t == "image/jpeg" {
                    println!("image/jpg");
                    count += 1;
                }
            }
            s if s.starts_with("text/") => {
                println!("{t}");
                count += 1;
            }
            _ => {}
        }
    }

    i32::from(count == 0)
}

fn output(mime: &str) -> i32 {
    match mime {
        m if m.starts_with("text/") => passthrough(m),
        // Try PNG directly first. On WSLg the clipboard only advertises BMP,
        // so this call fails fast and we fall through to the BMP decoder.
        "image/png" => wl_fetch("image/png").map_or_else(|_| bmp_to_png(), |d| write_stdout(&d)),
        "image/jpg" => passthrough("image/jpeg"),
        "image/jpeg" | "image/gif" | "image/webp" | "image/bmp" => passthrough(mime),
        _ => {
            eprintln!("xclip: unsupported MIME type: {mime}");
            1
        }
    }
}

fn passthrough(mime: &str) -> i32 {
    match wl_fetch(mime) {
        Ok(d) => write_stdout(&d),
        Err(e) => {
            eprintln!("xclip: wl-paste -t {mime} failed: {e}");
            1
        }
    }
}

fn bmp_to_png() -> i32 {
    let bmp = match wl_fetch("image/bmp") {
        Ok(d) => d,
        Err(e) => {
            eprintln!("xclip: wl-paste -t image/bmp failed: {e}");
            return 1;
        }
    };
    let img = match image::load_from_memory(&bmp) {
        Ok(i) => i,
        Err(e) => {
            eprintln!("xclip: BMP decode failed: {e}");
            return 1;
        }
    };
    let mut buf = Cursor::new(Vec::new());
    if let Err(e) = img.write_to(&mut buf, ImageFormat::Png) {
        eprintln!("xclip: PNG encode failed: {e}");
        return 1;
    }
    write_stdout(&buf.into_inner())
}

fn write_stdout(data: &[u8]) -> i32 {
    i32::from(io::stdout().write_all(data).is_err())
}

#[cfg(test)]
mod tests {
    use super::*;

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
        // 2 pixels * 3 bytes + 2 pad bytes to align row to 4 bytes
        bmp.extend_from_slice(&[255, 0, 0, 0, 255, 0, 0, 0]);
        bmp
    }

    #[test]
    fn bmp_round_trips_to_png() {
        let bmp = tiny_bmp();
        assert_eq!(&bmp[0..2], b"BM");
        let img = image::load_from_memory(&bmp).expect("load BMP");
        let mut out = Cursor::new(Vec::new());
        img.write_to(&mut out, ImageFormat::Png)
            .expect("encode PNG");
        let png = out.into_inner();
        assert_eq!(
            &png[0..8],
            &[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]
        );
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
    fn bitfields_bmp_round_trips_to_png() {
        let bmp = tiny_bmp_bitfields();
        assert_eq!(&bmp[0..2], b"BM");
        let img = image::load_from_memory(&bmp).expect("load BI_BITFIELDS BMP");
        assert_eq!(img.width(), 2);
        assert_eq!(img.height(), 1);
        let mut out = Cursor::new(Vec::new());
        img.write_to(&mut out, ImageFormat::Png)
            .expect("encode PNG");
        let png = out.into_inner();
        assert_eq!(
            &png[0..8],
            &[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]
        );
    }
}
