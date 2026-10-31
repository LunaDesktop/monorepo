//! Odtwarzanie filmów przez `ffmpeg` i `pw-play`/`paplay`.
//!
//! Nie linkujemy libav: `ffmpeg` dekoduje klatki do strumienia `rawvideo`
//! (RGBA) o stałym tempie 30 fps i z `-re`, czyli w tempie materiału.
//! Wątek dekodera trzyma tylko **najnowszą** klatkę — gdy rysowanie jest
//! wolniejsze, starsze klatki są pomijane, więc obraz nie zostaje w tyle za
//! dźwiękiem. Pauza i przewijanie zatrzymują potok i uruchamiają go od nowa —
//! to proste i przewidywalne, a przy okazji uwalnia dekoder, gdy nic nie gramy.
//!
//! Ścieżka dźwiękowa idzie osobnym potokiem `ffmpeg | pw-play` (albo `paplay`).
//! Oba procesy startują razem z obrazem; po przewinięciu oba startują od nowej
//! pozycji, więc nie rozjeżdżają się w czasie.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Instant;

use libluna_render::ImageData;

use crate::decode::{have_ffmpeg, in_path};

/// Stałe tempo klatek potoku (VFR i tak nie ma sensu w tej pętli).
pub const PLAY_FPS: f64 = 30.0;

/// Maksymalny rozmiar dekodowanej klatki.
const MAX_W: u32 = 1024;
const MAX_H: u32 = 576;

/// Metadane strumienia wideo.
#[derive(Debug, Clone, PartialEq)]
pub struct VideoSpec {
    pub width: u32,
    pub height: u32,
    pub fps: f64,
    pub duration: f64,
    pub codec: String,
    pub audio: bool,
}

impl VideoSpec {
    /// Krótki opis do belki odtwarzacza i panelu szczegółów.
    pub fn summary(&self) -> String {
        format!(
            "{}×{} · {}",
            self.width,
            self.height,
            crate::model::codec_label(&self.codec)
        )
    }
}

/// Próbkuje plik przez `ffprobe`. `None`, gdy brak narzędzi albo to nie wideo.
pub fn probe(path: &Path) -> Option<VideoSpec> {
    if !have_ffmpeg() || !in_path("ffprobe") {
        return None;
    }
    let output = Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-select_streams",
            "v:0",
            "-show_entries",
            "stream=width,height,avg_frame_rate,codec_name:format=duration",
            "-of",
            "default=noprint_wrappers=1",
        ])
        .arg(path)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let mut spec = parse_probe(&text)?;
    spec.audio = has_audio(path);
    Some(spec)
}

/// Parser wyjścia `ffprobe -of default=noprint_wrappers=1` (czysty, testowalny).
pub fn parse_probe(text: &str) -> Option<VideoSpec> {
    let mut width = 0u32;
    let mut height = 0u32;
    let mut fps = PLAY_FPS;
    let mut duration = 0.0f64;
    let mut codec = String::new();
    for line in text.lines() {
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let value = value.trim();
        match key.trim() {
            "width" => width = value.parse().unwrap_or(0),
            "height" => height = value.parse().unwrap_or(0),
            "avg_frame_rate" => fps = parse_ratio(value).unwrap_or(PLAY_FPS),
            "duration" => duration = value.parse().unwrap_or(0.0),
            "codec_name" => codec = value.to_owned(),
            _ => {}
        }
    }
    if width == 0 || height == 0 {
        return None;
    }
    Some(VideoSpec {
        width,
        height,
        fps: if fps.is_finite() && fps > 0.0 {
            fps.min(240.0)
        } else {
            PLAY_FPS
        },
        duration: if duration.is_finite() && duration > 0.0 {
            duration
        } else {
            0.0
        },
        codec,
        audio: false,
    })
}

fn parse_ratio(value: &str) -> Option<f64> {
    if let Some((num, den)) = value.split_once('/') {
        let num: f64 = num.parse().ok()?;
        let den: f64 = den.parse().ok()?;
        if den > 0.0 {
            return Some(num / den);
        }
        return None;
    }
    value.parse().ok()
}

fn has_audio(path: &Path) -> bool {
    let Ok(output) = Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-select_streams",
            "a:0",
            "-show_entries",
            "stream=codec_name",
            "-of",
            "default=noprint_wrappers=1:nokey=1",
        ])
        .arg(path)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
    else {
        return false;
    };
    output.status.success() && !String::from_utf8_lossy(&output.stdout).trim().is_empty()
}

/// Rozmiar klatki potoku: proporcje źródła, nie większy niż 1280×720.
pub fn target_size(width: u32, height: u32) -> (u32, u32) {
    let (w, h) = (width.max(2) as f64, height.max(2) as f64);
    let scale = (MAX_W as f64 / w).min(MAX_H as f64 / h).min(1.0);
    let tw = ((w * scale) as u32).max(2) & !1;
    let th = ((h * scale) as u32).max(2) & !1;
    (tw, th)
}

/// Jeden przebieg dekodowania: procesy ffmpeg (obraz + dźwięk) i odtwarzacz audio.
struct Pipeline {
    video: Child,
    audio_source: Option<Child>,
    audio_sink: Option<Child>,
}

impl Drop for Pipeline {
    fn drop(&mut self) {
        let _ = self.video.kill();
        let _ = self.video.wait();
        if let Some(mut source) = self.audio_source.take() {
            let _ = source.kill();
            let _ = source.wait();
        }
        if let Some(mut sink) = self.audio_sink.take() {
            let _ = sink.kill();
            let _ = sink.wait();
        }
    }
}

/// Odtwarzacz jednego pliku.
pub struct VideoPlayer {
    path: PathBuf,
    spec: VideoSpec,
    size: (u32, u32),
    position: f64,
    base: f64,
    started: Option<Instant>,
    playing: bool,
    muted: bool,
    ended: bool,
    got_frame: bool,
    frame: Option<Arc<ImageData>>,
    /// Najnowsza klatka z dekodera. Nadpisujemy ją zamiast kolejkować —
    /// gdy rysowanie jest wolniejsze od 30 fps, opuszczamy klatki, ale obraz
    /// nie zostaje w tyle za dźwiękiem.
    frame_sink: Option<Arc<Mutex<Option<Vec<u8>>>>>,
    frame_ended: Option<Arc<AtomicBool>>,
    pipeline: Option<Pipeline>,
    preview_rx: Option<Receiver<(u64, Option<ImageData>)>>,
    preview_id: u64,
    error: Option<String>,
}

impl VideoPlayer {
    /// Otwiera film i przygotowuje (jeszcze nie uruchamia) odtwarzanie.
    pub fn open(path: &Path, muted: bool) -> Result<Self, String> {
        if !have_ffmpeg() {
            return Err("ffmpeg is required to play videos".to_owned());
        }
        if !in_path("ffprobe") {
            return Err("ffprobe is required to play videos".to_owned());
        }
        let spec = probe(path).ok_or_else(|| "Cannot read this video".to_owned())?;
        let size = target_size(spec.width, spec.height);
        let mut player = Self {
            path: path.to_path_buf(),
            spec,
            size,
            position: 0.0,
            base: 0.0,
            started: None,
            playing: false,
            muted,
            ended: false,
            got_frame: false,
            frame: None,
            frame_sink: None,
            frame_ended: None,
            pipeline: None,
            preview_rx: None,
            preview_id: 0,
            error: None,
        };
        player.request_preview(0.0);
        Ok(player)
    }

    /// Metadane strumienia.
    pub fn spec(&self) -> &VideoSpec {
        &self.spec
    }

    pub fn size(&self) -> (u32, u32) {
        self.size
    }

    pub fn frame(&self) -> Option<&Arc<ImageData>> {
        self.frame.as_ref()
    }

    pub fn position(&self) -> f64 {
        if self.playing {
            self.live_position()
        } else {
            self.position
        }
    }

    pub fn duration(&self) -> f64 {
        self.spec.duration
    }

    pub fn playing(&self) -> bool {
        self.playing
    }

    pub fn muted(&self) -> bool {
        self.muted
    }

    pub fn ended(&self) -> bool {
        self.ended
    }

    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    fn live_position(&self) -> f64 {
        match self.started {
            Some(started) => (self.base + started.elapsed().as_secs_f64()).min(self.spec.duration),
            None => self.position,
        }
    }

    /// Uruchamia (albo wznawia) odtwarzanie.
    pub fn play(&mut self) {
        if self.playing {
            return;
        }
        if self.ended || self.position >= self.spec.duration - 0.05 {
            self.position = 0.0;
        }
        self.ended = false;
        self.base = self.position;
        self.preview_rx = None;
        if let Err(error) = self.start_pipeline() {
            self.error = Some(error);
            self.playing = false;
            self.started = None;
            return;
        }
        self.playing = true;
    }

    /// Wstrzymuje odtwarzanie (zapamiętuje pozycję).
    pub fn pause(&mut self) {
        if !self.playing {
            return;
        }
        self.position = self.live_position();
        self.playing = false;
        self.started = None;
        self.stop_pipeline();
    }

    pub fn toggle(&mut self) {
        if self.playing {
            self.pause();
        } else {
            self.play();
        }
    }

    /// Przewija do pozycji w sekundach.
    pub fn seek(&mut self, position: f64) {
        self.position = position.clamp(0.0, self.spec.duration);
        self.ended = false;
        if self.playing {
            self.base = self.position;
            self.started = None;
            self.stop_pipeline();
            if let Err(error) = self.start_pipeline() {
                self.error = Some(error);
                self.playing = false;
            }
        } else {
            self.request_preview(self.position);
        }
    }

    pub fn seek_by(&mut self, delta: f64) {
        self.seek(self.position() + delta);
    }

    pub fn set_muted(&mut self, muted: bool) {
        if self.muted == muted {
            return;
        }
        self.muted = muted;
        if self.playing {
            // Zmiana wyciszenia wymaga nowego potoku (audio jest osobnym procesem).
            let position = self.live_position();
            self.position = position;
            self.base = position;
            self.stop_pipeline();
            if let Err(error) = self.start_pipeline() {
                self.error = Some(error);
                self.playing = false;
            }
        }
    }

    /// Krok odtwarzania; zwraca `true`, gdy potrzebna jest kolejna klatka aplikacji.
    pub fn tick(&mut self) -> bool {
        let mut ended = false;
        if let Some(sink) = &self.frame_sink {
            let pixels = sink.lock().ok().and_then(|mut slot| slot.take());
            if let Some(pixels) = pixels {
                let (w, h) = self.size;
                self.got_frame = true;
                self.frame = Some(Arc::new(
                    ImageData::new(w, h, pixels)
                        .with_opaque(true)
                        .with_nearest(true),
                ));
            }
        }
        if self
            .frame_ended
            .as_ref()
            .is_some_and(|flag| flag.load(Ordering::Acquire))
        {
            // Ostatnia klatka mogła pojawić się tuż przed flagą zakończenia.
            if let Some(sink) = &self.frame_sink {
                if let Some(pixels) = sink.lock().ok().and_then(|mut slot| slot.take()) {
                    let (w, h) = self.size;
                    self.got_frame = true;
                    self.frame = Some(Arc::new(
                        ImageData::new(w, h, pixels)
                            .with_opaque(true)
                            .with_nearest(true),
                    ));
                }
            }
            self.position = self.spec.duration;
            self.playing = false;
            self.started = None;
            self.ended = true;
            // Seek pod sam koniec potrafi nie zwrócić żadnej klatki —
            // to normalne zakończenie, nie błąd dekodowania.
            let near_end = self.spec.duration > 0.0 && self.base >= self.spec.duration - 0.3;
            if !self.got_frame && !near_end && self.error.is_none() {
                self.error = Some("Cannot decode this video".to_owned());
            }
            ended = true;
        }
        if ended {
            self.stop_pipeline();
        }
        if let Some(rx) = self.preview_rx.take() {
            let mut keep = true;
            loop {
                match rx.try_recv() {
                    Ok((id, image)) => {
                        if id == self.preview_id {
                            if let Some(image) = image {
                                self.got_frame = true;
                                self.frame =
                                    Some(Arc::new(image.with_opaque(true).with_nearest(true)));
                            }
                            keep = false;
                            break;
                        }
                    }
                    Err(mpsc::TryRecvError::Empty) => break,
                    Err(mpsc::TryRecvError::Disconnected) => {
                        keep = false;
                        break;
                    }
                }
            }
            if keep {
                self.preview_rx = Some(rx);
            }
        }

        if self.playing {
            let position = self.live_position();
            if position >= self.spec.duration - 0.02 && self.spec.duration > 0.0 {
                self.position = self.spec.duration;
                self.playing = false;
                self.started = None;
                self.ended = true;
                self.stop_pipeline();
            }
        }
        self.playing || self.preview_rx.is_some()
    }

    fn start_pipeline(&mut self) -> Result<(), String> {
        let (w, h) = self.size;
        let filter = format!(
            "fps={fps},scale={w}:{h}:force_original_aspect_ratio=decrease,\
             pad={w}:{h}:(ow-iw)/2:(oh-ih)/2,format=rgba",
            fps = PLAY_FPS
        );
        let mut video = Command::new("ffmpeg")
            .args(["-v", "error", "-nostdin", "-ss"])
            .arg(format!("{:.3}", self.base.max(0.0)))
            .arg("-re")
            .arg("-i")
            .arg(&self.path)
            .args([
                "-map", "0:v:0", "-an", "-sn", "-vf", &filter, "-f", "rawvideo", "-pix_fmt",
                "rgba", "-",
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|_| "Could not start ffmpeg".to_owned())?;
        let Some(stdout) = video.stdout.take() else {
            let _ = video.kill();
            return Err("Could not read ffmpeg output".to_owned());
        };
        let sink = Arc::new(Mutex::new(None));
        let ended = Arc::new(AtomicBool::new(false));
        let thread_sink = Arc::clone(&sink);
        let thread_ended = Arc::clone(&ended);
        thread::Builder::new()
            .name("luna-media-video".into())
            .spawn(move || read_frames(stdout, w, h, thread_sink, thread_ended))
            .map_err(|_| "Could not start the video thread".to_owned())?;

        let (audio_source, audio_sink) = if !self.muted && self.spec.audio {
            spawn_audio(&self.path, self.base)
        } else {
            (None, None)
        };

        self.frame_sink = Some(sink);
        self.frame_ended = Some(ended);
        let started = Instant::now();
        self.started = Some(started);
        self.got_frame = false;
        self.pipeline = Some(Pipeline {
            video,
            audio_source,
            audio_sink,
        });
        Ok(())
    }

    fn stop_pipeline(&mut self) {
        self.frame_sink = None;
        self.frame_ended = None;
        self.pipeline = None;
    }

    fn request_preview(&mut self, position: f64) {
        self.preview_id = self.preview_id.wrapping_add(1);
        let id = self.preview_id;
        let (tx, rx) = mpsc::channel();
        let path = self.path.clone();
        let (w, h) = self.size;
        self.preview_rx = Some(rx);
        thread::Builder::new()
            .name("luna-media-frame".into())
            .spawn(move || {
                let image = grab_frame(&path, position, w, h);
                let _ = tx.send((id, image));
            })
            .ok();
    }
}

impl Drop for VideoPlayer {
    fn drop(&mut self) {
        self.stop_pipeline();
    }
}

fn read_frames(
    mut stdout: impl Read,
    width: u32,
    height: u32,
    sink: Arc<Mutex<Option<Vec<u8>>>>,
    ended: Arc<AtomicBool>,
) {
    let frame_len = width as usize * height as usize * 4;
    let mut buffer = vec![0u8; frame_len];
    loop {
        if stdout.read_exact(&mut buffer).is_err() {
            break;
        }
        // Nadpisujemy poprzednią klatkę — aplikacja zawsze dostaje najnowszą.
        if let Ok(mut slot) = sink.lock() {
            *slot = Some(buffer.clone());
        }
    }
    ended.store(true, Ordering::Release);
}

/// Wybiera dostępny odtwarzacz surowego PCM.
fn spawn_audio(path: &Path, from: f64) -> (Option<Child>, Option<Child>) {
    let (program, args): (&str, &[&str]) = if in_path("pw-play") {
        (
            "pw-play",
            &[
                "-a",
                "--rate",
                "48000",
                "--channels",
                "2",
                "--format",
                "s16",
                "-",
            ],
        )
    } else if in_path("paplay") {
        (
            "paplay",
            &["--raw", "--rate=48000", "--channels=2", "--format=s16le"],
        )
    } else {
        return (None, None);
    };

    let mut source = match Command::new("ffmpeg")
        .args(["-v", "error", "-nostdin", "-ss"])
        .arg(format!("{:.3}", from.max(0.0)))
        .arg("-i")
        .arg(path)
        .args([
            "-map", "0:a:0", "-vn", "-sn", "-ac", "2", "-ar", "48000", "-f", "s16le", "-",
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(child) => child,
        Err(_) => return (None, None),
    };
    let Some(stdout) = source.stdout.take() else {
        let _ = source.kill();
        return (None, None);
    };
    let sink = Command::new(program)
        .args(args)
        .stdin(Stdio::from(stdout))
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();
    match sink {
        Ok(sink) => (Some(source), Some(sink)),
        Err(_) => {
            let _ = source.kill();
            (None, None)
        }
    }
}

/// Jedna klatka do podglądu (pauza/seek/miniatura).
fn grab_frame(path: &Path, at: f64, width: u32, height: u32) -> Option<ImageData> {
    let filter = format!(
        "scale={w}:{h}:force_original_aspect_ratio=decrease,\
         pad={w}:{h}:(ow-iw)/2:(oh-ih)/2,format=rgba",
        w = width,
        h = height
    );
    let output = Command::new("ffmpeg")
        .args(["-v", "error", "-nostdin", "-ss"])
        .arg(format!("{:.3}", at.max(0.0)))
        .arg("-i")
        .arg(path)
        .args([
            "-map",
            "0:v:0",
            "-frames:v",
            "1",
            "-vf",
            &filter,
            "-f",
            "rawvideo",
            "-pix_fmt",
            "rgba",
            "-",
        ])
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    let expected = width as usize * height as usize * 4;
    if !output.status.success() || output.stdout.len() < expected {
        return None;
    }
    Some(ImageData::new(
        width,
        height,
        output.stdout[..expected].to_vec(),
    ))
}

/// Pobiera klatkę z pliku wideo do miniatury galerii.
pub fn thumbnail(path: &Path, max_side: u32) -> Option<ImageData> {
    crate::decode::video_frame(path, 0.5, max_side)
        .or_else(|| crate::decode::video_frame(path, 0.0, max_side))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn parses_ffprobe_output() {
        let text = "codec_name=h264\nwidth=1920\nheight=1080\navg_frame_rate=30000/1001\nduration=12.345000\n";
        let spec = parse_probe(text).unwrap();
        assert_eq!((spec.width, spec.height), (1920, 1080));
        assert!((spec.fps - 29.97).abs() < 0.01);
        assert!((spec.duration - 12.345).abs() < 1e-6);
        assert_eq!(spec.codec, "h264");
        assert!(!spec.audio);
    }

    #[test]
    fn rejects_output_without_dimensions() {
        assert!(parse_probe("width=0\nheight=0\n").is_none());
        assert!(parse_probe("codec_name=h264\n").is_none());
    }

    #[test]
    fn target_size_downscales_but_never_upscales() {
        assert_eq!(target_size(640, 360), (640, 360));
        assert_eq!(target_size(1920, 1080), (1024, 576));
        assert_eq!(target_size(3840, 2160), (1024, 576));
        assert_eq!(target_size(1080, 1920), (324, 576));
        assert_eq!(target_size(1, 1), (2, 2));
    }

    #[test]
    fn ratio_parser_handles_fractions_and_plain_numbers() {
        assert_eq!(parse_ratio("25/1"), Some(25.0));
        assert!((parse_ratio("30000/1001").unwrap() - 29.97002997).abs() < 1e-6);
        assert_eq!(parse_ratio("24"), Some(24.0));
        assert_eq!(parse_ratio("1/0"), None);
    }

    #[test]
    #[ignore = "wymaga ffmpeg/ffprobe i uruchamia prawdziwy potok"]
    fn plays_and_seeks_a_generated_clip() {
        if !have_ffmpeg() || !in_path("ffprobe") {
            eprintln!("brak ffmpeg/ffprobe — pomijam");
            return;
        }
        let dir = std::env::temp_dir().join(format!("luna-media-video-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("clip.mp4");
        let status = Command::new("ffmpeg")
            .args([
                "-v",
                "error",
                "-y",
                "-f",
                "lavfi",
                "-i",
                "testsrc=size=320x180:rate=25:duration=2",
                "-c:v",
                "libx264",
                "-pix_fmt",
                "yuv420p",
            ])
            .arg(&path)
            .status();
        assert!(status.map(|s| s.success()).unwrap_or(false));

        let mut player = VideoPlayer::open(&path, true).expect("probe powinien zadziałać");
        assert!(player.spec().duration > 1.0);
        let deadline = Instant::now() + Duration::from_secs(5);
        while player.frame().is_none() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(20));
            player.tick();
        }
        let frame = player.frame().expect("klatka podglądu");
        assert_eq!((frame.width, frame.height), player.size());

        player.play();
        std::thread::sleep(Duration::from_millis(500));
        assert!(player.tick());
        assert!(player.position() > 0.3);
        player.seek(1.2);
        assert!((player.position() - 1.2).abs() < 0.05);
        player.pause();
        assert!(!player.playing());

        let _ = std::fs::remove_dir_all(dir);
    }
}
