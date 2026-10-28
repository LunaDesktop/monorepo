//! Sonda i benchmark EGL/GLES — fundament decyzji o backendzie GPU.
//!
//! Uruchomienie (release obowiązkowo — na debug liczby nie mają sensu):
//!
//! ```sh
//! cargo run --release --manifest-path libluna/Cargo.toml \
//!     -p libluna-render --example gpu_probe
//!
//! # wynik do CI / porównań
//! cargo run --release ... --example gpu_probe -- --json
//! ```
//!
//! Program **nie zmienia** żadnego stanu systemu: otwiera EGL (najpierw
//! `EGL_PLATFORM_SURFACELESS_MESA`, potem domyślną platformę), tworzy kontekst
//! GLES2 i renderuje do FBO 1920×1080. Mierzy:
//!
//! - `clear` — czyszczenie całej klatki,
//! - `upload` — wysłanie tekstury 1920×1080 RGBA,
//! - `composite` — 30 okien 640×480 z blendingiem (typowa scena Selene),
//! - `readback` — odczyt klatki do pamięci (koszt ścieżki `wl_shm`/DRM),
//! - CPU — ta sama scena przez [`SoftwareRenderer`] (punkt odniesienia).
//!
//! Brak zależności od `libc`/`khronos-egl`: `dlopen` deklarujemy sami, a funkcje
//! ładujemy przez `eglGetProcAddress`. Dzięki temu backend można dodać bez
//! zmiany drzewa zależności (i bez sieci) — patrz `docs/gpu.md`.
//!
//! Kody wyjścia: `0` = GPU działa, `2` = brak `libEGL`/`libGLESv2`, `3` = brak
//! kontekstu (np. headless bez sterownika), `4` = błąd GL.

use std::ffi::{c_char, c_int, c_void, CStr};
use std::ptr;
use std::sync::Arc;
use std::time::Instant;

use libluna_core::{Color, Rect};
use libluna_render::{ImageData, Scene, SoftwareRenderer};

const W: u32 = 1920;
const H: u32 = 1080;
const WINDOWS: usize = 30;
const WIN_W: u32 = 640;
const WIN_H: u32 = 480;

// ── EGL ─────────────────────────────────────────────────────────────────────
const EGL_PLATFORM_SURFACELESS_MESA: u32 = 0x31DD;
const EGL_OPENGL_ES_API: u32 = 0x30A0;
const EGL_SURFACE_TYPE: u32 = 0x3033;
const EGL_WINDOW_BIT: u32 = 0x0004;
const EGL_PBUFFER_BIT: u32 = 0x0001;
const EGL_RENDERABLE_TYPE: u32 = 0x3040;
const EGL_OPENGL_ES2_BIT: u32 = 0x0004;
const EGL_RED_SIZE: u32 = 0x3024;
const EGL_GREEN_SIZE: u32 = 0x3023;
const EGL_BLUE_SIZE: u32 = 0x3022;
const EGL_ALPHA_SIZE: u32 = 0x3021;
const EGL_NONE: u32 = 0x3038;
const EGL_CONTEXT_CLIENT_VERSION: u32 = 0x3098;
const EGL_EXTENSIONS: u32 = 0x3055;
const EGL_VENDOR: u32 = 0x3053;
const EGL_VERSION: u32 = 0x3054;

// ── GLES2 ───────────────────────────────────────────────────────────────────
const GL_TEXTURE_2D: u32 = 0x0DE1;
const GL_RGBA: u32 = 0x1908;
const GL_UNSIGNED_BYTE: u32 = 0x1401;
const GL_TEXTURE_MIN_FILTER: u32 = 0x2801;
const GL_TEXTURE_MAG_FILTER: u32 = 0x2800;
const GL_LINEAR: u32 = 0x2601;
const GL_TEXTURE_WRAP_S: u32 = 0x2802;
const GL_TEXTURE_WRAP_T: u32 = 0x2803;
const GL_CLAMP_TO_EDGE: u32 = 0x812F;
const GL_FRAMEBUFFER: u32 = 0x8D40;
const GL_COLOR_ATTACHMENT0: u32 = 0x8CE0;
const GL_FRAMEBUFFER_COMPLETE: u32 = 0x8CD5;
const GL_COLOR_BUFFER_BIT: u32 = 0x4000;
const GL_BLEND: u32 = 0x0BE2;
const GL_SRC_ALPHA: u32 = 0x0302;
const GL_ONE_MINUS_SRC_ALPHA: u32 = 0x0303;
const GL_ARRAY_BUFFER: u32 = 0x8892;
const GL_STATIC_DRAW: u32 = 0x88E4;
const GL_FLOAT: u32 = 0x1406;
const GL_VERTEX_SHADER: u32 = 0x8B31;
const GL_FRAGMENT_SHADER: u32 = 0x8B30;
const GL_COMPILE_STATUS: u32 = 0x8B81;
const GL_LINK_STATUS: u32 = 0x8B82;
const GL_TEXTURE0: u32 = 0x84C0;
const GL_TRIANGLE_STRIP: u32 = 0x0005;
const GL_RENDERER: u32 = 0x1F01;
const GL_VERSION: u32 = 0x1F02;
const GL_VENDOR: u32 = 0x1F00;

type Gluint = u32;
type Glint = i32;
type Glsizei = i32;
type Glboolean = u8;
type Glchar = c_char;

#[link(name = "dl")]
extern "C" {
    fn dlopen(filename: *const c_char, flags: c_int) -> *mut c_void;
    fn dlsym(handle: *mut c_void, symbol: *const c_char) -> *mut c_void;
    fn dlerror() -> *mut c_char;
}

const RTLD_NOW: c_int = 2;

// Podpisy EGL (tylko to, czego używa sonda).
type EglGetProcAddress = unsafe extern "C" fn(*const c_char) -> *mut c_void;
type EglGetPlatformDisplay = unsafe extern "C" fn(u32, *mut c_void, *const i32) -> *mut c_void;
type EglGetDisplay = unsafe extern "C" fn(*mut c_void) -> *mut c_void;
type EglInitialize = unsafe extern "C" fn(*mut c_void, *mut i32, *mut i32) -> u32;
type EglQueryString = unsafe extern "C" fn(*mut c_void, u32) -> *const c_char;
type EglBindApi = unsafe extern "C" fn(u32) -> u32;
type EglChooseConfig =
    unsafe extern "C" fn(*mut c_void, *const i32, *mut *mut c_void, i32, *mut i32) -> u32;
type EglCreateContext =
    unsafe extern "C" fn(*mut c_void, *mut c_void, *mut c_void, *const i32) -> *mut c_void;
type EglMakeCurrent =
    unsafe extern "C" fn(*mut c_void, *mut c_void, *mut c_void, *mut c_void) -> u32;
type EglGetError = unsafe extern "C" fn() -> i32;

// Podpisy GLES2.
type GlGetString = unsafe extern "C" fn(u32) -> *const u8;
type GlGenTextures = unsafe extern "C" fn(i32, *mut Gluint);
type GlBindTexture = unsafe extern "C" fn(u32, Gluint);
type GlTexImage2D = unsafe extern "C" fn(u32, i32, i32, i32, i32, i32, u32, u32, *const c_void);
type GlTexSubImage2D = unsafe extern "C" fn(u32, i32, i32, i32, i32, i32, u32, u32, *const c_void);
type GlTexParameteri = unsafe extern "C" fn(u32, u32, i32);
type GlActiveTexture = unsafe extern "C" fn(u32);
type GlGenFramebuffers = unsafe extern "C" fn(i32, *mut Gluint);
type GlBindFramebuffer = unsafe extern "C" fn(u32, Gluint);
type GlFramebufferTexture2D = unsafe extern "C" fn(u32, u32, u32, Gluint, i32);
type GlCheckFramebufferStatus = unsafe extern "C" fn(u32) -> u32;
type GlViewport = unsafe extern "C" fn(i32, i32, i32, i32);
type GlClearColor = unsafe extern "C" fn(f32, f32, f32, f32);
type GlClear = unsafe extern "C" fn(u32);
type GlEnable = unsafe extern "C" fn(u32);
type GlBlendFunc = unsafe extern "C" fn(u32, u32);
type GlGenBuffers = unsafe extern "C" fn(i32, *mut Gluint);
type GlBindBuffer = unsafe extern "C" fn(u32, Gluint);
type GlBufferData = unsafe extern "C" fn(u32, isize, *const c_void, u32);
type GlCreateShader = unsafe extern "C" fn(u32) -> Gluint;
type GlShaderSource = unsafe extern "C" fn(Gluint, i32, *const *const Glchar, *const i32);
type GlCompileShader = unsafe extern "C" fn(Gluint);
type GlGetShaderiv = unsafe extern "C" fn(Gluint, u32, *mut Glint);
type GlGetShaderInfoLog = unsafe extern "C" fn(Gluint, Glsizei, *mut Glsizei, *mut Glchar);
type GlCreateProgram = unsafe extern "C" fn() -> Gluint;
type GlAttachShader = unsafe extern "C" fn(Gluint, Gluint);
type GlLinkProgram = unsafe extern "C" fn(Gluint);
type GlGetProgramiv = unsafe extern "C" fn(Gluint, u32, *mut Glint);
type GlGetProgramInfoLog = unsafe extern "C" fn(Gluint, Glsizei, *mut Glsizei, *mut Glchar);
type GlUseProgram = unsafe extern "C" fn(Gluint);
type GlGetUniformLocation = unsafe extern "C" fn(Gluint, *const Glchar) -> Glint;
type GlGetAttribLocation = unsafe extern "C" fn(Gluint, *const Glchar) -> Glint;
type GlUniform2f = unsafe extern "C" fn(Glint, f32, f32);
type GlUniform4f = unsafe extern "C" fn(Glint, f32, f32, f32, f32);
type GlUniform1i = unsafe extern "C" fn(Glint, i32);
type GlVertexAttribPointer = unsafe extern "C" fn(Gluint, i32, u32, Glboolean, i32, *const c_void);
type GlEnableVertexAttribArray = unsafe extern "C" fn(Gluint);
type GlDrawArrays = unsafe extern "C" fn(u32, i32, i32);
type GlReadPixels = unsafe extern "C" fn(i32, i32, i32, i32, u32, u32, *mut c_void);
type GlFinish = unsafe extern "C" fn();

/// Załadowane EGL + GLES2 (jedna instancja na proces).
struct Gl {
    egl: Egl,
    name: String,
    vendor: String,
    version: String,
    extensions: String,
    egl_version: String,
    egl_vendor: String,
}

struct Egl {
    /// Trzymane tylko po to, żeby kontekst i display żyły do końca procesu.
    _dpy: *mut c_void,
    _ctx: *mut c_void,
    get_error: EglGetError,
    // GL
    gen_textures: GlGenTextures,
    bind_texture: GlBindTexture,
    tex_image_2d: GlTexImage2D,
    tex_sub_image_2d: GlTexSubImage2D,
    tex_parameteri: GlTexParameteri,
    active_texture: GlActiveTexture,
    gen_framebuffers: GlGenFramebuffers,
    bind_framebuffer: GlBindFramebuffer,
    framebuffer_texture_2d: GlFramebufferTexture2D,
    check_framebuffer_status: GlCheckFramebufferStatus,
    viewport: GlViewport,
    clear_color: GlClearColor,
    clear: GlClear,
    enable: GlEnable,
    blend_func: GlBlendFunc,
    gen_buffers: GlGenBuffers,
    bind_buffer: GlBindBuffer,
    buffer_data: GlBufferData,
    create_shader: GlCreateShader,
    shader_source: GlShaderSource,
    compile_shader: GlCompileShader,
    get_shaderiv: GlGetShaderiv,
    get_shader_info_log: GlGetShaderInfoLog,
    create_program: GlCreateProgram,
    attach_shader: GlAttachShader,
    link_program: GlLinkProgram,
    get_programiv: GlGetProgramiv,
    get_program_info_log: GlGetProgramInfoLog,
    use_program: GlUseProgram,
    get_uniform_location: GlGetUniformLocation,
    get_attrib_location: GlGetAttribLocation,
    uniform_2f: GlUniform2f,
    uniform_4f: GlUniform4f,
    uniform_1i: GlUniform1i,
    vertex_attrib_pointer: GlVertexAttribPointer,
    enable_vertex_attrib_array: GlEnableVertexAttribArray,
    draw_arrays: GlDrawArrays,
    read_pixels: GlReadPixels,
    finish: GlFinish,
}

impl Gl {
    fn open() -> Result<Self, (i32, String)> {
        let egl_lib = dlopen_cstr("libEGL.so.1")
            .or_else(|| dlopen_cstr("libEGL.so"))
            .ok_or((2, "brak libEGL".to_owned()))?;
        let gles_lib = dlopen_cstr("libGLESv2.so.2")
            .or_else(|| dlopen_cstr("libGLESv2.so"))
            .ok_or((2, "brak libGLESv2".to_owned()))?;

        unsafe {
            let get_proc: EglGetProcAddress =
                std::mem::transmute(sym(egl_lib, "eglGetProcAddress")?);
            let egl_get_platform_display: Option<EglGetPlatformDisplay> =
                load_proc(get_proc, "eglGetPlatformDisplayEXT");
            let egl_get_display: Option<EglGetDisplay> = load_proc(get_proc, "eglGetDisplay");

            // 1) surfaceless (Mesa/NVIDIA obsługują to bez X11/Waylanda),
            // 2) domyślna platforma — sensowna, gdy działa DISPLAY/WAYLAND_DISPLAY.
            let mut dpy = ptr::null_mut();
            if let Some(get_display) = egl_get_platform_display {
                dpy = get_display(EGL_PLATFORM_SURFACELESS_MESA, ptr::null_mut(), ptr::null());
            }
            let mut platform = "surfaceless";
            if dpy.is_null() {
                if let Some(get_display) = egl_get_display {
                    dpy = get_display(ptr::null_mut()); // EGL_DEFAULT_DISPLAY
                    platform = "default";
                }
            }
            if dpy.is_null() {
                return Err((3, "nie udało się uzyskać EGLDisplay".to_owned()));
            }

            let initialize: EglInitialize = std::mem::transmute(sym(egl_lib, "eglInitialize")?);
            let (mut maj, mut min) = (0, 0);
            if initialize(dpy, &mut maj, &mut min) == 0 {
                return Err((
                    3,
                    format!("eglInitialize nie powiodło się (platforma {platform})"),
                ));
            }
            let egl_query: EglQueryString = std::mem::transmute(sym(egl_lib, "eglQueryString")?);
            let egl_bind: EglBindApi = std::mem::transmute(sym(egl_lib, "eglBindAPI")?);
            let egl_choose: EglChooseConfig = std::mem::transmute(sym(egl_lib, "eglChooseConfig")?);
            let egl_create_context: EglCreateContext =
                std::mem::transmute(sym(egl_lib, "eglCreateContext")?);
            let egl_make_current: EglMakeCurrent =
                std::mem::transmute(sym(egl_lib, "eglMakeCurrent")?);
            let egl_get_error: EglGetError = std::mem::transmute(sym(egl_lib, "eglGetError")?);

            if egl_bind(EGL_OPENGL_ES_API) == 0 {
                return Err((3, format!("eglBindAPI: 0x{:x}", egl_get_error())));
            }
            let mut config = ptr::null_mut();
            let mut n = 0;
            // Kolejność jak w praktyce: okno (najszersza zgodność), pbuffer, bez powierzchni.
            for bits in [EGL_WINDOW_BIT, EGL_PBUFFER_BIT, 0] {
                let attribs: [i32; 13] = [
                    EGL_SURFACE_TYPE as i32,
                    bits as i32,
                    EGL_RENDERABLE_TYPE as i32,
                    EGL_OPENGL_ES2_BIT as i32,
                    EGL_RED_SIZE as i32,
                    8,
                    EGL_GREEN_SIZE as i32,
                    8,
                    EGL_BLUE_SIZE as i32,
                    8,
                    EGL_ALPHA_SIZE as i32,
                    8,
                    EGL_NONE as i32,
                ];
                if egl_choose(dpy, attribs.as_ptr(), &mut config, 1, &mut n) != 0 && n >= 1 {
                    break;
                }
            }
            if n < 1 || config.is_null() {
                return Err((3, format!("eglChooseConfig: 0x{:x}", egl_get_error())));
            }
            let ctx_attribs: [i32; 3] = [EGL_CONTEXT_CLIENT_VERSION as i32, 2, EGL_NONE as i32];
            let ctx = egl_create_context(dpy, config, ptr::null_mut(), ctx_attribs.as_ptr());
            if ctx.is_null() {
                return Err((3, format!("eglCreateContext: 0x{:x}", egl_get_error())));
            }
            if egl_make_current(dpy, ptr::null_mut(), ptr::null_mut(), ctx) == 0 {
                return Err((3, format!("eglMakeCurrent: 0x{:x}", egl_get_error())));
            }

            // GL ładowany przez eglGetProcAddress z fallbackiem do dlsym.
            macro_rules! glfn {
                ($name:literal) => {
                    load_proc(get_proc, $name)
                        .or_else(|| dlsym_fn(gles_lib, $name))
                        .ok_or((4, format!("brak funkcji {}", $name)))?
                };
            }
            let get_string: GlGetString = glfn!("glGetString");
            let gl_vendor = cstr_u8(get_string(GL_VENDOR));
            let gl_renderer = cstr_u8(get_string(GL_RENDERER));
            let gl_version = cstr_u8(get_string(GL_VERSION));
            let gl_extensions = cstr_u8(get_string(0x1F03)); // GL_EXTENSIONS
            let egl_extensions = cstr(egl_query(dpy, EGL_EXTENSIONS));
            let egl_vendor = cstr(egl_query(dpy, EGL_VENDOR));
            let egl_version = cstr(egl_query(dpy, EGL_VERSION));

            Ok(Self {
                egl: Egl {
                    _dpy: dpy,
                    _ctx: ctx,
                    get_error: egl_get_error,
                    gen_textures: glfn!("glGenTextures"),
                    bind_texture: glfn!("glBindTexture"),
                    tex_image_2d: glfn!("glTexImage2D"),
                    tex_sub_image_2d: glfn!("glTexSubImage2D"),
                    tex_parameteri: glfn!("glTexParameteri"),
                    active_texture: glfn!("glActiveTexture"),
                    gen_framebuffers: glfn!("glGenFramebuffers"),
                    bind_framebuffer: glfn!("glBindFramebuffer"),
                    framebuffer_texture_2d: glfn!("glFramebufferTexture2D"),
                    check_framebuffer_status: glfn!("glCheckFramebufferStatus"),
                    viewport: glfn!("glViewport"),
                    clear_color: glfn!("glClearColor"),
                    clear: glfn!("glClear"),
                    enable: glfn!("glEnable"),
                    blend_func: glfn!("glBlendFunc"),
                    gen_buffers: glfn!("glGenBuffers"),
                    bind_buffer: glfn!("glBindBuffer"),
                    buffer_data: glfn!("glBufferData"),
                    create_shader: glfn!("glCreateShader"),
                    shader_source: glfn!("glShaderSource"),
                    compile_shader: glfn!("glCompileShader"),
                    get_shaderiv: glfn!("glGetShaderiv"),
                    get_shader_info_log: glfn!("glGetShaderInfoLog"),
                    create_program: glfn!("glCreateProgram"),
                    attach_shader: glfn!("glAttachShader"),
                    link_program: glfn!("glLinkProgram"),
                    get_programiv: glfn!("glGetProgramiv"),
                    get_program_info_log: glfn!("glGetProgramInfoLog"),
                    use_program: glfn!("glUseProgram"),
                    get_uniform_location: glfn!("glGetUniformLocation"),
                    get_attrib_location: glfn!("glGetAttribLocation"),
                    uniform_2f: glfn!("glUniform2f"),
                    uniform_4f: glfn!("glUniform4f"),
                    uniform_1i: glfn!("glUniform1i"),
                    vertex_attrib_pointer: glfn!("glVertexAttribPointer"),
                    enable_vertex_attrib_array: glfn!("glEnableVertexAttribArray"),
                    draw_arrays: glfn!("glDrawArrays"),
                    read_pixels: glfn!("glReadPixels"),
                    finish: glfn!("glFinish"),
                },
                name: gl_renderer,
                vendor: gl_vendor,
                version: gl_version,
                extensions: format!("{gl_extensions} {egl_extensions}"),
                egl_version,
                egl_vendor,
            })
        }
    }

    fn error(&self) -> i32 {
        unsafe { (self.egl.get_error)() }
    }
}

fn dlopen_cstr(name: &str) -> Option<*mut c_void> {
    let c = std::ffi::CString::new(name).ok()?;
    let handle = unsafe { dlopen(c.as_ptr(), RTLD_NOW) };
    (!handle.is_null()).then_some(handle)
}

unsafe fn sym(handle: *mut c_void, name: &str) -> Result<*mut c_void, (i32, String)> {
    let c = std::ffi::CString::new(name).map_err(|_| (4, "zła nazwa symbolu".to_owned()))?;
    let p = dlsym(handle, c.as_ptr());
    if p.is_null() {
        let err = dlerror();
        let msg = if err.is_null() {
            name.to_owned()
        } else {
            CStr::from_ptr(err).to_string_lossy().into_owned()
        };
        Err((4, format!("dlsym {name}: {msg}")))
    } else {
        Ok(p)
    }
}

unsafe fn load_proc<T: Copy>(get_proc: EglGetProcAddress, name: &str) -> Option<T> {
    let c = std::ffi::CString::new(name).ok()?;
    let p = get_proc(c.as_ptr());
    if p.is_null() {
        None
    } else {
        Some(std::mem::transmute_copy::<*mut c_void, T>(&p))
    }
}

unsafe fn dlsym_fn<T: Copy>(handle: *mut c_void, name: &str) -> Option<T> {
    let c = std::ffi::CString::new(name).ok()?;
    let p = dlsym(handle, c.as_ptr());
    if p.is_null() {
        None
    } else {
        Some(std::mem::transmute_copy::<*mut c_void, T>(&p))
    }
}

fn cstr(p: *const c_char) -> String {
    if p.is_null() {
        return String::new();
    }
    unsafe { CStr::from_ptr(p).to_string_lossy().into_owned() }
}

fn cstr_u8(p: *const u8) -> String {
    if p.is_null() {
        return String::new();
    }
    unsafe {
        CStr::from_ptr(p as *const c_char)
            .to_string_lossy()
            .into_owned()
    }
}

fn has_ext(ext: &str, name: &str) -> bool {
    ext.split_whitespace().any(|e| e == name)
}

fn main() {
    let json = std::env::args().skip(1).any(|a| a == "--json");
    let gl = match Gl::open() {
        Ok(gl) => gl,
        Err((code, msg)) => {
            if json {
                println!(
                    "{{\"ok\":false,\"code\":{code},\"error\":{}}}",
                    json_str(&msg)
                );
            } else {
                eprintln!("GPU: {msg}");
            }
            std::process::exit(code);
        }
    };

    if !json {
        println!("EGL {} ({})", gl.egl_version, gl.egl_vendor);
        println!("GL  {} ({})", gl.version, gl.vendor);
        println!("GPU {}", gl.name);
        for ext in [
            "EGL_EXT_image_dma_buf_import",
            "EGL_EXT_image_dma_buf_import_modifiers",
            "EGL_KHR_fence_sync",
            "EGL_KHR_surfaceless_context",
            "EGL_KHR_swap_buffers_with_damage",
            "EGL_ANDROID_native_fence_sync",
            "GL_OES_EGL_image",
        ] {
            println!("  {:<42} {}", ext, yes_no(has_ext(&gl.extensions, ext)));
        }
        println!();
    }

    let frame = Fbo::new(&gl);
    if frame.status != GL_FRAMEBUFFER_COMPLETE {
        eprintln!("GPU: FBO niekompletne (0x{:x})", frame.status);
        std::process::exit(4);
    }
    let prog = Program::new(&gl).unwrap_or_else(|msg| {
        eprintln!("GPU: shadery: {msg} (GL error 0x{:x})", gl.error());
        std::process::exit(4)
    });
    let quad = Quad::new(&gl);

    // ── 1) clear ────────────────────────────────────────────────────────────
    let iters = 60;
    let t = Instant::now();
    for i in 0..iters {
        unsafe {
            (gl.egl.clear_color)(0.05 + i as f32 * 0.001, 0.07, 0.1, 1.0);
            (gl.egl.clear)(GL_COLOR_BUFFER_BIT);
        }
    }
    unsafe { (gl.egl.finish)() };
    let clear_ms = ms_per(t.elapsed(), iters);

    // ── 2) upload 1920×1080 ─────────────────────────────────────────────────
    let big = vec![0x80u8; (W * H * 4) as usize];
    let upload_tex = Texture::new(&gl, W, H);
    let iters = 20;
    let t = Instant::now();
    for _ in 0..iters {
        upload_tex.upload(&gl, W, H, &big);
    }
    unsafe { (gl.egl.finish)() };
    let upload_ms = ms_per(t.elapsed(), iters);

    // ── 3) composite 30 okien (GPU) ─────────────────────────────────────────
    let windows: Vec<Texture> = (0..WINDOWS)
        .map(|i| {
            let tex = Texture::new(&gl, WIN_W, WIN_H);
            let px = patterned((i as f32 * 0.11).fract(), i as u8);
            tex.upload(&gl, WIN_W, WIN_H, &px);
            tex
        })
        .collect();
    let positions = window_positions();

    let iters = 30;
    let t = Instant::now();
    for _ in 0..iters {
        unsafe {
            (gl.egl.bind_framebuffer)(GL_FRAMEBUFFER, frame.fbo);
            (gl.egl.viewport)(0, 0, W as i32, H as i32);
            (gl.egl.clear_color)(0.06, 0.08, 0.11, 1.0);
            (gl.egl.clear)(GL_COLOR_BUFFER_BIT);
            (gl.egl.enable)(GL_BLEND);
            (gl.egl.blend_func)(GL_SRC_ALPHA, GL_ONE_MINUS_SRC_ALPHA);
        }
        prog.use_(&gl);
        quad.bind(&gl, &prog);
        for (tex, (x, y)) in windows.iter().zip(positions.iter()) {
            prog.draw(&gl, tex.id, *x, *y, WIN_W, WIN_H, 0.92);
        }
        unsafe { (gl.egl.finish)() };
    }
    let gpu_ms = ms_per(t.elapsed(), iters);

    // ── 4) readback 1920×1080 ───────────────────────────────────────────────
    let mut out = vec![0u8; (W * H * 4) as usize];
    let iters = 20;
    let t = Instant::now();
    for _ in 0..iters {
        unsafe {
            (gl.egl.read_pixels)(
                0,
                0,
                W as i32,
                H as i32,
                GL_RGBA,
                GL_UNSIGNED_BYTE,
                out.as_mut_ptr() as *mut c_void,
            );
        }
    }
    unsafe { (gl.egl.finish)() };
    let readback_ms = ms_per(t.elapsed(), iters);
    let nonzero = out.chunks_exact(4).filter(|p| p[3] != 0).count();

    // ── 5) CPU: ta sama scena przez SoftwareRenderer ────────────────────────
    let images: Vec<Arc<ImageData>> = (0..WINDOWS)
        .map(|i| {
            let px = patterned((i as f32 * 0.11).fract(), i as u8);
            Arc::new(ImageData::new(WIN_W, WIN_H, px))
        })
        .collect();
    let mut renderer = SoftwareRenderer::new(W, H);
    let mut scene = Scene::new();
    let iters = 10;
    let t = Instant::now();
    for _ in 0..iters {
        scene.clear();
        for (img, (x, y)) in images.iter().zip(positions.iter()) {
            scene.image(
                Rect::from_xywh(*x, *y, WIN_W as f64, WIN_H as f64),
                img.clone(),
            );
        }
        renderer.draw(&scene, 1.0, Color::from_rgba8(0x10, 0x14, 0x1c, 0xff));
    }
    let cpu_ms = ms_per(t.elapsed(), iters);

    if json {
        println!(
            "{{\"ok\":true,\"gpu\":{},\"gl\":{},\"egl\":{},\"clear_ms\":{:.2},\"upload_ms\":{:.2},\"composite_ms\":{:.2},\"readback_ms\":{:.2},\"cpu_ms\":{:.2},\"windows\":{},\"dmabuf_import\":{}}}",
            json_str(&gl.name),
            json_str(&gl.version),
            json_str(&gl.egl_version),
            clear_ms,
            upload_ms,
            gpu_ms,
            readback_ms,
            cpu_ms,
            WINDOWS,
            has_ext(&gl.extensions, "EGL_EXT_image_dma_buf_import")
        );
        return;
    }

    println!("Klatka 1920×1080 ({} okien {}×{}):", WINDOWS, WIN_W, WIN_H);
    println!("  clear        {clear_ms:6.2} ms");
    println!("  upload       {upload_ms:6.2} ms  (1920×1080 RGBA)");
    println!(
        "  composite    {gpu_ms:6.2} ms  GPU  → {:.1} fps",
        1000.0 / gpu_ms
    );
    println!("  readback     {readback_ms:6.2} ms  (niezerowe px: {nonzero})");
    println!(
        "  CPU scene    {cpu_ms:6.2} ms  → {:.1} fps",
        1000.0 / cpu_ms
    );
    println!(
        "  GPU/CPU      {:.2}×  (kompozycja bez readbacku)",
        cpu_ms / gpu_ms
    );
    let budget = gpu_ms + readback_ms;
    println!(
        "  razem GPU    {budget:6.2} ms  → {:.1} fps  (kompozycja + readback)",
        1000.0 / budget
    );
}

fn ms_per(elapsed: std::time::Duration, iters: u32) -> f64 {
    elapsed.as_secs_f64() * 1000.0 / iters as f64
}

fn yes_no(v: bool) -> &'static str {
    if v {
        "tak"
    } else {
        "nie"
    }
}

fn json_str(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            c if (c as u32) < 0x20 => out.push(' '),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Pozycje okien w stylu kaskady (te same dla GPU i CPU).
fn window_positions() -> Vec<(f64, f64)> {
    (0..WINDOWS)
        .map(|i| {
            let i = i as f64;
            (
                (i * 41.0) % (W as f64 - WIN_W as f64),
                (i * 29.0) % (H as f64 - WIN_H as f64),
            )
        })
        .collect()
}

/// Prosty wzór RGBA — tekstura bez ładowania plików.
fn patterned(tint: f32, seed: u8) -> Vec<u8> {
    let mut px = vec![0u8; (WIN_W * WIN_H * 4) as usize];
    for y in 0..WIN_H {
        for x in 0..WIN_W {
            let i = ((y * WIN_W + x) * 4) as usize;
            let n = ((x / 16 + y / 16 + seed as u32) % 2) as u8;
            px[i] = (60.0 + 120.0 * tint) as u8 + n * 40;
            px[i + 1] = 80 + n * 30;
            px[i + 2] = (120.0 + 80.0 * (1.0 - tint)) as u8;
            px[i + 3] = 255;
        }
    }
    px
}

struct Texture {
    id: Gluint,
}

impl Texture {
    fn new(gl: &Gl, w: u32, h: u32) -> Self {
        let mut id = 0;
        unsafe {
            (gl.egl.gen_textures)(1, &mut id);
            (gl.egl.bind_texture)(GL_TEXTURE_2D, id);
            (gl.egl.tex_image_2d)(
                GL_TEXTURE_2D,
                0,
                GL_RGBA as i32,
                w as i32,
                h as i32,
                0,
                GL_RGBA,
                GL_UNSIGNED_BYTE,
                ptr::null(),
            );
            (gl.egl.tex_parameteri)(GL_TEXTURE_2D, GL_TEXTURE_MIN_FILTER, GL_LINEAR as i32);
            (gl.egl.tex_parameteri)(GL_TEXTURE_2D, GL_TEXTURE_MAG_FILTER, GL_LINEAR as i32);
            (gl.egl.tex_parameteri)(GL_TEXTURE_2D, GL_TEXTURE_WRAP_S, GL_CLAMP_TO_EDGE as i32);
            (gl.egl.tex_parameteri)(GL_TEXTURE_2D, GL_TEXTURE_WRAP_T, GL_CLAMP_TO_EDGE as i32);
        }
        Self { id }
    }

    fn upload(&self, gl: &Gl, w: u32, h: u32, rgba: &[u8]) {
        unsafe {
            (gl.egl.bind_texture)(GL_TEXTURE_2D, self.id);
            (gl.egl.tex_sub_image_2d)(
                GL_TEXTURE_2D,
                0,
                0,
                0,
                w as i32,
                h as i32,
                GL_RGBA,
                GL_UNSIGNED_BYTE,
                rgba.as_ptr() as *const c_void,
            );
        }
    }
}

struct Fbo {
    fbo: Gluint,
    status: u32,
}

impl Fbo {
    fn new(gl: &Gl) -> Self {
        let tex = Texture::new(gl, W, H);
        let mut fbo = 0;
        unsafe {
            (gl.egl.gen_framebuffers)(1, &mut fbo);
            (gl.egl.bind_framebuffer)(GL_FRAMEBUFFER, fbo);
            (gl.egl.framebuffer_texture_2d)(
                GL_FRAMEBUFFER,
                GL_COLOR_ATTACHMENT0,
                GL_TEXTURE_2D,
                tex.id,
                0,
            );
        }
        let status = unsafe { (gl.egl.check_framebuffer_status)(GL_FRAMEBUFFER) };
        Self { fbo, status }
    }
}

struct Quad {
    vbo: Gluint,
}

impl Quad {
    fn new(gl: &Gl) -> Self {
        let verts: [f32; 8] = [0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 1.0, 1.0];
        let mut vbo = 0;
        unsafe {
            (gl.egl.gen_buffers)(1, &mut vbo);
            (gl.egl.bind_buffer)(GL_ARRAY_BUFFER, vbo);
            (gl.egl.buffer_data)(
                GL_ARRAY_BUFFER,
                std::mem::size_of_val(&verts) as isize,
                verts.as_ptr() as *const c_void,
                GL_STATIC_DRAW,
            );
        }
        Self { vbo }
    }

    fn bind(&self, gl: &Gl, prog: &Program) {
        unsafe {
            (gl.egl.bind_buffer)(GL_ARRAY_BUFFER, self.vbo);
            (gl.egl.enable_vertex_attrib_array)(prog.pos_attr as u32);
            (gl.egl.vertex_attrib_pointer)(prog.pos_attr as u32, 2, GL_FLOAT, 0, 0, ptr::null());
        }
    }
}

struct Program {
    id: Gluint,
    pos_attr: Glint,
    screen: Glint,
    rect: Glint,
    tint: Glint,
    sampler: Glint,
}

impl Program {
    fn new(gl: &Gl) -> Result<Self, String> {
        let vs = shader(
            gl,
            GL_VERTEX_SHADER,
            "attribute vec2 pos;
             uniform vec2 screen;
             uniform vec4 rect;
             varying vec2 uv;
             void main() {
                 uv = pos;
                 vec2 p = rect.xy + pos * rect.zw;
                 gl_Position = vec4(p.x / screen.x * 2.0 - 1.0,
                                    1.0 - p.y / screen.y * 2.0, 0.0, 1.0);
             }",
        )?;
        let fs = shader(
            gl,
            GL_FRAGMENT_SHADER,
            "precision mediump float;
             uniform sampler2D tex;
             uniform vec4 tint;
             varying vec2 uv;
             void main() { gl_FragColor = texture2D(tex, uv) * tint; }",
        )?;
        let id = unsafe { (gl.egl.create_program)() };
        unsafe {
            (gl.egl.attach_shader)(id, vs);
            (gl.egl.attach_shader)(id, fs);
            (gl.egl.link_program)(id);
        }
        let mut ok = 0;
        unsafe { (gl.egl.get_programiv)(id, GL_LINK_STATUS, &mut ok) };
        if ok == 0 {
            let mut log = [0 as Glchar; 1024];
            unsafe { (gl.egl.get_program_info_log)(id, 1024, ptr::null_mut(), log.as_mut_ptr()) };
            let msg = unsafe { CStr::from_ptr(log.as_ptr()) }
                .to_string_lossy()
                .into_owned();
            return Err(msg);
        }
        unsafe {
            (gl.egl.use_program)(id);
            Ok(Self {
                id,
                pos_attr: (gl.egl.get_attrib_location)(id, b"pos\0".as_ptr() as *const Glchar),
                screen: (gl.egl.get_uniform_location)(id, b"screen\0".as_ptr() as *const Glchar),
                rect: (gl.egl.get_uniform_location)(id, b"rect\0".as_ptr() as *const Glchar),
                tint: (gl.egl.get_uniform_location)(id, b"tint\0".as_ptr() as *const Glchar),
                sampler: (gl.egl.get_uniform_location)(id, b"tex\0".as_ptr() as *const Glchar),
            })
        }
    }

    fn use_(&self, gl: &Gl) {
        unsafe { (gl.egl.use_program)(self.id) };
    }

    #[allow(clippy::too_many_arguments)]
    fn draw(&self, gl: &Gl, tex: Gluint, x: f64, y: f64, w: u32, h: u32, alpha: f32) {
        unsafe {
            (gl.egl.active_texture)(GL_TEXTURE0);
            (gl.egl.bind_texture)(GL_TEXTURE_2D, tex);
            (gl.egl.uniform_1i)(self.sampler, 0);
            (gl.egl.uniform_2f)(self.screen, W as f32, H as f32);
            (gl.egl.uniform_4f)(self.rect, x as f32, y as f32, w as f32, h as f32);
            (gl.egl.uniform_4f)(self.tint, 1.0, 1.0, 1.0, alpha);
            (gl.egl.draw_arrays)(GL_TRIANGLE_STRIP, 0, 4);
        }
    }
}

fn shader(gl: &Gl, kind: u32, src: &str) -> Result<Gluint, String> {
    let id = unsafe { (gl.egl.create_shader)(kind) };
    let c = std::ffi::CString::new(src).map_err(|_| "NUL w shaderze".to_owned())?;
    let ptrs = [c.as_ptr() as *const Glchar];
    unsafe {
        (gl.egl.shader_source)(id, 1, ptrs.as_ptr(), ptr::null());
        (gl.egl.compile_shader)(id);
    }
    let mut ok = 0;
    unsafe { (gl.egl.get_shaderiv)(id, GL_COMPILE_STATUS, &mut ok) };
    if ok == 0 {
        let mut log = [0 as Glchar; 1024];
        unsafe { (gl.egl.get_shader_info_log)(id, 1024, ptr::null_mut(), log.as_mut_ptr()) };
        let msg = unsafe { CStr::from_ptr(log.as_ptr()) }
            .to_string_lossy()
            .into_owned();
        return Err(msg);
    }
    Ok(id)
}
