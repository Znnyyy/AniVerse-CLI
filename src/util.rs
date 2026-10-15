use colored::Colorize;
use dialoguer::{
    console::{style, Style},
    theme::ColorfulTheme,
};

/// Tema dialog cosmic AniVerse: prompt magenta, kursor `▶`, item aktif cyan tebal.
pub fn custom_theme() -> ColorfulTheme {
    ColorfulTheme {
        defaults_style: Style::new().for_stderr().magenta(),
        prompt_style: Style::new().for_stderr().bold(),
        prompt_prefix: style("❯".to_string()).for_stderr().magenta(),
        prompt_suffix: style("»".to_string()).for_stderr().black().bright(),
        success_prefix: style("✓".to_string()).for_stderr().cyan(),
        success_suffix: style("·".to_string()).for_stderr().black().bright(),
        error_prefix: style("✖".to_string()).for_stderr().red(),
        error_style: Style::new().for_stderr().red(),
        hint_style: Style::new().for_stderr().magenta(),
        values_style: Style::new().for_stderr().cyan(),
        active_item_style: Style::new().for_stderr().cyan().bold(),
        inactive_item_style: Style::new().for_stderr().black().bright(),
        active_item_prefix: style("▶".to_string()).for_stderr().cyan(),
        inactive_item_prefix: style(" ".to_string()).for_stderr(),
        checked_item_prefix: style("✓".to_string()).for_stderr().cyan(),
        unchecked_item_prefix: style("⬚".to_string()).for_stderr().magenta(),
        picked_item_prefix: style("▶".to_string()).for_stderr().cyan(),
        unpicked_item_prefix: style(" ".to_string()).for_stderr(),
        fuzzy_cursor_style: Style::new().for_stderr().white().on_magenta(),
        fuzzy_match_highlight_style: Style::new().for_stderr().bold().magenta(),
    }
}

pub fn temp_file(name: &str) -> String {
    std::env::temp_dir()
        .join(name)
        .to_string_lossy()
        .replace('\\', "/")
}

use reqwest::Client;

#[macro_export]
macro_rules! regex {
    ($re:literal $(,)?) => {{
        static RE: once_cell::sync::OnceCell<regex::Regex> = once_cell::sync::OnceCell::new();
        RE.get_or_init(|| regex::Regex::new($re).unwrap())
    }};
}

pub async fn show_image_thumb(url: String) {
    let client = Client::new();
    if let Ok(resp) = client.get(url).send().await {
        if let Ok(bytes) = resp.bytes().await {
            if let Ok(img) = image::load_from_memory(&bytes) {
                let conf = viuer::Config {
                    transparent: true,
                    width: Some(50),
                    height: Some(30),
                    y: 8,
                    x: 2,
                    ..Default::default()
                };
                if viuer::print(&img, &conf).is_ok() {
                    println!();
                }
            }
        }
    }
}

/// Baris banner besar (font ANSI Shadow), diwarnai per-baris dengan gradient cosmic.
const BANNER_ROWS: [&str; 6] = [
    " █████╗ ███╗   ██╗██╗██╗   ██╗███████╗██████╗ ███████╗███████╗",
    "██╔══██╗████╗  ██║██║██║   ██║██╔════╝██╔══██╗██╔════╝██╔════╝",
    "███████║██╔██╗ ██║██║██║   ██║█████╗  ██████╔╝███████╗█████╗  ",
    "██╔══██║██║╚██╗██║██║╚██╗ ██╔╝██╔══╝  ██╔══██╗╚════██║██╔══╝  ",
    "██║  ██║██║ ╚████║██║ ╚████╔╝ ███████╗██║  ██║███████║███████╗",
    "╚═╝  ╚═╝╚═╝  ╚═══╝╚═╝  ╚═══╝  ╚══════╝╚═╝  ╚═╝╚══════╝╚══════╝",
];

/// Gradient per baris: cyan terang (atas) → magenta pink (bawah).
const BANNER_GRADIENT: [(u8, u8, u8); 6] = [
    (96, 214, 255),
    (112, 172, 255),
    (136, 136, 252),
    (170, 108, 246),
    (208, 82, 232),
    (242, 68, 200),
];

pub fn clearscreen_and_show_banner() -> anyhow::Result<()> {
    clearscreen::clear()?;

    eprintln!(
        "\n{}",
        "        ✦  ˚ ·✧ ˚ ✦ ˚ ·✧ ˚ ✦ ˚ ·✧ ˚ ✦".truecolor(96, 214, 255)
    );
    for (row, &(r, g, b)) in BANNER_ROWS.iter().zip(BANNER_GRADIENT.iter()) {
        eprintln!("  {}", row.truecolor(r, g, b));
    }
    eprintln!(
        "  {}",
        "streaming anime & film · di ujung terminal".bright_black()
    );
    eprintln!(
        "     v{}  {}  {}  ·  {}",
        env!("CARGO_PKG_VERSION"),
        "AniVerse-CLI".bright_magenta().bold(),
        "©".cyan(),
        "by Znnyyy".bright_black()
    );
    eprintln!();
    Ok(())
}

/// Header section seragam di seluruh layar: `──· JUDUL ·──────────────`
pub fn section_header(title: &str) {
    println!(
        "\n  {} {} {}",
        "──·".bright_black(),
        title.to_string().bright_magenta().bold(),
        "─".repeat(50).bright_black()
    );
}
