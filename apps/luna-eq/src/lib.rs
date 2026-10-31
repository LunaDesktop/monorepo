//! # luna-eq — korektor graficzny Luny (wtyczka LADSPA)
//!
//! Dziesięciopasmowy korektor (pasma oktawowe 31 Hz … 16 kHz) plus przedwzmacniacz.
//! Wtyczka jest **samodzielna**: nie zależy od libLuny ani od żadnych bibliotek
//! zewnętrznych — ładuje ją PipeWire (`libpipewire-module-filter-chain`) przez
//! `libspa-filter-graph-plugin-ladspa.so`, a nie przez plugin `builtin` (który
//! w części dystrybucji nie ładuje się przez zepsute `libsndfile`/LAME).
//!
//! Wtyczka jest per-kanał: `filter-chain` tworzy po jednej instancji na kanał
//! (`audio.channels`), więc nie trzeba znać liczby kanałów w kodzie DSP.
//!
//! ## Strojenie
//! Każde pasmo to filtr `peaking` (RBJ biquad) o szerokości jednej oktawy
//! (Q ≈ 1.414), wzmocnienie −12…+12 dB. Wartości 0 dB są pomijane (tożsamość),
//! więc płaski korektor nic nie liczy ponad mnożenie przedwzmacniacza.

#![allow(non_snake_case)]

use std::ffi::{c_char, c_ulong, c_void};
use std::sync::OnceLock;

/// Częstotliwości środkowe pasm (Hz).
pub const BAND_HZ: [f32; 10] = [
    31.0, 62.0, 125.0, 250.0, 500.0, 1_000.0, 2_000.0, 4_000.0, 8_000.0, 16_000.0,
];

/// Etykiety pasm (spójne z UI ustawień).
pub const BAND_LABELS: [&str; 10] = [
    "31", "62", "125", "250", "500", "1k", "2k", "4k", "8k", "16k",
];

/// Maksymalne wzmocnienie/tłumienie pasma w dB.
pub const MAX_GAIN_DB: f32 = 12.0;

/// Dobroć pasma o szerokości jednej oktawy.
pub const BAND_Q: f32 = std::f32::consts::SQRT_2;

/// Filtr bikwadratowy w postaci transponowanej (Direct Form II transposed).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Biquad {
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
    z1: f32,
    z2: f32,
}

impl Biquad {
    /// Ustawia filtr półkowy `peaking` (RBJ) o zadanej dobroci i wzmocnieniu w dB.
    ///
    /// Wzmocnienie 0 dB daje dokładną tożsamość (`b0 = 1`, reszta zer).
    pub fn peaking(sample_rate: f32, freq: f32, q: f32, gain_db: f32) -> Self {
        let mut filter = Self::default();
        filter.set_peaking(sample_rate, freq, q, gain_db);
        filter
    }

    /// Przelicza współczynniki (stan filtrów zostaje — pozwala zmieniać pasma w locie).
    pub fn set_peaking(&mut self, sample_rate: f32, freq: f32, q: f32, gain_db: f32) {
        if !gain_db.is_finite() || gain_db.abs() < 0.005 {
            self.b0 = 1.0;
            self.b1 = 0.0;
            self.b2 = 0.0;
            self.a1 = 0.0;
            self.a2 = 0.0;
            return;
        }
        let fs = if sample_rate.is_finite() && sample_rate > 0.0 {
            sample_rate
        } else {
            48_000.0
        };
        // Pasmo powyżej Nyquista nie ma sensu — zostaw tożsamość.
        if freq >= fs * 0.48 || freq <= 0.0 {
            self.b0 = 1.0;
            self.b1 = 0.0;
            self.b2 = 0.0;
            self.a1 = 0.0;
            self.a2 = 0.0;
            return;
        }
        let a = 10f32.powf(gain_db.clamp(-24.0, 24.0) / 40.0);
        let w0 = 2.0 * std::f32::consts::PI * freq / fs;
        let (sin, cos) = w0.sin_cos();
        let alpha = sin / (2.0 * q.max(0.1));
        let a0 = 1.0 + alpha / a;

        self.b0 = (1.0 + alpha * a) / a0;
        self.b1 = (-2.0 * cos) / a0;
        self.b2 = (1.0 - alpha * a) / a0;
        self.a1 = (-2.0 * cos) / a0;
        self.a2 = (1.0 - alpha / a) / a0;
    }

    /// Zeruje pamięć filtra (bez zmiany współczynników).
    pub fn reset(&mut self) {
        self.z1 = 0.0;
        self.z2 = 0.0;
    }

    #[inline]
    pub fn process(&mut self, x: f32) -> f32 {
        let y = self.b0 * x + self.z1;
        self.z1 = self.b1 * x - self.a1 * y + self.z2;
        self.z2 = self.b2 * x - self.a2 * y;
        // Bardzo małe resztki (denormale) potrafią zwolnić DSP — zerujemy je.
        if self.z1.abs() < 1e-20 {
            self.z1 = 0.0;
        }
        if self.z2.abs() < 1e-20 {
            self.z2 = 0.0;
        }
        y
    }
}

/// Łańcuch 10 pasm + przedwzmacniacz — czysta część DSP (testowalna bez FFI).
#[derive(Clone, Debug)]
pub struct EqChain {
    sample_rate: f32,
    preamp_db: f32,
    gains: [f32; 10],
    applied: [f32; 10],
    applied_preamp: f32,
    preamp_linear: f32,
    filters: [Biquad; 10],
}

impl EqChain {
    pub fn new(sample_rate: f32) -> Self {
        let sample_rate = if sample_rate.is_finite() && sample_rate > 0.0 {
            sample_rate
        } else {
            48_000.0
        };
        let mut chain = Self {
            sample_rate,
            preamp_db: 0.0,
            gains: [0.0; 10],
            applied: [f32::NAN; 10],
            applied_preamp: f32::NAN,
            preamp_linear: 1.0,
            filters: [Biquad::default(); 10],
        };
        chain.update_coeffs();
        chain
    }

    /// Ustawia wzmocnienia pasm (dB, przycinane do ±[`MAX_GAIN_DB`]) i przedwzmacniacz.
    ///
    /// Współczynniki przelicza dopiero [`Self::update_coeffs`] — dzięki temu pętla
    /// audio nie musi tego robić, jeśli nic się nie zmieniło.
    pub fn set_gains(&mut self, preamp_db: f32, gains: [f32; 10]) {
        self.preamp_db = if preamp_db.is_finite() {
            preamp_db.clamp(-MAX_GAIN_DB, MAX_GAIN_DB)
        } else {
            0.0
        };
        for (dst, src) in self.gains.iter_mut().zip(gains) {
            *dst = if src.is_finite() {
                src.clamp(-MAX_GAIN_DB, MAX_GAIN_DB)
            } else {
                0.0
            };
        }
    }

    /// Przelicza współczynniki, jeśli któryś suwak się zmienił. Zwraca `true` przy zmianie.
    pub fn update_coeffs(&mut self) -> bool {
        // Porównania muszą być odporne na NaN: wartością początkową `applied` jest NaN,
        // a `NaN > x` jest fałszem — bez zaprzeczenia współczynniki nigdy by się nie
        // policzyły i na wyjściu pojawiłyby się NaN-y.
        let mut changed = !((self.applied_preamp - self.preamp_db).abs() <= 1e-4);
        for (i, &gain) in self.gains.iter().enumerate() {
            if !((self.applied[i] - gain).abs() <= 1e-3) {
                changed = true;
                self.filters[i].set_peaking(self.sample_rate, BAND_HZ[i], BAND_Q, gain);
                self.applied[i] = gain;
            }
        }
        if changed {
            self.applied_preamp = self.preamp_db;
            self.preamp_linear = 10f32.powf(self.applied_preamp / 20.0);
        }
        changed
    }

    /// Zeruje pamięć wszystkich filtrów (bez zmiany ustawień).
    pub fn reset(&mut self) {
        for filter in &mut self.filters {
            filter.reset();
        }
    }

    #[inline]
    pub fn process(&mut self, x: f32) -> f32 {
        let mut y = x * self.preamp_linear;
        for filter in &mut self.filters {
            y = filter.process(y);
        }
        y
    }

    /// Przetwarza bufor w miejscu (używane przez `run` wtyczki i w testach).
    pub fn process_buffer(&mut self, samples: &mut [f32]) {
        self.update_coeffs();
        for x in samples.iter_mut() {
            *x = self.process(*x);
        }
    }
}

// ───────────────────────────── LADSPA (C ABI) ─────────────────────────────

const LADSPA_PORT_INPUT: i32 = 0x1;
const LADSPA_PORT_OUTPUT: i32 = 0x2;
const LADSPA_PORT_CONTROL: i32 = 0x4;
const LADSPA_PORT_AUDIO: i32 = 0x8;

const LADSPA_HINT_BOUNDED_BELOW: i32 = 0x1;
const LADSPA_HINT_BOUNDED_ABOVE: i32 = 0x2;
const LADSPA_HINT_DEFAULT_0: i32 = 0x80;

const LADSPA_PROPERTY_HARD_RT_CAPABLE: i32 = 0x1;

/// Liczba portów: In, Out + przedwzmacniacz + 10 pasm.
const N_PORTS: usize = 13;
/// Liczba portów sterujących.
const N_CONTROLS: usize = 11;

const UNIQUE_ID: c_ulong = 0x4C55_4E41_4551; // "LUNAEQ"

type LadspaHandle = *mut c_void;

#[repr(C)]
pub struct PortRangeHint {
    hint_descriptor: i32,
    lower_bound: f32,
    upper_bound: f32,
}

#[repr(C)]
pub struct LadspaDescriptor {
    unique_id: c_ulong,
    label: *const c_char,
    properties: i32,
    name: *const c_char,
    maker: *const c_char,
    copyright: *const c_char,
    port_count: c_ulong,
    port_descriptors: *const i32,
    port_names: *const *const c_char,
    port_range_hints: *const PortRangeHint,
    implementation_data: *mut c_void,
    instantiate: Option<unsafe extern "C" fn(*const LadspaDescriptor, c_ulong) -> LadspaHandle>,
    connect_port: Option<unsafe extern "C" fn(LadspaHandle, c_ulong, *mut f32)>,
    activate: Option<unsafe extern "C" fn(LadspaHandle)>,
    run: Option<unsafe extern "C" fn(LadspaHandle, c_ulong)>,
    run_adding: Option<unsafe extern "C" fn(LadspaHandle, c_ulong)>,
    set_run_adding_gain: Option<unsafe extern "C" fn(LadspaHandle, f32)>,
    deactivate: Option<unsafe extern "C" fn(LadspaHandle)>,
    cleanup: Option<unsafe extern "C" fn(LadspaHandle)>,
}

/// Instancja wtyczki (jedna na kanał audio).
struct Instance {
    chain: EqChain,
    input: *mut f32,
    output: *mut f32,
    controls: [*mut f32; N_CONTROLS],
}

impl Instance {
    fn read_controls(&mut self) {
        let mut gains = [0.0f32; 10];
        for (i, gain) in gains.iter_mut().enumerate() {
            let ptr = self.controls[i + 1];
            if !ptr.is_null() {
                *gain = unsafe { *ptr };
            }
        }
        let preamp = {
            let ptr = self.controls[0];
            if ptr.is_null() {
                0.0
            } else {
                unsafe { *ptr }
            }
        };
        self.chain.set_gains(preamp, gains);
        self.chain.update_coeffs();
    }
}

static PORT_DESCRIPTORS: [i32; N_PORTS] = [
    LADSPA_PORT_INPUT | LADSPA_PORT_AUDIO,
    LADSPA_PORT_OUTPUT | LADSPA_PORT_AUDIO,
    LADSPA_PORT_INPUT | LADSPA_PORT_CONTROL,
    LADSPA_PORT_INPUT | LADSPA_PORT_CONTROL,
    LADSPA_PORT_INPUT | LADSPA_PORT_CONTROL,
    LADSPA_PORT_INPUT | LADSPA_PORT_CONTROL,
    LADSPA_PORT_INPUT | LADSPA_PORT_CONTROL,
    LADSPA_PORT_INPUT | LADSPA_PORT_CONTROL,
    LADSPA_PORT_INPUT | LADSPA_PORT_CONTROL,
    LADSPA_PORT_INPUT | LADSPA_PORT_CONTROL,
    LADSPA_PORT_INPUT | LADSPA_PORT_CONTROL,
    LADSPA_PORT_INPUT | LADSPA_PORT_CONTROL,
    LADSPA_PORT_INPUT | LADSPA_PORT_CONTROL,
];

static PORT_NAMES_TEXT: [&str; N_PORTS] = [
    "In", "Out", "Preamp", "31 Hz", "62 Hz", "125 Hz", "250 Hz", "500 Hz", "1 kHz", "2 kHz",
    "4 kHz", "8 kHz", "16 kHz",
];

const GAIN_HINT: i32 =
    LADSPA_HINT_BOUNDED_BELOW | LADSPA_HINT_BOUNDED_ABOVE | LADSPA_HINT_DEFAULT_0;

static PORT_HINTS: [PortRangeHint; N_PORTS] = [
    PortRangeHint {
        hint_descriptor: 0,
        lower_bound: 0.0,
        upper_bound: 0.0,
    },
    PortRangeHint {
        hint_descriptor: 0,
        lower_bound: 0.0,
        upper_bound: 0.0,
    },
    PortRangeHint {
        hint_descriptor: GAIN_HINT,
        lower_bound: -MAX_GAIN_DB,
        upper_bound: MAX_GAIN_DB,
    },
    PortRangeHint {
        hint_descriptor: GAIN_HINT,
        lower_bound: -MAX_GAIN_DB,
        upper_bound: MAX_GAIN_DB,
    },
    PortRangeHint {
        hint_descriptor: GAIN_HINT,
        lower_bound: -MAX_GAIN_DB,
        upper_bound: MAX_GAIN_DB,
    },
    PortRangeHint {
        hint_descriptor: GAIN_HINT,
        lower_bound: -MAX_GAIN_DB,
        upper_bound: MAX_GAIN_DB,
    },
    PortRangeHint {
        hint_descriptor: GAIN_HINT,
        lower_bound: -MAX_GAIN_DB,
        upper_bound: MAX_GAIN_DB,
    },
    PortRangeHint {
        hint_descriptor: GAIN_HINT,
        lower_bound: -MAX_GAIN_DB,
        upper_bound: MAX_GAIN_DB,
    },
    PortRangeHint {
        hint_descriptor: GAIN_HINT,
        lower_bound: -MAX_GAIN_DB,
        upper_bound: MAX_GAIN_DB,
    },
    PortRangeHint {
        hint_descriptor: GAIN_HINT,
        lower_bound: -MAX_GAIN_DB,
        upper_bound: MAX_GAIN_DB,
    },
    PortRangeHint {
        hint_descriptor: GAIN_HINT,
        lower_bound: -MAX_GAIN_DB,
        upper_bound: MAX_GAIN_DB,
    },
    PortRangeHint {
        hint_descriptor: GAIN_HINT,
        lower_bound: -MAX_GAIN_DB,
        upper_bound: MAX_GAIN_DB,
    },
    PortRangeHint {
        hint_descriptor: GAIN_HINT,
        lower_bound: -MAX_GAIN_DB,
        upper_bound: MAX_GAIN_DB,
    },
];

unsafe extern "C" fn instantiate(_desc: *const LadspaDescriptor, rate: c_ulong) -> LadspaHandle {
    let instance = Box::new(Instance {
        chain: EqChain::new(rate as f32),
        input: std::ptr::null_mut(),
        output: std::ptr::null_mut(),
        controls: [std::ptr::null_mut(); N_CONTROLS],
    });
    Box::into_raw(instance).cast()
}

unsafe extern "C" fn connect_port(handle: LadspaHandle, port: c_ulong, data: *mut f32) {
    let instance = &mut *(handle as *mut Instance);
    match port as usize {
        0 => instance.input = data,
        1 => instance.output = data,
        2..=12 => instance.controls[port as usize - 2] = data,
        _ => {}
    }
}

unsafe extern "C" fn activate(handle: LadspaHandle) {
    let instance = &mut *(handle as *mut Instance);
    instance.read_controls();
    instance.chain.reset();
}

unsafe extern "C" fn run(handle: LadspaHandle, count: c_ulong) {
    let instance = &mut *(handle as *mut Instance);
    if instance.input.is_null() || instance.output.is_null() {
        return;
    }
    instance.read_controls();
    let n = count as usize;
    let input = std::slice::from_raw_parts_mut(instance.input, n);
    let output = std::slice::from_raw_parts_mut(instance.output, n);
    for i in 0..n {
        output[i] = instance.chain.process(input[i]);
    }
}

unsafe extern "C" fn deactivate(handle: LadspaHandle) {
    let instance = &mut *(handle as *mut Instance);
    instance.chain.reset();
}

unsafe extern "C" fn cleanup(handle: LadspaHandle) {
    if !handle.is_null() {
        drop(Box::from_raw(handle as *mut Instance));
    }
}

struct DescriptorSlot(LadspaDescriptor);
// Wskaźniki w deskryptorze wskazują na dane statyczne/`leaked` — są wieczne i tylko
// do odczytu, więc współdzielenie między wątkami jest bezpieczne.
unsafe impl Sync for DescriptorSlot {}
unsafe impl Send for DescriptorSlot {}

static DESCRIPTOR: OnceLock<DescriptorSlot> = OnceLock::new();

/// Eksport LADSPA: PipeWire woła `ladspa_descriptor(0)` przy ładowaniu wtyczki.
///
/// # Safety
/// Funkcja jest wywoływana przez hosta LADSPA (C); zwraca wskaźnik do statycznego
/// deskryptora, który żyje do końca procesu.
#[no_mangle]
pub extern "C" fn ladspa_descriptor(index: c_ulong) -> *const LadspaDescriptor {
    if index != 0 {
        return std::ptr::null();
    }
    &DESCRIPTOR
        .get_or_init(|| {
            let names: Vec<*const c_char> = PORT_NAMES_TEXT
                .iter()
                .map(|name| {
                    let mut bytes = Vec::with_capacity(name.len() + 1);
                    bytes.extend_from_slice(name.as_bytes());
                    bytes.push(0);
                    Box::leak(bytes.into_boxed_slice()).as_ptr().cast()
                })
                .collect();
            DescriptorSlot(LadspaDescriptor {
                unique_id: UNIQUE_ID,
                label: c"luna_eq".as_ptr(),
                properties: LADSPA_PROPERTY_HARD_RT_CAPABLE,
                name: c"Luna Equalizer".as_ptr(),
                maker: c"Luna".as_ptr(),
                copyright: c"GPL-3.0-or-later".as_ptr(),
                port_count: N_PORTS as c_ulong,
                port_descriptors: PORT_DESCRIPTORS.as_ptr(),
                port_names: Box::leak(names.into_boxed_slice()).as_ptr(),
                port_range_hints: PORT_HINTS.as_ptr(),
                implementation_data: std::ptr::null_mut(),
                instantiate: Some(instantiate),
                connect_port: Some(connect_port),
                activate: Some(activate),
                run: Some(run),
                run_adding: None,
                set_run_adding_gain: None,
                deactivate: Some(deactivate),
                cleanup: Some(cleanup),
            })
        })
        .0
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Skuteczna wartość sygnału po odrzuceniu stanu przejściowego.
    fn rms_after(samples: &[f32], skip: usize) -> f32 {
        let tail = &samples[skip..];
        (tail.iter().map(|x| x * x).sum::<f32>() / tail.len() as f32).sqrt()
    }

    #[test]
    fn flat_eq_is_identity() {
        let mut chain = EqChain::new(48_000.0);
        let mut buffer: Vec<f32> = (0..256).map(|i| (i as f32 * 0.1).sin() * 0.5).collect();
        let original = buffer.clone();
        chain.process_buffer(&mut buffer);
        for (a, b) in buffer.iter().zip(original.iter()) {
            assert!((a - b).abs() < 1e-6, "{a} != {b}");
        }
    }

    #[test]
    fn preamp_scales_output() {
        let mut chain = EqChain::new(48_000.0);
        chain.set_gains(-6.02, [0.0; 10]);
        chain.update_coeffs();
        let gain = 10f32.powf(-6.02 / 20.0);
        for x in [0.5f32, -0.25, 0.0, 1.0] {
            assert!((chain.process(x) - x * gain).abs() < 1e-6);
        }
    }

    #[test]
    fn peaking_boosts_center_frequency() {
        let fs = 48_000.0;
        let freq = 1_000.0;
        let mut chain = EqChain::new(fs);
        let mut gains = [0.0; 10];
        gains[5] = 6.0;
        chain.set_gains(0.0, gains);
        chain.update_coeffs();

        let n = (fs * 2.0) as usize;
        let mut buffer: Vec<f32> = (0..n)
            .map(|i| (2.0 * std::f32::consts::PI * freq * i as f32 / fs).sin())
            .collect();
        chain.process_buffer(&mut buffer);

        let ratio = rms_after(&buffer, n / 2) / std::f32::consts::FRAC_1_SQRT_2;
        assert!((ratio - 2.0).abs() < 0.06, "wzmocnienie {ratio}");
    }

    #[test]
    fn peaking_cuts_center_frequency() {
        let fs = 48_000.0;
        let freq = 1_000.0;
        let mut chain = EqChain::new(fs);
        let mut gains = [0.0; 10];
        gains[5] = -6.0;
        chain.set_gains(0.0, gains);
        chain.update_coeffs();

        let n = (fs * 2.0) as usize;
        let mut buffer: Vec<f32> = (0..n)
            .map(|i| (2.0 * std::f32::consts::PI * freq * i as f32 / fs).sin())
            .collect();
        chain.process_buffer(&mut buffer);

        let ratio = rms_after(&buffer, n / 2) / std::f32::consts::FRAC_1_SQRT_2;
        assert!((ratio - 0.5).abs() < 0.02, "wzmocnienie {ratio}");
    }

    #[test]
    fn gains_are_clamped_and_non_finite_ignored() {
        let mut chain = EqChain::new(48_000.0);
        chain.set_gains(f32::NAN, [f32::INFINITY; 10]);
        let buffer = [0.0f32; 8];
        let mut buffer = buffer.to_vec();
        chain.process_buffer(&mut buffer);
        assert!(buffer.iter().all(|x| x.is_finite()));
    }

    #[test]
    fn band_above_nyquist_is_identity() {
        let filter = Biquad::peaking(32_000.0, 16_000.0, BAND_Q, 12.0);
        // 16 kHz > 0.48 * 32 kHz → filtr tożsamościowy, nie NaN.
        assert_eq!(filter.b0, 1.0);
        assert_eq!(filter.b1, 0.0);
    }

    #[test]
    fn descriptor_exposes_all_ports() {
        let desc = ladspa_descriptor(0);
        assert!(!desc.is_null());
        let desc = unsafe { &*desc };
        assert_eq!(desc.port_count, N_PORTS as c_ulong);
        assert!(ladspa_descriptor(1).is_null());
        // Instantiate/cleanup nie mogą wyciekać ani się wywracać.
        let handle = unsafe { instantiate(desc, 48_000) };
        assert!(!handle.is_null());
        unsafe {
            connect_port(handle, 0, std::ptr::null_mut());
            cleanup(handle);
        }
    }
}
