use discord_rich_presence::{activity, DiscordIpc, DiscordIpcClient};
use std::io::{stdout, Write};
use std::process::Command;
use aniverse::AniVerseCli;
use colored::Colorize;
use ext::Ext;
use tokio::runtime;

use crate::{input::{get_user_input, load_history, save_history, pick_provider, load_watch_history, delete_watch_entry_by_ids, delete_watch_entries_for_movie, delete_search_history_entry, clear_watch_history, clear_search_history, group_watch_history}, util::clearscreen_and_show_banner};

mod aniverse;
mod ext;
mod input;
mod models;
mod proxy;
mod util;
mod live_input;
mod tracks;

const DISCORD_APP_ID: &str = "1549897147577667784";
const DEFAULT_DISCORD_LOGO: &str = "logoaniverse";
fn idlix_audio_path() -> String {
    crate::util::temp_file("aniverse_idlix_audio.m3u8")
}

fn idlix_subtitle_path() -> String {
    crate::util::temp_file("aniverse_idlix.vtt")
}

fn mpv_track_script_path() -> String {
    crate::util::temp_file("aniverse_track.lua")
}

fn mpv_last_pos_file_path() -> String {
    crate::util::temp_file("aniverse_last_pos.txt")
}

pub const PROVIDER_IDLIX: usize = 0;
pub const PROVIDER_OTAKUDESU: usize = 1;
pub const PROVIDER_SAMEHADAKU: usize = 2;
pub const PROVIDER_KUSONIME: usize = 3;

fn provider_name(provider_type: usize) -> &'static str {
    match provider_type {
        PROVIDER_IDLIX => "Idlix",
        PROVIDER_OTAKUDESU => "Otakudesu",
        PROVIDER_SAMEHADAKU => "Samehadaku",
        PROVIDER_KUSONIME => "Kusonime",
        _ => "Tidak dikenal",
    }
}

const REPO_URL: &str = "https://github.com/yuutaa7x/AniVerse-CLI";
const MENU_UPDATE: usize = 0;
const MENU_WATCH: usize = 1;
const MENU_HEALTH: usize = 2;
const MENU_EXIT: usize = 3;

fn get_ext_by_provider(provider_type: usize) -> Box<dyn Ext> {
    match provider_type {
        PROVIDER_IDLIX => Box::new(ext::idlix::Idlix::new()),
        PROVIDER_OTAKUDESU => Box::new(ext::otakudesu::Otakudesu::new()),
        PROVIDER_SAMEHADAKU => Box::new(ext::samehadaku::Samehadaku::new()),
        PROVIDER_KUSONIME => Box::new(ext::kusonime::Kusonime::new()),
        _ => Box::new(ext::idlix::Idlix::new()),
    }
}

fn get_ext(input: &crate::models::Input) -> Box<dyn Ext> {
    get_ext_by_provider(input.tipe)
}

fn check_command_exists(cmd: &str) -> bool {
    let p = std::path::Path::new(cmd);
    if p.is_file() {
        return true;
    }

    if let Some(paths) = std::env::var_os("PATH") {
        let extensions: Vec<String> = if cfg!(target_os = "windows") {
            let pathext = std::env::var("PATHEXT").unwrap_or_else(|_| ".EXE;.CMD;.BAT;.COM".to_string());
            pathext.split(';').map(|s| s.to_string()).collect()
        } else {
            vec!["".to_string()]
        };

        for dir in std::env::split_paths(&paths) {
            let direct = dir.join(cmd);
            if direct.is_file() {
                return true;
            }
            if cfg!(target_os = "windows") {
                for ext in &extensions {
                    let with_ext = dir.join(format!("{}{}", cmd, ext));
                    if with_ext.is_file() {
                        return true;
                    }
                }
            }
        }
    }

    let checker = if cfg!(target_os = "windows") { "where" } else { "which" };
    if let Ok(status) = Command::new(checker)
        .arg(cmd)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
    {
        if status.success() {
            return true;
        }
    }

    if cfg!(target_os = "windows") && check_windows_known_paths(cmd) {
        return true;
    }

    false
}

fn check_windows_known_paths(cmd: &str) -> bool {
    let cmd_exe = if cmd.ends_with(".exe") {
        cmd.to_string()
    } else {
        format!("{}.exe", cmd)
    };

    // Kandidat path harus spesifik untuk `cmd` yang dicari. Path milik aplikasi lain
    // (mis. mpv/vlc) hanya ditambahkan ketika `cmd` memang aplikasi tersebut,
    // agar check_command_exists("foo") tidak ikut menemukan binary yang salah.
    let mut candidates: Vec<std::path::PathBuf> = Vec::new();

    if let Ok(local_app_data) = std::env::var("LOCALAPPDATA") {
        candidates.push(
            std::path::Path::new(&local_app_data)
                .join("Microsoft")
                .join("WindowsApps")
                .join(&cmd_exe),
        );
    }

    if let Ok(prog_files) = std::env::var("ProgramFiles") {
        candidates.push(std::path::Path::new(&prog_files).join(cmd).join(&cmd_exe));
        match cmd {
            "mpv" => {
                candidates.push(std::path::Path::new(&prog_files).join("mpv").join("mpv.exe"));
                candidates.push(
                    std::path::Path::new(&prog_files).join("MPV Player").join("mpv.exe"),
                );
                candidates.push(
                    std::path::Path::new(&prog_files).join("mpv.net").join("mpvnet.exe"),
                );
                candidates.push(std::path::PathBuf::from(r"C:\mpv\mpv.exe"));
            }
            "vlc" => {
                candidates
                    .push(std::path::Path::new(&prog_files).join("VideoLAN").join("VLC").join("vlc.exe"));
            }
            _ => {}
        }
    }

    if let Ok(prog_files_x86) = std::env::var("ProgramFiles(x86)") {
        candidates.push(std::path::Path::new(&prog_files_x86).join(cmd).join(&cmd_exe));
        match cmd {
            "mpv" => candidates
                .push(std::path::Path::new(&prog_files_x86).join("mpv").join("mpv.exe")),
            "vlc" => candidates.push(
                std::path::Path::new(&prog_files_x86).join("VideoLAN").join("VLC").join("vlc.exe"),
            ),
            _ => {}
        }
    }

    if let Ok(prog_data) = std::env::var("ProgramData") {
        candidates
            .push(std::path::Path::new(&prog_data).join("chocolatey").join("bin").join(&cmd_exe));
    }

    if let Some(home) = dirs::home_dir() {
        candidates.push(home.join("scoop").join("shims").join(&cmd_exe));
        candidates.push(home.join("scoop").join("shims").join(format!("{}.cmd", cmd)));
        candidates.push(home.join("scoop").join("shims").join(format!("{}.ps1", cmd)));
        candidates
            .push(home.join("scoop").join("apps").join(cmd).join("current").join(&cmd_exe));
    }

    candidates.iter().any(|p| p.exists())
}

fn resolve_command_path(cmd: &str) -> String {
    if !cfg!(target_os = "windows") {
        return cmd.to_string();
    }

    let cmd_exe = if cmd.ends_with(".exe") {
        cmd.to_string()
    } else {
        format!("{}.exe", cmd)
    };

    if let Ok(local_app_data) = std::env::var("LOCALAPPDATA") {
        let p = std::path::Path::new(&local_app_data).join("Microsoft").join("WindowsApps").join(&cmd_exe);
        if p.exists() {
            return p.to_string_lossy().to_string();
        }
    }

    if let Ok(prog_files) = std::env::var("ProgramFiles") {
        let p = std::path::Path::new(&prog_files).join(cmd).join(&cmd_exe);
        if p.exists() {
            return p.to_string_lossy().to_string();
        }
        if cmd == "mpv" {
            let p_mpv = std::path::Path::new(&prog_files).join("mpv").join("mpv.exe");
            if p_mpv.exists() {
                return p_mpv.to_string_lossy().to_string();
            }
            let p_mpv2 = std::path::Path::new(&prog_files).join("MPV Player").join("mpv.exe");
            if p_mpv2.exists() {
                return p_mpv2.to_string_lossy().to_string();
            }
            let p_mpvnet = std::path::Path::new(&prog_files).join("mpv.net").join("mpvnet.exe");
            if p_mpvnet.exists() {
                return p_mpvnet.to_string_lossy().to_string();
            }
        }
        if cmd == "vlc" {
            let pv = std::path::Path::new(&prog_files).join("VideoLAN").join("VLC").join("vlc.exe");
            if pv.exists() {
                return pv.to_string_lossy().to_string();
            }
        }
    }

    if let Some(home) = dirs::home_dir() {
        let scoop = home.join("scoop").join("shims").join(&cmd_exe);
        if scoop.exists() {
            return scoop.to_string_lossy().to_string();
        }
        let choco = std::path::PathBuf::from(r"C:\ProgramData\chocolatey\bin").join(&cmd_exe);
        if choco.exists() {
            return choco.to_string_lossy().to_string();
        }
        let c_mpv = std::path::PathBuf::from(r"C:\mpv\mpv.exe");
        if cmd == "mpv" && c_mpv.exists() {
            return c_mpv.to_string_lossy().to_string();
        }
    }

    cmd.to_string()
}

fn install_dependencies(package_manager: &str) {
    println!("\n{} Menginstall dependensi menggunakan {}...", "◆".blue(), package_manager);
    match package_manager {
        "apt-get" => {
            let _ = Command::new("sudo")
                .args(["apt-get", "install", "-y", "mpv", "yt-dlp"])
                .status();
        },
        "pacman" => {
            let _ = Command::new("sudo")
                .args(["pacman", "-S", "--noconfirm", "mpv", "yt-dlp"])
                .status();
        },
        "dnf" => {
            let _ = Command::new("sudo")
                .args(["dnf", "install", "-y", "mpv", "yt-dlp"])
                .status();
        },
        "zypper" => {
            let _ = Command::new("sudo")
                .args(["zypper", "install", "-y", "mpv", "yt-dlp"])
                .status();
        },
        "apk" => {
            let _ = Command::new("sudo")
                .args(["apk", "add", "mpv", "yt-dlp"])
                .status();
        },
        "pkg" => {
            // Termux (Android) — tidak butuh sudo
            println!("  {} Menjalankan: pkg install mpv yt-dlp ...", "ℹ".yellow());
            let _ = Command::new("pkg")
                .args(["install", "-y", "mpv", "yt-dlp"])
                .status();
        },
        "brew" => {
            let _ = Command::new("brew")
                .args(["install", "mpv", "yt-dlp"])
                .status();
        },
        "winget" => {
            let winget_bin = resolve_command_path("winget");
            println!("  {} Menjalankan: winget install --id shinchiro.mpv --source winget ...", "ℹ".yellow());
            let status1 = Command::new(&winget_bin)
                .args(["install", "--id", "shinchiro.mpv", "-e", "--source", "winget", "--accept-source-agreements", "--accept-package-agreements"])
                .status();

            let mpv_ok = match status1 {
                Ok(s) if s.success() => true,
                _ => {
                    println!("  {} Mencoba alternatif: winget install --id mpv.net --source winget ...", "ℹ".yellow());
                    let s2 = Command::new(&winget_bin)
                        .args(["install", "--id", "mpv.net", "-e", "--source", "winget", "--accept-source-agreements", "--accept-package-agreements"])
                        .status();
                    s2.map(|s| s.success()).unwrap_or(false)
                }
            };

            if mpv_ok {
                println!("  {} MPV berhasil diinstal!", "✓".green());
            } else {
                eprintln!("  {} Gagal menginstal MPV via winget.", "✖".red());
            }

            println!("  {} Menjalankan: winget install --id yt-dlp.yt-dlp --source winget ...", "ℹ".yellow());
            let _ = Command::new(&winget_bin)
                .args(["install", "--id", "yt-dlp.yt-dlp", "-e", "--source", "winget", "--accept-source-agreements", "--accept-package-agreements"])
                .status();
        },
        "choco" => {
            let choco_bin = resolve_command_path("choco");
            println!("  {} Menjalankan: choco install -y mpv yt-dlp ...", "ℹ".yellow());
            let _ = Command::new(&choco_bin)
                .args(["install", "-y", "mpv", "yt-dlp"])
                .status();
        },
        "scoop" => {
            let scoop_bin = resolve_command_path("scoop");
            println!("  {} Menjalankan: scoop install mpv yt-dlp ...", "ℹ".yellow());
            let _ = Command::new(&scoop_bin)
                .args(["install", "mpv", "yt-dlp"])
                .status();
        },
        _ => {},
    }
}

fn detect_package_manager() -> &'static str {
    if cfg!(target_os = "windows") {
        if check_command_exists("winget") { return "winget"; }
        if check_command_exists("choco") { return "choco"; }
        if check_command_exists("scoop") { return "scoop"; }
    } else if cfg!(target_os = "macos") {
        if check_command_exists("brew") { return "brew"; }
    } else {
        // Termux (Android) uses pkg; check before apt-get since Termux also ships apt-get
        if check_command_exists("pkg") && std::env::var("TERMUX_VERSION").is_ok() {
            return "pkg";
        }
        if check_command_exists("apt-get") { return "apt-get"; }
        if check_command_exists("pacman") { return "pacman"; }
        if check_command_exists("dnf") { return "dnf"; }
        if check_command_exists("zypper") { return "zypper"; }
        if check_command_exists("apk") { return "apk"; }
    }
    "unknown"
}

fn show_health_screen() -> anyhow::Result<bool> {
    loop {
        clearscreen_and_show_banner()?;
        
        // Kartu status (lebar 50 kolom)
        println!(
            "  {} {} {}",
            "┌─".bright_black(),
            "✦ STATUS SISTEM".bright_magenta().bold(),
            "─".repeat(31).bright_black()
        );
        fn badge(label: &str, tone: u8) -> String {
            let inner = format!("[ {:^11} ]", label);
            match tone {
                0 => inner.bright_green().to_string(),
                1 => inner.bright_red().to_string(),
                2 => inner.bright_yellow().to_string(),
                _ => inner.bright_cyan().to_string(),
            }
        }
        fn health_row(name: &str, badge: &str) {
            println!(
                "  {}  {:<28} {} {}",
                "│".bright_black(),
                name,
                badge,
                "│".bright_black()
            );
        }

        let has_mpv = check_command_exists("mpv");
        health_row("MPV Video Player", &badge(if has_mpv { "TERPASANG" } else { "TIDAK ADA" }, if has_mpv { 0 } else { 1 }));
        if !has_mpv {
            println!(
                "  {}  {} {}",
                "│".bright_black(),
                " ".repeat(28),
                "» nonton via browser saja".bright_yellow()
            );
        }

        let has_ytdlp = check_command_exists("yt-dlp") || check_command_exists("youtube-dl");
        health_row("yt-dlp / youtube-dl", &badge(if has_ytdlp { "TERPASANG" } else { "OPSIONAL" }, if has_ytdlp { 0 } else { 2 }));
        if !has_ytdlp {
            println!(
                "  {}  {} {}",
                "│".bright_black(),
                " ".repeat(28),
                "» fallback & download nonaktif".bright_black()
            );
        }

        let has_curl = check_command_exists("curl");
        health_row("curl · wajib untuk Idlix", &badge(if has_curl { "TERPASANG" } else { "TIDAK ADA" }, if has_curl { 0 } else { 1 }));
        if !has_curl {
            println!(
                "  {}  {} {}",
                "│".bright_black(),
                " ".repeat(28),
                "» provider Idlix nonaktif".bright_red()
            );
        }

        health_row("Scraper Rust bawaan", &badge("BAWAAN", 3));
        println!(
            "  {}{}",
            "└".bright_black(),
            "─".repeat(48).bright_black()
        );
        println!();

        if has_mpv && has_curl {
            println!(
                "  {} {}",
                "✦".bright_green(),
                "Semua sistem siap — selamat menonton!".bright_green()
            );
        } else {
            println!(
                "  {} {}",
                "⚠".bright_yellow(),
                "Sebagian fitur terbatas — lihat catatan di atas.".bright_yellow()
            );
        }
        println!();

        println!(
            "  {} {}",
            "[ 1 ]".bright_cyan(),
            "Install dependensi otomatis (MPV + yt-dlp)".bright_white()
        );
        if has_mpv {
            println!(
                "  {} {}",
                "[ 2 ]".bright_cyan(),
                "Masuk ke AniVerse-CLI (Nonton Anime / Film)".bright_white()
            );
        } else {
            println!(
                "  {} {}",
                "[ 2 ]".bright_black(),
                "Masuk ke AniVerse-CLI (Nonton — Browser Saja)".bright_black()
            );
        }
        println!("  {} {}", "[ q ]".bright_cyan(), "Keluar".bright_white());
        println!();

        print!("  {} ", "❯".bright_magenta());
        stdout().flush()?;
        
        let term = dialoguer::console::Term::stdout();
        let choice = term.read_char()?;
        println!("{}", choice);
        
        if choice == 'q' || choice == 'Q' {
            std::process::exit(0);
        } else if choice == '2' || choice == '\n' {
            return Ok(true);
        } else if choice == '1' {
            let package_manager = detect_package_manager();
            if package_manager == "unknown" {
                if cfg!(target_os = "windows") {
                    println!("\n{} Tidak ditemukan package manager otomatis (winget / choco / scoop).", "✖".red());
                    println!("  Silahkan pilih salah satu opsi instalasi MPV untuk Windows:");
                    println!("  1. Jalankan di PowerShell / CMD:");
                    println!("     {}", "winget install --id shinchiro.mpv --source winget".cyan());
                    println!("  2. Atau install via Scoop:");
                    println!("     {}", "scoop install mpv".cyan());
                    println!("  3. Atau unduh manual installer MPV di:");
                    println!("     {}", "https://mpv.io/installation/".cyan());
                    println!("\n  Ingin membuka halaman unduhan MPV di browser? (y/n)");
                    let mut answer = String::new();
                    if std::io::stdin().read_line(&mut answer).is_ok() && answer.trim().eq_ignore_ascii_case("y") {
                        let _ = open::that("https://mpv.io/installation/");
                    }
                } else {
                    println!("\n{} Package manager tidak terdeteksi! Silahkan install mpv dan yt-dlp secara manual.", "✖".red());
                }
            } else {
                install_dependencies(package_manager);
            }
            
            println!("\n  {} Tekan Enter untuk memeriksa kembali status dependensi.", "✓".green());
            let mut dummy = String::new();
            std::io::stdin().read_line(&mut dummy)?;
        }
    }
}

fn clean_title(title: &str) -> String {
    let mut cleaned = title.to_string();
    cleaned = cleaned.replace("[Movie]", "");
    cleaned = cleaned.replace("[Series]", "");
    cleaned = cleaned.replace("Subtitle Indonesia", "");
    cleaned = cleaned.replace("Sub Indo", "");
    cleaned = cleaned.replace("subtitle indonesia", "");
    cleaned = cleaned.replace("sub indo", "");
    
    let re_season = regex::Regex::new(r"(?i)\bSeason\s*\d+\b").unwrap();
    cleaned = re_season.replace_all(&cleaned, "").to_string();
    
    let re_s = regex::Regex::new(r"(?i)\bS\d+\b").unwrap();
    cleaned = re_s.replace_all(&cleaned, "").to_string();
    
    let re_ep_range = regex::Regex::new(r"(?i)\(\s*episode\s*.*?\)").unwrap();
    cleaned = re_ep_range.replace_all(&cleaned, "").to_string();
    
    let re_batch = regex::Regex::new(r"(?i)\(\s*batch\s*\)").unwrap();
    cleaned = re_batch.replace_all(&cleaned, "").to_string();

    let re_spaces = regex::Regex::new(r"\s+").unwrap();
    cleaned = re_spaces.replace_all(&cleaned, " ").to_string();
    
    let re_hyphen = regex::Regex::new(r"(?i)\s*-\s*$").unwrap();
    cleaned = re_hyphen.replace_all(&cleaned, "").to_string();
    
    let re_brackets = regex::Regex::new(r"\(\s*\)").unwrap();
    cleaned = re_brackets.replace_all(&cleaned, "").to_string();
    
    cleaned.trim().to_string()
}

fn detect_season(title: &str, img_url: &str, meta_data: &[(String, String)]) -> Option<u32> {
    let re_season = regex::Regex::new(r"(?i)(?:season|s)\s*-?\s*(\d+)").unwrap();
    let re_ordinal = regex::Regex::new(r"(?i)(\d+)(?:st|nd|rd|th)\s+season").unwrap();
    let re_roman = regex::Regex::new(r"(?i)\b(II|III|IV|V|VI|VII|VIII|IX|X)\b").unwrap();

    let parse_text = |text: &str| -> Option<u32> {
        if let Some(caps) = re_season.captures(text) {
            if let Ok(num) = caps[1].parse::<u32>() { return Some(num); }
        }
        if let Some(caps) = re_ordinal.captures(text) {
            if let Ok(num) = caps[1].parse::<u32>() { return Some(num); }
        }
        if let Some(caps) = re_roman.captures(text) {
            let roman = caps[1].to_uppercase();
            let num = match roman.as_str() {
                "II" => 2, "III" => 3, "IV" => 4, "V" => 5, "VI" => 6,
                "VII" => 7, "VIII" => 8, "IX" => 9, "X" => 10, _ => 1,
            };
            return Some(num);
        }
        None
    };

    if let Some(s) = parse_text(title) { return Some(s); }
    if let Some(s) = parse_text(img_url) { return Some(s); }
    
    for (_meta_key, meta_value) in meta_data {
        if let Some(s) = parse_text(meta_value) { return Some(s); }
    }

    None
}

fn format_episode(title: &str, is_series: bool, season_num: u32) -> String {
    let trimmed = title.trim();
    if !is_series {
        return "Full Movie".to_string();
    }
    
    let re_episode = regex::Regex::new(r"(?i)episode\s*(\d+)").unwrap();
    if let Some(caps) = re_episode.captures(trimmed) {
        if let Some(num) = caps.get(1) {
            let ep_num: u32 = num.as_str().parse().unwrap_or(1);
            return if season_num > 1 {
                format!("Season {} • Episode {}", season_num, ep_num)
            } else {
                format!("Episode {}", ep_num)
            };
        }
    }

    let re_season_ep = regex::Regex::new(r"(?i)s(\d+)e(\d+)").unwrap();
    if let Some(caps) = re_season_ep.captures(trimmed) {
        let season: u32 = caps[1].parse().unwrap_or(season_num);
        let ep: u32 = caps[2].parse().unwrap_or(1);
        return if season > 1 {
            format!("Season {} • Episode {}", season, ep)
        } else {
            format!("Episode {}", ep)
        };
    }
    
    if trimmed.chars().all(|c| c.is_ascii_digit()) {
        return if season_num > 1 {
            format!("Season {} • Episode {}", season_num, trimmed)
        } else {
            format!("Episode {}", trimmed)
        };
    }
    
    if season_num > 1 {
        format!("Season {} • {}", season_num, trimmed)
    } else {
        trimmed.to_string()
    }
}

fn update_discord_status(
    discord: &mut Option<DiscordIpcClient>,
    movie_title: &str,
    episode_title: &str,
    is_series: bool,
    season_num: u32,
    thumb_url: Option<String>,
) {
    let drpc = match discord {
        Some(ref mut client) => client,
        None => return,
    };

    let mut large_image = thumb_url.unwrap_or_else(|| DEFAULT_DISCORD_LOGO.to_string());
    if large_image.contains("otakudesu") {
        large_image = DEFAULT_DISCORD_LOGO.to_string();
    }

    let clean_movie = clean_title(movie_title);
    let clean_ep = clean_title(episode_title);

    let mut assets = activity::Assets::new()
        .large_image(&large_image)
        .large_text(&clean_movie);

    if large_image != DEFAULT_DISCORD_LOGO {
        assets = assets.small_image(DEFAULT_DISCORD_LOGO).small_text("AniVerse-CLI");
    }

    let current_timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;

    let _ = drpc.set_activity(
        activity::Activity::new()
            .details(&clean_movie)
            .state(format_episode(&clean_ep, is_series, season_num))
            .assets(assets)
            .timestamps(activity::Timestamps::new().start(current_timestamp))
            .activity_type(activity::ActivityType::Watching),
    );
}

fn reset_discord_status(discord: &mut Option<DiscordIpcClient>) {
    if let Some(ref mut drpc) = discord {
        let _ = drpc.set_activity(
            activity::Activity::new()
                .state("Memilih Anime...")
                .details("AniVerse-CLI")
                .activity_type(activity::ActivityType::Watching),
        );
    }
}

/// Hasil dari percobaan memutar stream.
#[derive(Debug, Clone, PartialEq)]
enum PlayOutcome {
    /// Pemutar berjalan normal; berisi posisi terakhir dalam detik.
    Played(u64),
    /// Pemutar gagal membuka stream (user perlu memilih server/pemutar lain).
    Failed,
    /// User memilih Kembali / membatalkan aksi.
    Cancelled,
}

/// Parameter pemutaran yang dibutuhkan pemutar eksternal.
struct PlayRequest<'a> {
    stream_url: &'a str,
    provider_type: usize,
    start_seconds: u64,
    movie_title: &'a str,
    episode_title: &'a str,
    sub_url: Option<&'a str>,
    audio_index: Option<u32>,
}

/// Hasil satu percobaan menjalankan MPV.
struct MpvAttempt {
    ok: bool,
    exit_code: Option<i32>,
    pos: u64,
    log_path: String,
    spawn_failed: bool,
}

/// Hasil resolve ulang URL lewat yt-dlp.
enum YtDlpResolve {
    Url(String),
    Failed(String),
    NotFound,
}

fn wait_enter() {
    println!("  Tekan Enter untuk kembali...");
    let mut dummy = String::new();
    let _ = std::io::stdin().read_line(&mut dummy);
}

/// Bersihkan string agar aman dipakai sebagai nama file di semua OS.
fn sanitize_filename(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    for c in name.chars() {
        match c {
            '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' => out.push(' '),
            c if c.is_control() => out.push(' '),
            c => out.push(c),
        }
    }
    let re = crate::regex!(r"\s+");
    let mut limited: String = re
        .replace_all(&out, " ")
        .trim()
        .to_string()
        .chars()
        .take(120)
        .collect();
    while limited.ends_with(' ') {
        limited.pop();
    }
    if limited.is_empty() {
        "episode".to_string()
    } else {
        limited
    }
}

/// Folder unduhan per judul: `~/Downloads/AniVerse/<judul>/`.
fn download_dir_for(movie_title: &str) -> std::path::PathBuf {
    let base = dirs::download_dir()
        .or_else(dirs::home_dir)
        .unwrap_or_else(std::env::temp_dir);
    base.join("AniVerse").join(sanitize_filename(movie_title))
}

/// Referer per provider untuk yt-dlp / unduhan / proxy.
fn referer_for_provider(provider_type: usize) -> Option<&'static str> {
    match provider_type {
        PROVIDER_OTAKUDESU => Some("https://desustream.net/"),
        PROVIDER_IDLIX => Some("https://z2.idlixku.com/"),
        PROVIDER_SAMEHADAKU => Some("https://samehadaku.li/"),
        PROVIDER_KUSONIME => Some("https://kusonime.com/"),
        _ => None,
    }
}

/// Coba resolve URL stream yang bisa diputar langsung lewat `yt-dlp -g`.
fn resolve_stream_via_ytdlp(url: &str, provider_type: usize) -> YtDlpResolve {
    let ytdlp_bin = if check_command_exists("yt-dlp") {
        resolve_command_path("yt-dlp")
    } else if check_command_exists("youtube-dl") {
        resolve_command_path("youtube-dl")
    } else {
        return YtDlpResolve::NotFound;
    };
    let mut cmd = Command::new(&ytdlp_bin);
    cmd.args(["-g", "-f", "bv*+ba/b/best"]);
    if let Some(referer) = referer_for_provider(provider_type) {
        cmd.args(["--add-header", &format!("Referer:{}", referer)]);
    }
    let out = match cmd.arg(url).output() {
        Ok(o) => o,
        Err(e) => return YtDlpResolve::Failed(format!("Gagal menjalankan yt-dlp: {}", e)),
    };
    let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
    if !out.status.success() {
        return YtDlpResolve::Failed(if stderr.is_empty() {
            format!("yt-dlp exit code {:?}", out.status.code())
        } else {
            stderr
        });
    }
    let stdout = String::from_utf8_lossy(&out.stdout);
    match stdout.lines().map(str::trim).find(|l| l.starts_with("http")) {
        Some(u) => YtDlpResolve::Url(u.to_string()),
        None => YtDlpResolve::Failed("yt-dlp tidak menghasilkan URL".to_string()),
    }
}

/// Jalankan MPV sekali dan laporkan hasilnya secara terstruktur.
fn run_mpv(
    stream_url: &str,
    provider_type: usize,
    start_seconds: u64,
    sub_url: Option<&str>,
    audio_index: Option<u32>,
) -> MpvAttempt {
    let track_script = mpv_track_script_path();
    let last_pos_file = mpv_last_pos_file_path();
    let mpv_log_file = crate::util::temp_file("aniverse_mpv.log");

    let lua_script = format!(
        r#"
local last_pos = {}
mp.observe_property("time-pos", "number", function(name, val)
    if val then last_pos = val end
end)
local function save()
    local f = io.open("{}", "w")
    if f then
        f:write(tostring(math.floor(last_pos)))
        f:close()
    end
end
mp.add_periodic_timer(2, save)
mp.register_event("shutdown", save)
"#,
        start_seconds, last_pos_file
    );
    let _ = std::fs::write(&track_script, lua_script);
    let _ = std::fs::remove_file(&last_pos_file);
    let _ = std::fs::remove_file(&mpv_log_file);

    let mpv_bin = resolve_command_path("mpv");
    let mut cmd = Command::new(&mpv_bin);

    if std::env::var("TERMUX_VERSION").is_ok() {
        // Cek apakah termux-x11 sudah berjalan
        if let Ok(out) = Command::new("pgrep").arg("termux-x11").output() {
            if out.stdout.is_empty() {
                println!("  {} Menjalankan Termux-X11 server otomatis...", "ℹ".yellow());
                let _ = Command::new("termux-x11").arg(":0").spawn();
                std::thread::sleep(std::time::Duration::from_millis(2000));
            }
        }

        cmd.env("DISPLAY", ":0");
        cmd.arg("--vo=x11"); // Pastikan pakai x11 untuk mencegah error gpu-context
    }

    cmd.arg("--hwdec=auto-safe");
    cmd.arg("--cache=yes");
    cmd.arg(format!("--script={}", track_script));

    if start_seconds > 0 {
        cmd.arg(format!("--start={}", start_seconds));
    }

    if let Some(s) = sub_url {
        cmd.arg(format!("--sub-file={}", s));
    }
    if let Some(a) = audio_index {
        cmd.arg(format!("--aid={}", a));
    }

    if provider_type == PROVIDER_IDLIX {
        cmd.arg("--demuxer=lavf");
        cmd.arg("--demuxer-lavf-o-append=protocol_whitelist=file,http,https,tcp,tls,crypto,data");
        cmd.arg("--demuxer-lavf-o-append=http_persistent=0");
        cmd.arg("--demuxer-lavf-o-append=reconnect=1");
        cmd.arg("--demuxer-lavf-o-append=reconnect_streamed=1");
        cmd.arg("--demuxer-lavf-o-append=reconnect_on_network_error=1");
        cmd.arg("--demuxer-lavf-o-append=reconnect_delay_max=5");
        cmd.arg("--demuxer-max-bytes=150M");
        cmd.arg("--demuxer-readahead-secs=30");
        cmd.arg("--ytdl=no");
        cmd.arg("--msg-level=ffmpeg/demuxer=error");
        let audio_file = idlix_audio_path();
        if std::path::Path::new(&audio_file).exists() {
            cmd.arg(format!("--audio-file={}", audio_file));
        }
        let sub_file = idlix_subtitle_path();
        if std::path::Path::new(&sub_file).exists() {
            cmd.arg(format!("--sub-file={}", sub_file));
        }
    } else if let Some(referer) = referer_for_provider(provider_type) {
        cmd.arg(format!("--http-header-fields=Referer: {}", referer));
        cmd.arg("--user-agent=Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/110.0.0.0 Safari/537.36");
    }
    cmd.arg(format!("--log-file={}", mpv_log_file));
    let status_result = cmd.arg(stream_url).status();

    let mut pos = start_seconds;
    if let Ok(saved) = std::fs::read_to_string(&last_pos_file) {
        if let Ok(p) = saved.trim().parse::<u64>() {
            pos = p;
        }
    }
    let _ = std::fs::remove_file(&last_pos_file);
    let _ = std::fs::remove_file(&track_script);

    match status_result {
        Ok(status) => MpvAttempt {
            ok: status.success(),
            exit_code: status.code(),
            pos,
            log_path: mpv_log_file,
            spawn_failed: false,
        },
        Err(_) => MpvAttempt {
            ok: false,
            exit_code: None,
            pos,
            log_path: mpv_log_file,
            spawn_failed: true,
        },
    }
}

fn print_mpv_error_panel(
    attempt: &MpvAttempt,
    mpv_bin: &str,
    stream_url: &str,
    ytdlp_note: Option<&str>,
) {
    eprintln!(
        "{} Gagal memutar video di MPV (exit code {}).",
        "✖".red(),
        attempt.exit_code.unwrap_or(-1)
    );
    eprintln!("  {} Binary : {}", "»".bright_yellow(), mpv_bin);
    eprintln!("  {} Stream : {}", "»".bright_yellow(), stream_url);
    if let Some(note) = ytdlp_note {
        eprintln!("  {} yt-dlp : {}", "»".bright_yellow(), note);
    }
    if let Ok(log) = std::fs::read_to_string(&attempt.log_path) {
        // Prioritaskan baris error/warning; abaikan spam debug/verbose.
        let important: Vec<&str> = log
            .lines()
            .filter(|l| l.contains("[e][") || l.contains("[w]["))
            .collect();
        let fallback: Vec<&str> = log
            .lines()
            .filter(|l| !l.contains("[d][") && !l.contains("[v]["))
            .collect();
        let source = if important.is_empty() { &fallback } else { &important };
        let take = 12.min(source.len());
        if take > 0 {
            eprintln!("  {} Pesan mpv:", "»".bright_yellow());
            for line in &source[source.len() - take..] {
                eprintln!("      {}", line);
            }
        }
        eprintln!("  {} Log lengkap: {}", "»".bright_yellow(), attempt.log_path);
    }
}

/// Jalankan unduhan lewat yt-dlp (progress tampil langsung di terminal).
fn download_with_ytdlp(req: &PlayRequest) -> anyhow::Result<PlayOutcome> {
    let ytdlp_bin = if check_command_exists("yt-dlp") {
        resolve_command_path("yt-dlp")
    } else if check_command_exists("youtube-dl") {
        resolve_command_path("youtube-dl")
    } else {
        eprintln!("{} yt-dlp tidak ditemukan di sistem.", "✖".red());
        println!("  Install dulu, contoh:");
        println!("    {}", "winget install yt-dlp.yt-dlp".cyan());
        println!("    {}", "scoop install yt-dlp".cyan());
        wait_enter();
        return Ok(PlayOutcome::Cancelled);
    };

    let dir = download_dir_for(req.movie_title);
    if let Err(e) = std::fs::create_dir_all(&dir) {
        eprintln!("{} Gagal membuat folder {}: {}", "✖".red(), dir.display(), e);
        wait_enter();
        return Ok(PlayOutcome::Cancelled);
    }
    let out_template = dir.join(format!("{}.%(ext)s", sanitize_filename(req.episode_title)));

    println!(
        "{} Mengunduh '{}' ...",
        "◆".blue(),
        req.episode_title.trim().green()
    );
    println!("  {} Target : {}", "»".bright_yellow(), dir.display());

    let mut cmd = Command::new(&ytdlp_bin);
    cmd.args(["--no-playlist", "--merge-output-format", "mp4"]);
    cmd.args(["-o", &out_template.to_string_lossy()]);
    if let Some(referer) = referer_for_provider(req.provider_type) {
        cmd.args(["--add-header", &format!("Referer:{}", referer)]);
    }
    // Inherit stdout/stderr → progress yt-dlp tampil di terminal.
    match cmd.arg(req.stream_url).status() {
        Ok(status) if status.success() => {
            println!("\n  {} Unduhan selesai di: {}", "✓".green(), dir.display());
        }
        Ok(status) => {
            eprintln!(
                "\n  {} Unduhan gagal (exit code {}).",
                "✖".red(),
                status.code().unwrap_or(-1)
            );
        }
        Err(e) => {
            eprintln!("\n  {} Gagal menjalankan yt-dlp: {}", "✖".red(), e);
        }
    }
    wait_enter();
    Ok(PlayOutcome::Cancelled)
}

async fn execute_player(req: PlayRequest<'_>) -> anyhow::Result<PlayOutcome> {
    let stream_url = req.stream_url;
    let provider_type = req.provider_type;
    let start_seconds = req.start_seconds;

    println!("{} Membuka tautan diaplikasi eksternal .. ", "◆".blue());
    stdout().flush()?;

    let is_android = std::env::var("TERMUX_VERSION").is_ok();

    let menu_items: Vec<&str> = if is_android {
        vec![
            "Putar di VLC / Pemutar Android",
            "Buka di Browser",
            "Download dengan yt-dlp",
            "Kembali",
        ]
    } else {
        vec![
            "Putar dengan MPV",
            "Putar dengan VLC",
            "Buka di Browser",
            "Download dengan yt-dlp",
            "Kembali",
        ]
    };

    let player_choice = match dialoguer::Select::with_theme(&crate::util::custom_theme())
        .with_prompt("Pilih Aksi: (Esc untuk kembali)")
        .default(0)
        .items(&menu_items)
        .interact_opt()?
    {
        Some(choice) => choice,
        None => return Ok(PlayOutcome::Cancelled),
    };
    let choice_label = menu_items[player_choice].to_string();

    let mut final_pos = start_seconds;

    match choice_label.as_str() {
        s if s.contains("MPV") => {
            println!("Sedang memutar video di MPV... (Tutup MPV untuk kembali ke menu)");

            let mpv_bin = resolve_command_path("mpv");
            let mut attempt = run_mpv(
                stream_url,
                provider_type,
                start_seconds,
                req.sub_url,
                req.audio_index,
            );
            final_pos = attempt.pos;

            let mut ytdlp_note: Option<String> = None;
            let mut played_url = stream_url.to_string();

            if !attempt.ok && !attempt.spawn_failed {
                match resolve_stream_via_ytdlp(stream_url, provider_type) {
                    YtDlpResolve::Url(resolved) => {
                        println!(
                            "{} MPV gagal; yt-dlp menemukan URL lain. Mencoba putar ulang...",
                            "ℹ".yellow()
                        );
                        attempt = run_mpv(
                            &resolved,
                            provider_type,
                            start_seconds,
                            req.sub_url,
                            req.audio_index,
                        );
                        final_pos = attempt.pos;
                        played_url = resolved;
                    }
                    YtDlpResolve::Failed(err) => {
                        println!("  {} Fallback yt-dlp juga gagal.", "✖".red());
                        ytdlp_note = Some(err);
                    }
                    YtDlpResolve::NotFound => {
                        ytdlp_note =
                            Some("yt-dlp tidak terpasang (fallback nonaktif)".to_string());
                    }
                }
            }

            if attempt.ok {
                Ok(PlayOutcome::Played(final_pos))
            } else if attempt.spawn_failed {
                eprintln!("{} MPV tidak ditemukan!", "✖".red());
                wait_enter();
                Ok(PlayOutcome::Failed)
            } else {
                print_mpv_error_panel(&attempt, &mpv_bin, &played_url, ytdlp_note.as_deref());
                wait_enter();
                Ok(PlayOutcome::Failed)
            }
        },
        s if s.contains("VLC") || s.contains("Android") => {
            if is_android {
                println!("Membuka aplikasi video player bawaan Android (VLC)...");
                
                // Simpan subtitle ke folder Download agar user bisa nge-load manual
                let sub_path = "/sdcard/Download/aniverse_idlix.vtt";
                if std::path::Path::new(sub_path).exists() {
                    println!("{} Subtitle disimpan di folder Download (aniverse_idlix.vtt). Silahkan load manual di VLC.", "ℹ".yellow());
                }

                // Kita nyalakan Local HTTP Proxy untuk memanipulasi Header!
                let proxy_port = crate::proxy::start_proxy(provider_type).await.unwrap_or(8080);
                let encoded_url = urlencoding::encode(stream_url);
                let proxy_url = format!("http://127.0.0.1:{}/?url={}", proxy_port, encoded_url);

                // Buka menggunakan termux-open dengan MIME video/* 
                // Karena kita pakai http://127.0.0.1, VLC tidak butuh Header lagi, Proxy yang urus!
                let status = Command::new("termux-open")
                    .args(["--content-type", "video/*", "--chooser", &proxy_url])
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .status();
                
                if status.is_err() || !status.unwrap().success() {
                    eprintln!("{} Gagal memanggil termux-open. Pastikan VLC sudah di-install.", "✖".red());
                    std::thread::sleep(std::time::Duration::from_secs(3));
                    return Ok(PlayOutcome::Failed);
                }

                println!("Proxy berjalan di port {}. Silahkan tonton di aplikasi pilihanmu!", proxy_port);
                println!("\n{} Jangan tutup layar ini selama menonton agar proxy tetap hidup.", "ℹ".yellow());
                println!("Jika sudah selesai menonton, tekan Enter untuk kembali...");
                let mut dummy = String::new();
                let _ = std::io::stdin().read_line(&mut dummy);
                
                print!("Simpan posisi terakhir tontonan (dalam menit)? (Kosongkan jika sudah tamat): ");
                stdout().flush().unwrap_or(());
                let mut pos_input = String::new();
                let _ = std::io::stdin().read_line(&mut pos_input);
                if let Ok(m) = pos_input.trim().parse::<u64>() {
                    final_pos = m * 60;
                } else if pos_input.trim().is_empty() {
                    final_pos = 0;
                }
                return Ok(PlayOutcome::Played(final_pos));
            }

            println!("Sedang memutar video di VLC... (Tutup VLC untuk kembali ke menu)");
            let vlc_bin = resolve_command_path("vlc");
            let mut cmd = Command::new(&vlc_bin);
            if start_seconds > 0 {
                cmd.arg(format!("--start-time={}", start_seconds));
            }
            if let Some(s) = req.sub_url {
                cmd.arg(format!("--sub-file={}", s));
            }
            if let Some(referer) = referer_for_provider(provider_type) {
                cmd.arg(format!("--http-referrer={}", referer));
                cmd.arg("--http-user-agent=Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/110.0.0.0 Safari/537.36");
            }
            match cmd
                .arg(stream_url)
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status()
            {
                Ok(status) if status.success() => {}
                Ok(status) => {
                    eprintln!(
                        "{} Gagal memutar video di VLC (exit code {}).",
                        "✖".red(),
                        status.code().unwrap_or(-1)
                    );
                    return Ok(PlayOutcome::Failed);
                }
                Err(_) => {
                    eprintln!("{} VLC tidak ditemukan!", "✖".red());
                    return Ok(PlayOutcome::Failed);
                }
            }

            print!("Simpan posisi terakhir tontonan (dalam menit)? (Kosongkan jika sudah tamat): ");
            stdout().flush().unwrap_or(());
            let mut pos_input = String::new();
            let _ = std::io::stdin().read_line(&mut pos_input);
            if let Ok(m) = pos_input.trim().parse::<u64>() {
                final_pos = m * 60;
            } else if pos_input.trim().is_empty() {
                final_pos = 0;
            }
            Ok(PlayOutcome::Played(final_pos))
        },
        s if s.contains("Browser") => {
            if open::that(stream_url).is_ok() {
                if dialoguer::Confirm::with_theme(&crate::util::custom_theme())
                    .with_prompt("Tutup aplikasi?")
                    .interact_opt()?
                    .unwrap_or(false)
                {
                    std::process::exit(0);
                }
                return Ok(PlayOutcome::Played(final_pos));
            }
            eprintln!("{} Gagal membuka browser.", "✖".red());
            Ok(PlayOutcome::Failed)
        },
        s if s.contains("Download") => download_with_ytdlp(&req),
        _ => Ok(PlayOutcome::Cancelled),
    }
}

fn extract_ep_num(title: &str) -> u32 {
    // Prioritas sama dengan format_episode: "Episode 10" -> 10, "S01E05" -> 5.
    // Digit 4-digit (tahun) diabaikan agar "Episode 1 (2024)" tidak menjadi 2024.
    let re_episode = crate::regex!(r"(?i)episode\s*(\d+)");
    if let Some(caps) = re_episode.captures(title) {
        if let Ok(n) = caps[1].parse::<u32>() {
            return n;
        }
    }

    let re_season_ep = crate::regex!(r"(?i)\bs(\d+)e(\d+)");
    if let Some(caps) = re_season_ep.captures(title) {
        if let Ok(n) = caps[2].parse::<u32>() {
            return n;
        }
    }

    let mut last_num = 0u32;
    let mut num_str = String::new();
    for c in title.chars() {
        if c.is_ascii_digit() {
            num_str.push(c);
            continue;
        }
        if !num_str.is_empty() {
            if let Ok(n) = num_str.parse::<u32>() {
                if n < 1000 {
                    last_num = n;
                }
            }
            num_str.clear();
        }
    }
    if !num_str.is_empty() {
        if let Ok(n) = num_str.parse::<u32>() {
            if n < 1000 {
                last_num = n;
            }
        }
    }
    last_num
}

async fn handle_movie_episodes(
    aniverse: &mut AniVerseCli,
    movie: &crate::models::Movie,
    provider_type: usize,
    discord: &mut Option<DiscordIpcClient>,
) -> anyhow::Result<()> {
    let mut cached_season: Option<u32> = None;

    loop {
        clearscreen_and_show_banner()?;
        let mut episode = match aniverse.extract_episode(movie.clone()).await? {
            Some(ep) => ep,
            None => break, // Back to movie search
        };

        loop {
            let season_num = match cached_season {
                Some(s) => s,
                None => {
                    let detected = if episode.is_series {
                        let meta_opt = aniverse.get_meta(&movie.id);
                        let thumb = meta_opt.clone().and_then(|m| m.thumb_url).unwrap_or_default();
                        let meta_data = meta_opt.map(|m| m.data).unwrap_or_default();
                        if let Some(s) = detect_season(&movie.title, &thumb, &meta_data) {
                            s
                        } else {
                            let answer: String = dialoguer::Input::with_theme(&crate::util::custom_theme())
                                .with_prompt("Sistem gagal mendeteksi Season. Ini Season berapa? (Kosongkan jika S1)")
                                .allow_empty(true)
                                .interact_text()
                                .unwrap_or_default();
                            answer.trim().parse::<u32>().unwrap_or(1)
                        }
                    } else {
                        1
                    };
                    cached_season = Some(detected);
                    detected
                }
            };

            // Stream selection & playback
            clearscreen_and_show_banner()?;
            let meta_opt = aniverse.get_meta(&movie.id);
            let thumb_url = meta_opt.and_then(|m| m.thumb_url);
            
            update_discord_status(discord, &movie.title, &episode.title, episode.is_series, season_num, thumb_url);

            // Pemutaran dengan retry: kalau pemutar gagal, user bisa memilih
            // server lain / mencoba lagi, bukan langsung dibuang ke menu episode.
            let mut played_ok = false;
            let mut retry_url: Option<String> = None;
            let mut sub_url: Option<String> = None;
            let mut audio_index: Option<u32> = None;

            loop {
                let fresh = retry_url.is_none();
                let stream_url = match retry_url.take() {
                    Some(url) => url,
                    None => match aniverse.extract_stream_urls(episode.clone()).await? {
                        Some(stream) => stream.url,
                        None => break, // Esc / Kembali dari pemilih server
                    },
                };

                if fresh {
                    sub_url = None;
                    audio_index = None;
                    if let Some(choice) =
                        crate::tracks::probe_and_choose(&stream_url, provider_type).await
                    {
                        sub_url = choice.sub_url;
                        audio_index = choice.audio_index;
                    }
                }

                match execute_player(crate::PlayRequest {
                    stream_url: &stream_url,
                    provider_type,
                    start_seconds: 0,
                    movie_title: movie.title.trim(),
                    episode_title: episode.title.trim(),
                    sub_url: sub_url.as_deref(),
                    audio_index,
                })
                .await?
                {
                    PlayOutcome::Played(pos) => {
                        input::save_watch_entry(&crate::models::WatchEntry {
                            movie: movie.clone(),
                            episode: episode.clone(),
                            provider_type,
                            position_seconds: pos,
                            updated_at: 0,
                        });
                        played_ok = true;
                        break;
                    }
                    PlayOutcome::Failed => {
                        let actions = [
                            "Pilih server lain (resolusi / mirror)",
                            "Coba lagi dengan stream yang sama",
                            "Kembali ke daftar episode",
                        ];
                        let sel = dialoguer::Select::with_theme(&crate::util::custom_theme())
                            .with_prompt("Pemutaran gagal. Pilih aksi: (Esc untuk kembali)")
                            .default(0)
                            .items(&actions)
                            .interact_opt()?;
                        match sel {
                            Some(0) => continue,
                            Some(1) => {
                                retry_url = Some(stream_url);
                                continue;
                            }
                            _ => break, // "Kembali ke daftar episode" atau Esc
                        }
                    }
                    PlayOutcome::Cancelled => break,
                }
            }

            reset_discord_status(discord);

            if !played_ok {
                // Pemutaran dibatalkan/gagal → kembali ke pemilih episode
                break;
            }

            // Auto-next logic
            let mut next_ep_opt = None;
            if let Some(episodes) = aniverse.get_episode_list(&movie.id) {
                let current_num = extract_ep_num(&episode.title);
                if current_num > 0 {
                    next_ep_opt = episodes.iter().find(|e| extract_ep_num(&e.title) == current_num + 1).cloned();
                }
            }

            if let Some(next_ep) = next_ep_opt {
                let confirm = dialoguer::Confirm::with_theme(&crate::util::custom_theme())
                    .with_prompt(format!("Lanjut ke {}?", next_ep.title.trim()))
                    .default(true)
                    .interact_opt()?
                    .unwrap_or(false);
                
                if confirm {
                    episode = next_ep;
                    continue; // Loop play again with next episode
                }
            }
            
            // If no next episode, or user declined, break back to episode selection
            break;
        }
    }
    Ok(())
}

fn show_main_menu() -> anyhow::Result<usize> {
    crate::util::section_header("MENU UTAMA");
    let items = [
        "1. Perbarui AniVerse-CLI",
        "2. Nonton Anime / Film",
        "3. Cek Kesehatan Sistem",
        "4. Keluar",
    ];

    let choice = match dialoguer::Select::with_theme(&crate::util::custom_theme())
        .with_prompt("Pilih Menu: (Esc untuk keluar)")
        .default(MENU_WATCH)
        .items(&items)
        .interact_opt()?
    {
        Some(c) => c,
        None => MENU_EXIT, // Esc
    };

    Ok(choice)
}

fn handle_self_update() -> anyhow::Result<()> {
    clearscreen_and_show_banner()?;
    println!("{} Memeriksa dan mengunduh pembaruan...", "◆".blue());
    println!("  Repository: {}\n", REPO_URL.cyan());

    let bin_name = if cfg!(target_os = "windows") { "aniverse-cli.exe" } else { "aniverse-cli" };
    let is_git_repo = std::path::Path::new(".git").exists();

    // On Windows, rename running executable before cargo install to avoid file locking
    let mut renamed_old: Option<(std::path::PathBuf, std::path::PathBuf)> = None;
    if cfg!(target_os = "windows") {
        if let Ok(current_exe) = std::env::current_exe() {
            let old_exe = current_exe.with_extension("exe.old");
            let _ = std::fs::remove_file(&old_exe);
            if std::fs::rename(&current_exe, &old_exe).is_ok() {
                renamed_old = Some((current_exe, old_exe));
            }
        }
    }

    let status = if is_git_repo {
        if !check_command_exists("git") {
            println!("\n  {} git tidak ditemukan di PATH.", "✖".red());
            println!("  Silahkan install git terlebih dahulu, lalu jalankan ulang update ini.");
            println!("\nTekan Enter untuk kembali ke menu...");
            let mut dummy = String::new();
            let _ = std::io::stdin().read_line(&mut dummy);
            return Ok(());
        }
        if !check_command_exists("cargo") {
            println!("\n  {} cargo / Rust toolchain tidak ditemukan di PATH.", "✖".red());
            println!("  Install Rust dari https://rustup.rs/ lalu jalankan ulang update ini.");
            println!("\nTekan Enter untuk kembali ke menu...");
            let mut dummy = String::new();
            let _ = std::io::stdin().read_line(&mut dummy);
            return Ok(());
        }
        println!("{} Terdeteksi repositori lokal. Menjalankan git pull & cargo install...", "ℹ".yellow());
        let _ = Command::new("git").args(["pull", "origin", "main"]).status();
        Command::new("cargo")
            .args(["install", "--path", ".", "--force"])
            .status()
    } else {
        if !check_command_exists("cargo") {
            println!("\n  {} cargo / Rust toolchain tidak ditemukan di PATH.", "✖".red());
            println!("  Install Rust dari https://rustup.rs/ lalu jalankan ulang update ini.");
            println!("\nTekan Enter untuk kembali ke menu...");
            let mut dummy = String::new();
            let _ = std::io::stdin().read_line(&mut dummy);
            return Ok(());
        }
        println!("{} Mengunduh dan mengompilasi dari GitHub...", "◆".blue());
        Command::new("cargo")
            .args(["install", "--git", REPO_URL, "--force"])
            .status()
    };

    match status {
        Ok(s) if s.success() => {
            println!("\n  {} Pembaruan berhasil diinstal!", "✓".green());
            if let Some(home) = dirs::home_dir() {
                let local_bin = home.join(".local/bin").join(bin_name);
                let candidate_bins = [
                    home.join(".cargo/bin").join(bin_name),
                    home.join(".local/share/rust-cargo/bin").join(bin_name),
                    std::path::PathBuf::from(format!("target/release/{}", bin_name)),
                    std::path::PathBuf::from(format!("target/debug/{}", bin_name)),
                ];
                for candidate in candidate_bins {
                    if candidate.exists() && local_bin.exists() {
                        let _ = std::fs::copy(&candidate, &local_bin);
                        break;
                    }
                }
            }
            if let Some((_, old_exe)) = renamed_old {
                let _ = std::fs::remove_file(old_exe);
            }
            println!("  Silahkan jalankan ulang aplikasi.");
        }
        _ => {
            if let Some((current_exe, old_exe)) = renamed_old {
                let _ = std::fs::rename(old_exe, current_exe);
            }
            println!("\n  {} Gagal memperbarui secara otomatis.", "✖".red());
            println!("  Silahkan jalankan secara manual:");
            println!("  cargo install --git {} --force", REPO_URL);
        }
    }

    println!("\nTekan Enter untuk kembali ke menu...");
    let mut dummy = String::new();
    let _ = std::io::stdin().read_line(&mut dummy);
    Ok(())
}

async fn handle_search_and_watch(discord: &mut Option<DiscordIpcClient>) -> anyhow::Result<()> {
    loop {
        clearscreen_and_show_banner()?;
        let input = match get_user_input().await? {
            Some(user_input) => user_input,
            None => break, // Back to watch submenu
        };

        let extractor = get_ext(&input);
        let mut aniverse = AniVerseCli::new(extractor);

        loop {
            clearscreen_and_show_banner()?;
            let movie = match aniverse.search(&input.title).await? {
                Some(selected_movie) => selected_movie,
                None => break, // Back to input prompt
            };

            handle_movie_episodes(&mut aniverse, &movie, input.tipe, discord).await?;
        }
    }
    Ok(())
}

/// Item menu riwayat.
enum HistItem {
    Group(usize),
    Separator,
    Search(usize),
    ClearWatch,
    ClearAll,
    Back,
}

async fn handle_resume_last_watch(discord: &mut Option<DiscordIpcClient>) -> anyhow::Result<()> {
    loop {
        clearscreen_and_show_banner()?;
        crate::util::section_header("RIWAYAT & PENCARIAN");

        let watch_history = load_watch_history();
        let search_history = load_history();
        let groups = group_watch_history(&watch_history);

        if groups.is_empty() && search_history.is_empty() {
            println!("Belum ada riwayat tontonan/pencarian.\n");
            println!("Tekan Enter untuk kembali...");
            let mut dummy = String::new();
            let _ = std::io::stdin().read_line(&mut dummy);
            return Ok(());
        }

        let mut items: Vec<String> = Vec::new();
        let mut kinds: Vec<HistItem> = Vec::new();

        for (i, group) in groups.iter().enumerate() {
            let last = &group.entries[0];
            items.push(format!(
                "▶ {} ({} ep ditonton) • {} • {} • {}",
                group.movie.title.trim(),
                group.entries.len(),
                last.episode.title.trim(),
                last.format_duration(),
                provider_name(last.provider_type)
            ));
            kinds.push(HistItem::Group(i));
        }

        if !search_history.is_empty() {
            items.push("──✦ Riwayat Pencarian ✦───────────────────────────".to_string());
            kinds.push(HistItem::Separator);
            for (i, title) in search_history.iter().enumerate() {
                items.push(format!("  {}", title));
                kinds.push(HistItem::Search(i));
            }
        }

        items.push("  Bersihkan Riwayat Tontonan".to_string());
        kinds.push(HistItem::ClearWatch);
        items.push("  Bersihkan Semua Riwayat".to_string());
        kinds.push(HistItem::ClearAll);
        items.push("  Kembali".to_string());
        kinds.push(HistItem::Back);

        let choice = match dialoguer::FuzzySelect::with_theme(&crate::util::custom_theme())
            .with_prompt("Watch & Search History (ketik untuk filter): (Esc untuk kembali)")
            .default(0)
            .items(&items)
            .interact_opt()?
        {
            Some(c) => c,
            None => break, // Esc
        };

        match &kinds[choice] {
            HistItem::Group(gi) => {
                handle_watch_group(&groups[*gi], discord).await?;
            }
            HistItem::Separator => continue,
            HistItem::Search(si) => {
                handle_search_history_item(&search_history[*si], discord).await?;
            }
            HistItem::ClearWatch => {
                if dialoguer::Confirm::with_theme(&crate::util::custom_theme())
                    .with_prompt("Hapus seluruh riwayat tontonan?")
                    .default(false)
                    .interact_opt()?
                    .unwrap_or(false)
                {
                    clear_watch_history();
                    println!("  {} Riwayat tontonan dibersihkan.", "✓".green());
                    wait_enter();
                }
            }
            HistItem::ClearAll => {
                if dialoguer::Confirm::with_theme(&crate::util::custom_theme())
                    .with_prompt("Hapus SEMUA riwayat (tontonan + pencarian)?")
                    .default(false)
                    .interact_opt()?
                    .unwrap_or(false)
                {
                    clear_watch_history();
                    clear_search_history();
                    println!("  {} Semua riwayat dibersihkan.", "✓".green());
                    wait_enter();
                }
            }
            HistItem::Back => break,
        }
    }
    Ok(())
}

/// Sub-menu per judul: pilih episode untuk dilanjutkan, kelola, atau hapus judul.
async fn handle_watch_group(
    group: &input::WatchGroup,
    discord: &mut Option<DiscordIpcClient>,
) -> anyhow::Result<()> {
    loop {
        clearscreen_and_show_banner()?;
        crate::util::section_header("PILIH EPISODE");
        println!(
            "  {} — {} episode ditonton ({})\n",
            group.movie.title.trim(),
            group.entries.len(),
            provider_name(group.entries[0].provider_type)
        );

        let mut items: Vec<String> = Vec::new();
        for entry in &group.entries {
            items.push(format!(
                "▶ {} • {} • {}",
                entry.episode.title.trim(),
                entry.format_duration(),
                provider_name(entry.provider_type)
            ));
        }
        items.push("  Hapus judul ini dari riwayat".to_string());
        items.push("  Kembali".to_string());

        let choice = match dialoguer::FuzzySelect::with_theme(&crate::util::custom_theme())
            .with_prompt("Pilih episode: (Esc untuk kembali, ketik untuk filter)")
            .default(0)
            .items(&items)
            .interact_opt()?
        {
            Some(c) => c,
            None => break,
        };

        if choice == group.entries.len() {
            if dialoguer::Confirm::with_theme(&crate::util::custom_theme())
                .with_prompt(format!(
                    "Hapus '{}' dari riwayat tontonan?",
                    group.movie.title.trim()
                ))
                .default(true)
                .interact_opt()?
                .unwrap_or(false)
            {
                delete_watch_entries_for_movie(&group.movie.id);
                println!("  {} Dihapus.", "✓".green());
                wait_enter();
            }
            continue;
        }
        if choice == group.entries.len() + 1 {
            break;
        }

        let selected = group.entries[choice].clone();
        if handle_watch_entry_actions(&selected, discord).await? {
            delete_watch_entry_by_ids(&selected.movie.id, &selected.episode.id);
        }
    }
    Ok(())
}

/// Menu aksi + pemutaran untuk satu entri riwayat.
/// Mengembalikan `true` kalau user meminta entri dihapus.
async fn handle_watch_entry_actions(
    selected_watch: &crate::models::WatchEntry,
    discord: &mut Option<DiscordIpcClient>,
) -> anyhow::Result<bool> {
    clearscreen_and_show_banner()?;
    crate::util::section_header("DETAIL TONTONAN");
    println!("  Judul    : {}", selected_watch.movie.title.trim());
    println!("  Episode  : {}", selected_watch.episode.title.trim());
    println!("  Posisi   : {}", selected_watch.format_duration());
    println!(
        "  Provider : {}\n",
        provider_name(selected_watch.provider_type)
    );

    let actions = [
        format!(
            "1. Lanjutkan Menonton (mulai {})",
            selected_watch.format_duration()
        ),
        "2. Putar Ulang dari Awal (00:00)".to_string(),
        "3. Hapus episode ini dari histori".to_string(),
        "4. Kembali".to_string(),
    ];

    let action_choice = match dialoguer::Select::with_theme(&crate::util::custom_theme())
        .with_prompt("Pilih Aksi: (Esc untuk kembali)")
        .default(0)
        .items(&actions)
        .interact_opt()?
    {
        Some(a) => a,
        None => return Ok(false), // Esc
    };

    if action_choice == 2 {
        return Ok(true);
    } else if action_choice == 3 {
        return Ok(false);
    }

    let start_seconds = if action_choice == 0 {
        selected_watch.position_seconds
    } else {
        0
    };

    clearscreen_and_show_banner()?;

    let season_num = if selected_watch.episode.is_series {
        detect_season(&selected_watch.movie.title, "", &[]).unwrap_or(1)
    } else {
        1
    };

    update_discord_status(
        discord,
        &selected_watch.movie.title,
        &selected_watch.episode.title,
        selected_watch.episode.is_series,
        season_num,
        None,
    );

    let mut aniverse = AniVerseCli::new(get_ext_by_provider(selected_watch.provider_type));
    let mut retry_url: Option<String> = None;
    let mut sub_url: Option<String> = None;
    let mut audio_index: Option<u32> = None;

    loop {
        let fresh = retry_url.is_none();
        let stream_url = match retry_url.take() {
            Some(url) => url,
            None => match aniverse.extract_stream_urls(selected_watch.episode.clone()).await? {
                Some(stream) => stream.url,
                None => break, // Esc / Kembali dari pemilih server
            },
        };

        if fresh {
            sub_url = None;
            audio_index = None;
            if let Some(choice) =
                crate::tracks::probe_and_choose(&stream_url, selected_watch.provider_type).await
            {
                sub_url = choice.sub_url;
                audio_index = choice.audio_index;
            }
        }

        match execute_player(crate::PlayRequest {
            stream_url: &stream_url,
            provider_type: selected_watch.provider_type,
            start_seconds,
            movie_title: selected_watch.movie.title.trim(),
            episode_title: selected_watch.episode.title.trim(),
            sub_url: sub_url.as_deref(),
            audio_index,
        })
        .await?
        {
            PlayOutcome::Played(pos) => {
                input::save_watch_entry(&crate::models::WatchEntry {
                    movie: selected_watch.movie.clone(),
                    episode: selected_watch.episode.clone(),
                    provider_type: selected_watch.provider_type,
                    position_seconds: pos,
                    updated_at: 0,
                });
                break;
            }
            PlayOutcome::Failed => {
                let actions = [
                    "Pilih server lain (resolusi / mirror)",
                    "Coba lagi dengan stream yang sama",
                    "Kembali ke menu riwayat",
                ];
                let sel = dialoguer::Select::with_theme(&crate::util::custom_theme())
                    .with_prompt("Pemutaran gagal. Pilih aksi: (Esc untuk kembali)")
                    .default(0)
                    .items(&actions)
                    .interact_opt()?;
                match sel {
                    Some(0) => continue,
                    Some(1) => {
                        retry_url = Some(stream_url);
                        continue;
                    }
                    _ => break, // "Kembali ke menu riwayat" atau Esc
                }
            }
            PlayOutcome::Cancelled => break,
        }
    }
    reset_discord_status(discord);
    Ok(false)
}

/// Menu aksi untuk satu judul di riwayat pencarian.
async fn handle_search_history_item(
    selected_title: &str,
    discord: &mut Option<DiscordIpcClient>,
) -> anyhow::Result<()> {
    let actions = [
        "1. Cari Judul Ini".to_string(),
        "2. Hapus dari Histori".to_string(),
        "3. Kembali".to_string(),
    ];

    clearscreen_and_show_banner()?;
    crate::util::section_header("RIWAYAT PENCARIAN");
    println!("  Judul : {}\n", selected_title.bright_cyan());

    let action_choice = match dialoguer::Select::with_theme(&crate::util::custom_theme())
        .with_prompt("Pilih Aksi: (Esc untuk kembali)")
        .default(0)
        .items(&actions)
        .interact_opt()?
    {
        Some(a) => a,
        None => return Ok(()), // Esc
    };

    if action_choice == 1 {
        delete_search_history_entry(selected_title);
        return Ok(());
    } else if action_choice == 2 {
        return Ok(());
    }

    let selected_title = selected_title.to_string();
    let tipe = pick_provider(&selected_title, false).await?;
    save_history(&selected_title);

    let input_data = crate::models::Input {
        title: selected_title,
        tipe,
    };
    let extractor = get_ext(&input_data);
    let mut aniverse = AniVerseCli::new(extractor);

    loop {
        clearscreen_and_show_banner()?;
        let movie = match aniverse.search(&input_data.title).await? {
            Some(m) => m,
            None => break,
        };
        handle_movie_episodes(&mut aniverse, &movie, input_data.tipe, discord).await?;
    }
    Ok(())
}

async fn handle_watch_mode(discord: &mut Option<DiscordIpcClient>) -> anyhow::Result<()> {
    loop {
        clearscreen_and_show_banner()?;
        let menu_items = [
            "1. Watch Any Anime / Movie",
            "2. Watch Last Anime / Movie [Watch & Search History]",
            "3. Kembali ke Menu Utama",
        ];

        let choice = match dialoguer::Select::with_theme(&crate::util::custom_theme())
            .with_prompt("Pilih Mode Menonton: (Esc untuk kembali)")
            .default(0)
            .items(&menu_items)
            .interact_opt()?
        {
            Some(c) => c,
            None => break, // Esc
        };

        match choice {
            0 => {
                handle_search_and_watch(discord).await?;
            }
            1 => {
                handle_resume_last_watch(discord).await?;
            }
            _ => break,
        }
    }
    Ok(())
}

async fn run_app() -> anyhow::Result<()> {
    let mut discord = Some(DiscordIpcClient::new(DISCORD_APP_ID));
    if let Some(ref mut drpc) = discord {
        if drpc.connect().is_ok() {
            reset_discord_status(&mut discord);
        } else {
            discord = None;
        }
    }

    if !check_command_exists("mpv") {
        show_health_screen()?;
    }

    loop {
        clearscreen_and_show_banner()?;
        let menu_choice = show_main_menu()?;

        match menu_choice {
            MENU_UPDATE => {
                handle_self_update()?;
            }
            MENU_WATCH => {
                handle_watch_mode(&mut discord).await?;
            }
            MENU_HEALTH => {
                let _ = show_health_screen()?;
            }
            MENU_EXIT => {
                println!("\nSampai jumpa lagi!\n");
                return Ok(());
            }
            _ => break,
        }
    }

    Ok(())
}

fn main() -> anyhow::Result<()> {
    let rt = runtime::Builder::new_multi_thread().enable_all().build()?;
    rt.block_on(async {
        if let Err(e) = run_app().await {
            println!(" {} {}\n", "✖".red(), format!("{:#?}", e).yellow());
        }
    });
    Ok(())
}

#[cfg(test)]
mod main_tests {
    use super::*;

    #[test]
    fn test_check_command_exists() {
        assert!(check_command_exists("cargo"));
        assert!(!check_command_exists("nonexistent_random_command_12345"));
    }

    #[test]
    fn test_temp_file_paths() {
        let p = idlix_audio_path();
        assert!(p.contains("aniverse_idlix_audio.m3u8"));
        assert!(!p.contains('\\'));
    }
}

