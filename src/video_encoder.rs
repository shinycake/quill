//! Which ffmpeg writes H.264 (or VP9) for Quill's command lines.
//!
//! The command lines name `libx264`, the encoder every distro and Homebrew
//! ffmpeg has. The ffmpeg Quill ships is LGPL-only and so cannot contain
//! libx264 (GPL): on Windows it carries Media Foundation's `h264_mf`, on
//! Linux no H.264 encoder at all. So the encoder comes from the first ffmpeg
//! (the bundled one, then `PATH`) that has one, and the `-c:v libx264 -preset
//! .. -crf ..` part of the arguments is rewritten for it.
//! (docs/decisions/sweep-ffmpeg-tools-bundled.md)
#![deny(clippy::indexing_slicing)]

use crate::media_tools::{NoConsole, candidates};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{LazyLock, Mutex};

/// H.264 encoders, best first: x264 (what the command lines name), then the
/// operating system's (VideoToolbox on macOS, Media Foundation on Windows).
const H264_ENCODERS: [&str; 3] = ["libx264", "h264_videotoolbox", "h264_mf"];

/// How long `ffmpeg -encoders` may take; it is quick, but encoding starts
/// from the UI thread for round videos.
const LISTING_LIMIT: std::time::Duration = std::time::Duration::from_secs(5);

/// Cached successful listings; a failed or timed-out one is retried next time.
static LISTINGS: LazyLock<Mutex<HashMap<PathBuf, String>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// An ffmpeg program with the arguments to run it with.
pub struct EncodeJob {
    pub program: PathBuf,
    pub args: Vec<String>,
}

/// The ffmpeg and arguments that encode `args` (which name `libx264`) as
/// H.264, `pixels` being the output's width times height (it sizes the
/// bitrate where the encoder has no constant-quality mode). `what` starts
/// the error message ("{what}: Quill needs an ffmpeg ...").
pub fn h264_job(what: &str, args: &[String], pixels: u32) -> Result<EncodeJob, String> {
    let tools = candidates("ffmpeg");
    if tools.is_empty() {
        return Err(crate::media_tools::ffmpeg_missing_message(what));
    }
    tools
        .into_iter()
        .find_map(|program| {
            let encoder = pick_h264_encoder(&encoders_of(&program)?)?;
            Some(EncodeJob {
                args: rewrite_h264_args(args, encoder, pixels),
                program,
            })
        })
        .ok_or_else(|| {
            format!(
                "{what}: Quill needs an ffmpeg with an H.264 encoder, and the one found can't \
                 write H.264 (install ffmpeg with libx264, for example `brew install ffmpeg` or \
                 `sudo apt install ffmpeg`)."
            )
        })
}

/// The first ffmpeg that lists `encoder`.
pub fn program_with_encoder(encoder: &str) -> Option<PathBuf> {
    candidates("ffmpeg")
        .into_iter()
        .find(|program| encoders_of(program).is_some_and(|l| lists_encoder(&l, encoder)))
}

/// List the encoders of every ffmpeg now, so the first encode doesn't wait.
/// Run it off the UI thread (see `media_tools::warm_up`).
pub fn warm_listings() {
    for program in candidates("ffmpeg") {
        let _ = encoders_of(&program);
    }
}

/// A `Command` for `program` without a console window on Windows.
pub fn quiet_command_for(program: &Path) -> Command {
    let mut command = Command::new(program);
    command.no_console();
    command
}

/// `program -encoders`, cached; `None` when it didn't answer in time.
fn encoders_of(program: &Path) -> Option<String> {
    if let Ok(cache) = LISTINGS.lock()
        && let Some(listing) = cache.get(program)
    {
        return Some(listing.clone());
    }
    let mut list = quiet_command_for(program);
    list.args(["-hide_banner", "-encoders"]);
    let listing = crate::media_capture::stdout_within(&mut list, LISTING_LIMIT)?;
    if let Ok(mut cache) = LISTINGS.lock() {
        cache.insert(program.to_path_buf(), listing.clone());
    }
    Some(listing)
}

/// True when an `-encoders` line `V....D name  description` names `encoder`.
fn lists_encoder(listing: &str, encoder: &str) -> bool {
    listing.lines().any(|line| {
        let mut words = line.split_whitespace();
        words.next().is_some_and(|flags| flags.starts_with('V')) && words.next() == Some(encoder)
    })
}

/// The best H.264 encoder an `-encoders` listing has.
fn pick_h264_encoder(listing: &str) -> Option<&'static str> {
    H264_ENCODERS
        .into_iter()
        .find(|encoder| lists_encoder(listing, encoder))
}

/// `-c:v <encoder>` and its rate control. Only x264 has constant quality;
/// the others take a bitrate of about 3 bits per pixel per second.
fn h264_codec_args(encoder: &str, pixels: u32) -> Vec<String> {
    let bitrate = pixels.saturating_mul(3).clamp(500_000, 8_000_000);
    ["-c:v", encoder, "-b:v", &bitrate.to_string()]
        .map(String::from)
        .to_vec()
}

/// [`h264_codec_args`], minus the bitrate when the command line sets its own.
fn h264_codec_args_for(args: &[String], encoder: &str, pixels: u32) -> Vec<String> {
    let mut codec = h264_codec_args(encoder, pixels);
    if args.iter().any(|arg| arg == "-b:v") {
        codec.truncate(2);
    }
    codec
}

/// `args` with `-c:v libx264` and its `-preset X` / `-crf N` (wherever they
/// sit in the options) turned into the options
/// `encoder` understands (a command line's own `-b:v` is kept); unchanged
/// for libx264 itself.
fn rewrite_h264_args(args: &[String], encoder: &str, pixels: u32) -> Vec<String> {
    if encoder == "libx264" {
        return args.to_vec();
    }
    let names_x264 = args
        .windows(2)
        .any(|pair| matches!(pair, [flag, codec] if flag == "-c:v" && codec == "libx264"));
    let mut out = Vec::with_capacity(args.len());
    let mut iter = args.iter().peekable();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "-c:v" if iter.peek().is_some_and(|next| next.as_str() == "libx264") => {
                iter.next();
                out.extend(h264_codec_args_for(args, encoder, pixels));
            }
            "-preset" | "-crf" if names_x264 => {
                iter.next();
            }
            _ => out.push(arg.clone()),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{h264_codec_args, lists_encoder, pick_h264_encoder, rewrite_h264_args};

    fn strings(list: &[&str]) -> Vec<String> {
        list.iter().map(|arg| arg.to_string()).collect()
    }

    const WINDOWS_BUNDLED: &str = " V....D mpeg4                MPEG-4 part 2\n \
V....D h264_mf              H264 via MediaFoundation (codec h264)\n \
A....D aac                  AAC (Advanced Audio Coding)\n";
    const DISTRO: &str = " V....D libx264              libx264 H.264 / AVC (codec h264)\n \
V....D h264_nvenc           NVIDIA NVENC H.264 encoder (codec h264)\n";

    #[test]
    fn the_encoder_comes_from_the_listing_best_first() {
        assert_eq!(pick_h264_encoder(WINDOWS_BUNDLED), Some("h264_mf"));
        assert_eq!(pick_h264_encoder(DISTRO), Some("libx264"));
        assert_eq!(pick_h264_encoder(" V....D mpeg4  MPEG-4 part 2\n"), None);
        assert_eq!(pick_h264_encoder(""), None);
    }

    #[test]
    fn only_video_encoder_lines_count() {
        assert!(lists_encoder(WINDOWS_BUNDLED, "mpeg4"));
        assert!(!lists_encoder(WINDOWS_BUNDLED, "aac"));
        assert!(!lists_encoder("Encoders:\n V..... = Video\n", "Video"));
        assert!(lists_encoder(
            " V....D libvpx-vp9 libvpx VP9\n",
            "libvpx-vp9"
        ));
    }

    #[test]
    fn libx264_arguments_are_left_alone() {
        let args = strings(&["-i", "in.mp4", "-c:v", "libx264", "-crf", "23", "out.mp4"]);
        assert_eq!(rewrite_h264_args(&args, "libx264", 921_600), args);
    }

    #[test]
    fn x264_options_become_a_bitrate_for_the_system_encoder() {
        let args = strings(&[
            "-y",
            "-i",
            "in.mp4",
            "-c:v",
            "libx264",
            "-preset",
            "veryfast",
            "-crf",
            "23",
            "-c:a",
            "aac",
            "-movflags",
            "+faststart",
            "out.mp4",
        ]);
        assert_eq!(
            rewrite_h264_args(&args, "h264_mf", 1280 * 720),
            strings(&[
                "-y",
                "-i",
                "in.mp4",
                "-c:v",
                "h264_mf",
                "-b:v",
                "2764800",
                "-c:a",
                "aac",
                "-movflags",
                "+faststart",
                "out.mp4"
            ])
        );
        let round = strings(&[
            "-map", "[out]", "-c:v", "libx264", "-preset", "veryfast", "-crf", "24",
        ]);
        assert_eq!(
            rewrite_h264_args(&round, "h264_videotoolbox", 400 * 400),
            strings(&[
                "-map",
                "[out]",
                "-c:v",
                "h264_videotoolbox",
                "-b:v",
                "500000"
            ])
        );
    }

    #[test]
    fn x264_options_are_dropped_wherever_they_sit() {
        let args = strings(&[
            "-c:v",
            "libx264",
            "-pix_fmt",
            "yuv420p",
            "-preset",
            "veryfast",
            "-crf",
            "23",
            "-shortest",
            "out.mp4",
        ]);
        assert_eq!(
            rewrite_h264_args(&args, "h264_mf", 1280 * 720),
            strings(&[
                "-c:v",
                "h264_mf",
                "-b:v",
                "2764800",
                "-pix_fmt",
                "yuv420p",
                "-shortest",
                "out.mp4"
            ])
        );
        let vp9 = strings(&["-c:v", "libvpx-vp9", "-crf", "30", "out.webm"]);
        assert_eq!(rewrite_h264_args(&vp9, "h264_mf", 1), vp9);
    }

    #[test]
    fn a_bitrate_the_command_line_sets_is_kept() {
        let avatar = strings(&[
            "-c:v", "libx264", "-b:v", "1200k", "-maxrate", "1500k", "out.mp4",
        ]);
        assert_eq!(
            rewrite_h264_args(&avatar, "h264_mf", 800 * 800),
            strings(&[
                "-c:v", "h264_mf", "-b:v", "1200k", "-maxrate", "1500k", "out.mp4"
            ])
        );
    }

    #[test]
    fn the_bitrate_stays_in_a_sane_range() {
        assert_eq!(
            h264_codec_args("h264_mf", 0).get(3).map(String::as_str),
            Some("500000")
        );
        assert_eq!(
            h264_codec_args("h264_mf", 3840 * 2160)
                .get(3)
                .map(String::as_str),
            Some("8000000")
        );
    }
}
