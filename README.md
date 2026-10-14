<div align="center">

# 🌌 AniVerse-CLI

**Streaming anime & film Asia dengan subtitle Indonesia — langsung dari terminal.**

*Fork dari [lucasbuilds/animeku-cli](https://github.com/lucasbuilds/animeku-cli)* | Vibecoded BTW (sorry for this bad things)

</div>

### Instalasi
> [!NOTE]
> Pastikan kamu sudah menginstall Rust dan pemutar media (**mpv** sangat disarankan), jika belum silahkan klik [tautan berikut](https://rustup.rs/).
>
> - **Windows**: Install MPV dengan mudah melalui terminal (PowerShell / CMD):
>   ```powershell
>   winget install --id shinchiro.mpv
>   ```
> - **Linux**: `sudo apt install mpv` (Ubuntu/Debian) atau `sudo pacman -S mpv` (Arch).
> - **Android (Termux)**: `pkg install rust mpv`.

#### Auto Install (Windows — satu baris)

Buka **PowerShell** atau **CMD** lalu jalankan perintah berikut:

```powershell
powershell -c "irm https://raw.githubusercontent.com/yuutaa7x/AniVerse-CLI/refs/heads/main/install.bat -OutFile install.bat; .\install.bat"
```

#### Auto Install (Linux / macOS — satu baris)

Buka terminal lalu jalankan:

```bash
curl -sL https://raw.githubusercontent.com/yuutaa7x/AniVerse-CLI/refs/heads/main/install.sh | bash
```

Script akan otomatis menginstall Rust dan MPV jika belum ada, lalu build dan install AniVerse-CLI.

#### Auto Install (Android / Termux — satu baris)

Buka aplikasi Termux lalu jalankan script dibawah ini untuk auto-install:

```bash
curl -sL https://raw.githubusercontent.com/yuutaa7x/AniVerse-CLI/refs/heads/main/install_termux.sh | bash
```

Tunggu instalasi selesai, dan jangan lupa download VLC for Android di Play Store terlebih dahulu sebelum menginstall/sebelum nonton

#### Manual Install (semua platform)

*Copy paste* perintah berikut:
```bash
cargo install --git https://github.com/yuutaa7x/AniVerse-CLI
```

Silahkan tunggu proses penginstallan selesai dan jika sudah, ketik `aniverse-cli` untuk menjalankannya.

#### Bawa ke device lain

AniVerse-CLI portable — cukup salin satu file binary hasil build (`target\release\aniverse-cli.exe` / `aniverse-cli`) ke device lain dan jalankan. Tidak perlu instalasi; riwayat & config dibuat baru per-user.

### Uninstall

#### Auto Uninstall (Windows — satu baris)

Buka **PowerShell** atau **CMD** lalu jalankan perintah berikut:

```powershell
powershell -c "irm https://raw.githubusercontent.com/yuutaa7x/AniVerse-CLI/refs/heads/main/uninstall.bat -OutFile uninstall.bat; .\uninstall.bat"
```

#### Auto Uninstall (Linux / macOS / Termux — satu baris)

```bash
curl -sL https://raw.githubusercontent.com/yuutaa7x/AniVerse-CLI/refs/heads/main/uninstall.sh | bash
```

#### Manual Uninstall (semua platform)

```bash
cargo uninstall aniverse-cli
```

### Keunggulan & Fitur
- Sangat mudah digunakan langsung dari command line.
- Menonton Anime, Live Action, Film, dan TV Series secara gratis tanpa iklan.
- **4 provider**: Idlix, Otakudesu, Samehadaku, dan Kusonime (batch/download).
- Hasil pencarian lengkap dengan pilihan server & resolusi (1080p, 720p, 480p, 360p).
- **Menu pilihan subtitle & audio track** — pilih bahasa subtitle/track audio sebelum mulai (HLS master).
- **Fallback yt-dlp otomatis** kalau pemutaran MPV gagal.
- **Download offline** via yt-dlp ke `~/Downloads/AniVerse/<judul>/` dengan progress live.
- Otomatis memuat Subtitle Indonesia langsung ke player.
- Terintegrasi dengan Discord Rich Presence (RPC).
- **Riwayat tontonan terkelompok per judul** (sub-menu episode, hapus per judul, hapus semua).
- Fitur Riwayat Pencarian (suggestion otomatis saat mengetik).
- Resume Otomatis (melanjutkan episode dari menit/detik terakhir yang ditonton).
- Auto-Next Episode setelah selesai memutar.
- Pengecekan dependensi dan Check Health sistem yang interaktif.

### Changelog

#### v0.3.0 — Rebrand & Provider Baru
- **Rebrand**: seluruh tampilan menjadi **AniVerse-CLI** — banner, warna tema (cyan/magenta), ikon error (`✖`), marker detail (`»`), menu berbahasa Indonesia, dan identitas Discord RPC.
- **Nama & path**: binary `aniverse-cli`, riwayat di `~/.aniverse_history` & `~/.aniverse_watch_history.json` (riwayat lama otomatis dimigrasi), folder download `~/Downloads/AniVerse/`.
- **Provider baru**:
  - **Samehadaku** (samehadaku.li) — pencarian via WP REST + resolver khusus embed Blogger (RPC batchexecute) → URL googlevideo langsung, karena extractor yt-dlp rusak untuk player Blogger baru.
  - **Kusonime** (kusonime.com) — provider batch/movie, menampilkan link Google Drive/Mega dengan label jujur "(ZIP batch)".
- **Fitur pemutaran**:
  - Menu pilihan **subtitle & audio track** dari master playlist HLS.
  - **Fallback yt-dlp otomatis** bila MPV gagal (resolve ulang URL + retry).
  - **Download offline** per-episode via yt-dlp (progress live, hint instalasi bila yt-dlp belum ada).
- **Riwayat tontonan dikelompokkan per judul** + hapus massal (per judul / semua) + kapasitas 100 entri.
- **Manifest Scoop & winget** disiapkan di `packaging/`.
- Kualitas: clippy 0 warning, 24+ unit test (termasuk test live provider).

#### Sebelumnya (v0.2.0) — Perbaikan dari Upstream
Dibandingkan dengan repo upstream (`lucasbuilds/animeku-cli`):

##### 1. Menu & Riwayat Tontonan / Pencarian
- Menu utama & submenu terstruktur, termasuk `Check Health` interaktif.
- Auto-tracking & resume playback multi-entry (banyak judul sekaligus, presisi per-detik).
- Search history terintegrasi dengan suggestion otomatis; deteksi jenis konten otomatis (Movie/Series → Idlix; Anime → pilih provider).

##### 2. Provider Streaming
- **Idlix**: bypass gateway sesi, multi-resolusi HLS terurut tanpa duplikat, audio terpisah tersinkron, subtitle `.vtt` otomatis, optimasi CDN anti-stutter.
- **Otakudesu**: parser streaming anime Sub Indo yang stabil (fix HTTP 403 via dekode HTML entity pada URL embed).
- Pembersihan provider mati (`nontonanime`, `tenflix`).

##### 3. Discord Rich Presence (RPC)
- Format judul bersih, status dinamis (*Full Movie* / *Season X • Episode Y*), elapsed timestamp, poster thumbnail.

##### 4. Pemutar Media & Tampilan CLI
- Integrasi MPV matang (demuxer lavf, whitelist HLS, anti-spam log), resume via `--start`, opsi VLC & Browser.
- Refaktor & kualitas kode: 0 compiler warnings, unit test otomatis.

##### 5. Kompatibilitas Windows & Cross-Platform
- Auto-install satu baris (`.bat`/`.sh`), deteksi package manager Windows, cari binary MPV/VLC cerdas.
- Temp file native cross-platform, self-update aman di Windows (auto-rename binary berjalan).

### Credits
- [lucasbuilds](https://github.com/lucasbuilds/animeku-cli) - For Base Repo
- [Wingky530](https://github.com/Wingky530/otakudesu-scraper) - For Otakudesu Scraper
- [annurdian](https://github.com/annurdien/IDLIX-API) - For Idlixku API
