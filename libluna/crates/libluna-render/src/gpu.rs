//! Backend GPU za [`Renderer`](crate::Renderer): EGL (surfaceless) + GLES2.
//!
//! Kontrakt jest ten sam co dla CPU — `draw(&Scene, scale, background)` i
//! `data() -> &[u8]` — więc widgety i platforma nie wiedzą, co rasteryzuje scenę.
//!
//! Strategia (etap 1 z `docs/gpu.md`):
//! - **GPU**: tło, obrazy (bufory okien), prostokąty, zaokrąglenia, obwódki,
//!   okręgi i clip (prostokątny oraz zaokrąglony, przez SDF w shaderze).
//! - **CPU (warstwy)**: tekst, łamane i wielokąty (ikony, kursor, tytuły SSD).
//!   Kolejne takie polecenia trafiają do małej przezroczystej warstwy, która jest
//!   rasteryzowana przez [`SoftwareRenderer`] i wstawiana w tej samej kolejności
//!   `z` co reszta sceny. Dzięki temu ikony i napisy wyglądają identycznie jak
//!   na CPU, a pełnoekranowe mieszanie pikseli robi GPU.
//! - **Wyjście**: `glReadPixels` do bufora RGBA (na NVIDIA proprietary nie ma
//!   `EGL_PLATFORM_GBM`; zero-copy zostaje na później — patrz `docs/gpu.md`).
//!
//! Fallback: brak `libEGL`/kontekstu = [`GpuRenderer::new`] zwraca `Err`, a wołający
//! używa CPU. GPU nigdy nie może przewrócić sesji.

use std::collections::HashMap;
use std::ffi::{c_char, c_int, c_void, CStr};
use std::os::fd::AsRawFd;
use std::ptr;
use std::sync::Arc;

use libluna_core::{Color, Point, Rect};
use tiny_skia::Pixmap;

use crate::renderer::Renderer;
use crate::scene::{DmabufImage, DrawCommand, ImageData, Scene, TextRun};
use crate::software::SoftwareRenderer;
use crate::text::TextEngine;

// ── Stałe EGL ───────────────────────────────────────────────────────────────
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
const EGL_WIDTH: u32 = 0x3057;
const EGL_HEIGHT: u32 = 0x3056;
const EGL_LINUX_DMA_BUF_EXT: u32 = 0x3270;
const EGL_LINUX_DRM_FOURCC_EXT: u32 = 0x3271;
const EGL_DMA_BUF_PLANE0_FD_EXT: u32 = 0x3272;
const EGL_DMA_BUF_PLANE0_OFFSET_EXT: u32 = 0x3273;
const EGL_DMA_BUF_PLANE0_PITCH_EXT: u32 = 0x3274;
const EGL_DMA_BUF_PLANE1_FD_EXT: u32 = 0x3275;
const EGL_DMA_BUF_PLANE1_OFFSET_EXT: u32 = 0x3276;
const EGL_DMA_BUF_PLANE1_PITCH_EXT: u32 = 0x3277;
const EGL_DMA_BUF_PLANE2_FD_EXT: u32 = 0x3278;
const EGL_DMA_BUF_PLANE2_OFFSET_EXT: u32 = 0x3279;
const EGL_DMA_BUF_PLANE2_PITCH_EXT: u32 = 0x327A;
const EGL_DMA_BUF_PLANE3_FD_EXT: u32 = 0x3440;
const EGL_DMA_BUF_PLANE3_OFFSET_EXT: u32 = 0x3441;
const EGL_DMA_BUF_PLANE3_PITCH_EXT: u32 = 0x3442;
const EGL_DMA_BUF_PLANE0_MODIFIER_LO_EXT: u32 = 0x3443;
const EGL_DMA_BUF_PLANE0_MODIFIER_HI_EXT: u32 = 0x3444;
const EGL_NO_CONTEXT: *mut c_void = ptr::null_mut();

// ── Stałe GLES2 ─────────────────────────────────────────────────────────────
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
const GL_ONE: u32 = 1;
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
const GL_UNPACK_ALIGNMENT: u32 = 0x0CF5;
const GL_RENDERER: u32 = 0x1F01;
const GL_VERSION: u32 = 0x1F02;
const GL_VENDOR: u32 = 0x1F00;
const GL_EXTENSIONS: u32 = 0x1F03;

type Gluint = u32;
type Glint = i32;
type Glsizei = i32;
type Glboolean = u8;
type Glchar = c_char;

#[link(name = "dl")]
extern "C" {
    fn dlopen(filename: *const c_char, flags: c_int) -> *mut c_void;
    fn dlsym(handle: *mut c_void, symbol: *const c_char) -> *mut c_void;
}
const RTLD_NOW: c_int = 2;

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
type EglSwapInterval = unsafe extern "C" fn(*mut c_void, i32) -> u32;
// `eglCreateImageKHR` ma atrybuty 32-bitowe (w odróżnieniu od 1.5 `eglCreateImage`
// z `EGLAttrib` 64-bit); używamy wariantu KHR dla importu dmabuf i testów.
type EglCreateImage =
    unsafe extern "C" fn(*mut c_void, *mut c_void, u32, *mut c_void, *const i32) -> *mut c_void;
type EglDestroyImage = unsafe extern "C" fn(*mut c_void, *mut c_void) -> u32;
type EglQueryDmaBufFormats = unsafe extern "C" fn(*mut c_void, i32, *mut i32, *mut i32) -> u32;
type EglQueryDmaBufModifiers =
    unsafe extern "C" fn(*mut c_void, i32, i32, *mut u64, *mut u32, *mut i32) -> u32;
type EglExportDmaBuf =
    unsafe extern "C" fn(*mut c_void, *mut c_void, *mut i32, *mut i32, *mut i32) -> u32;
type EglExportDmaBufQuery =
    unsafe extern "C" fn(*mut c_void, *mut c_void, *mut i32, *mut i32, *mut u64) -> u32;
type GlEglImageTargetTexture2D = unsafe extern "C" fn(u32, *mut c_void);

type GlGetString = unsafe extern "C" fn(u32) -> *const u8;
type GlGenTextures = unsafe extern "C" fn(i32, *mut Gluint);
type GlDeleteTextures = unsafe extern "C" fn(i32, *const Gluint);
type GlBindTexture = unsafe extern "C" fn(u32, Gluint);
type GlTexImage2D = unsafe extern "C" fn(u32, i32, i32, i32, i32, i32, u32, u32, *const c_void);
type GlTexSubImage2D = unsafe extern "C" fn(u32, i32, i32, i32, i32, i32, u32, u32, *const c_void);
type GlTexParameteri = unsafe extern "C" fn(u32, u32, i32);
type GlActiveTexture = unsafe extern "C" fn(u32);
type GlPixelStorei = unsafe extern "C" fn(u32, i32);
type GlGenFramebuffers = unsafe extern "C" fn(i32, *mut Gluint);
type GlDeleteFramebuffers = unsafe extern "C" fn(i32, *const Gluint);
type GlBindFramebuffer = unsafe extern "C" fn(u32, Gluint);
type GlFramebufferTexture2D = unsafe extern "C" fn(u32, u32, u32, Gluint, i32);
type GlCheckFramebufferStatus = unsafe extern "C" fn(u32) -> u32;
type GlViewport = unsafe extern "C" fn(i32, i32, i32, i32);
type GlClearColor = unsafe extern "C" fn(f32, f32, f32, f32);
type GlClear = unsafe extern "C" fn(u32);
type GlEnable = unsafe extern "C" fn(u32);
type GlDisable = unsafe extern "C" fn(u32);
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
type GlUniform1i = unsafe extern "C" fn(Glint, i32);
type GlUniform1f = unsafe extern "C" fn(Glint, f32);
type GlUniform2f = unsafe extern "C" fn(Glint, f32, f32);
type GlUniform4f = unsafe extern "C" fn(Glint, f32, f32, f32, f32);
type GlVertexAttribPointer = unsafe extern "C" fn(Gluint, i32, u32, Glboolean, i32, *const c_void);
type GlEnableVertexAttribArray = unsafe extern "C" fn(Gluint);
type GlDrawArrays = unsafe extern "C" fn(u32, i32, i32);
type GlReadPixels = unsafe extern "C" fn(i32, i32, i32, i32, u32, u32, *mut c_void);

/// Informacje o kontekście GPU (diagnostyka, `gpu_probe`, log przy starcie).
#[derive(Debug, Clone)]
pub struct GpuInfo {
    pub renderer: String,
    pub vendor: String,
    pub version: String,
    pub egl_vendor: String,
    pub egl_version: String,
    /// Połączone ciągi rozszerzeń EGL + GL.
    pub extensions: String,
}

impl GpuInfo {
    /// Czy dostępny jest import buforów `dmabuf` (klienty GPU).
    pub fn has_dma_buf_import(&self) -> bool {
        has_ext(&self.extensions, "EGL_EXT_image_dma_buf_import")
    }
}

fn has_ext(all: &str, name: &str) -> bool {
    all.split_whitespace().any(|e| e == name)
}

struct Gl {
    /// Trzymane tylko po to, żeby kontekst żył przez cały czas życia renderera.
    dpy: *mut c_void,
    _ctx: *mut c_void,
    get_error: EglGetError,
    /// Import `dmabuf` (EGL 1.5 / `EGL_KHR_image_base`).
    create_image: Option<EglCreateImage>,
    destroy_image: Option<EglDestroyImage>,
    /// `glEGLImageTargetTexture2DOES` (`GL_OES_EGL_image`).
    egl_image_texture: Option<GlEglImageTargetTexture2D>,
    query_formats: Option<EglQueryDmaBufFormats>,
    query_modifiers: Option<EglQueryDmaBufModifiers>,
    /// Eksport tekstury do `dmabuf` — tylko do testów round-trip importu.
    #[allow(dead_code)]
    export_image: Option<EglExportDmaBuf>,
    #[allow(dead_code)]
    export_query: Option<EglExportDmaBufQuery>,
    gen_textures: GlGenTextures,
    delete_textures: GlDeleteTextures,
    bind_texture: GlBindTexture,
    tex_image_2d: GlTexImage2D,
    tex_sub_image_2d: GlTexSubImage2D,
    tex_parameteri: GlTexParameteri,
    active_texture: GlActiveTexture,
    pixel_storei: GlPixelStorei,
    gen_framebuffers: GlGenFramebuffers,
    delete_framebuffers: GlDeleteFramebuffers,
    bind_framebuffer: GlBindFramebuffer,
    framebuffer_texture_2d: GlFramebufferTexture2D,
    check_framebuffer_status: GlCheckFramebufferStatus,
    viewport: GlViewport,
    clear_color: GlClearColor,
    clear: GlClear,
    enable: GlEnable,
    disable: GlDisable,
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
    uniform_1i: GlUniform1i,
    uniform_1f: GlUniform1f,
    uniform_2f: GlUniform2f,
    uniform_4f: GlUniform4f,
    vertex_attrib_pointer: GlVertexAttribPointer,
    enable_vertex_attrib_array: GlEnableVertexAttribArray,
    draw_arrays: GlDrawArrays,
    read_pixels: GlReadPixels,
}

impl Gl {
    fn open() -> Result<(Self, GpuInfo), String> {
        let egl_lib = dlopen_cstr("libEGL.so.1")
            .or_else(|| dlopen_cstr("libEGL.so"))
            .ok_or("brak libEGL")?;
        let gles_lib = dlopen_cstr("libGLESv2.so.2")
            .or_else(|| dlopen_cstr("libGLESv2.so"))
            .ok_or("brak libGLESv2")?;

        unsafe {
            let get_proc: EglGetProcAddress =
                std::mem::transmute(sym(egl_lib, "eglGetProcAddress")?);
            let get_platform_display: Option<EglGetPlatformDisplay> =
                load_proc(get_proc, "eglGetPlatformDisplayEXT");
            let get_display: Option<EglGetDisplay> = load_proc(get_proc, "eglGetDisplay");

            let mut dpy = ptr::null_mut();
            if let Some(f) = get_platform_display {
                dpy = f(EGL_PLATFORM_SURFACELESS_MESA, ptr::null_mut(), ptr::null());
            }
            if dpy.is_null() {
                if let Some(f) = get_display {
                    dpy = f(ptr::null_mut());
                }
            }
            if dpy.is_null() {
                return Err("nie udało się uzyskać EGLDisplay".to_owned());
            }

            let initialize: EglInitialize = std::mem::transmute(sym(egl_lib, "eglInitialize")?);
            let (mut maj, mut min) = (0, 0);
            if initialize(dpy, &mut maj, &mut min) == 0 {
                return Err("eglInitialize nie powiodło się".to_owned());
            }
            let query: EglQueryString = std::mem::transmute(sym(egl_lib, "eglQueryString")?);
            let bind_api: EglBindApi = std::mem::transmute(sym(egl_lib, "eglBindAPI")?);
            let choose: EglChooseConfig = std::mem::transmute(sym(egl_lib, "eglChooseConfig")?);
            let create_context: EglCreateContext =
                std::mem::transmute(sym(egl_lib, "eglCreateContext")?);
            let make_current: EglMakeCurrent = std::mem::transmute(sym(egl_lib, "eglMakeCurrent")?);
            let get_error: EglGetError = std::mem::transmute(sym(egl_lib, "eglGetError")?);
            // Nested: bez vsyncu — i tak prezentujemy przez `wl_shm`/readback.
            if let Some(p) = load_proc::<EglSwapInterval>(get_proc, "eglSwapInterval") {
                p(dpy, 0);
            }

            if bind_api(EGL_OPENGL_ES_API) == 0 {
                return Err(format!("eglBindAPI: 0x{:x}", get_error()));
            }
            let mut config = ptr::null_mut();
            let mut n = 0;
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
                if choose(dpy, attribs.as_ptr(), &mut config, 1, &mut n) != 0 && n >= 1 {
                    break;
                }
            }
            if n < 1 || config.is_null() {
                return Err(format!("eglChooseConfig: 0x{:x}", get_error()));
            }
            let ctx_attribs: [i32; 3] = [EGL_CONTEXT_CLIENT_VERSION as i32, 2, EGL_NONE as i32];
            let ctx = create_context(dpy, config, ptr::null_mut(), ctx_attribs.as_ptr());
            if ctx.is_null() {
                return Err(format!("eglCreateContext: 0x{:x}", get_error()));
            }
            if make_current(dpy, ptr::null_mut(), ptr::null_mut(), ctx) == 0 {
                return Err(format!("eglMakeCurrent: 0x{:x}", get_error()));
            }

            macro_rules! glfn {
                ($name:literal) => {
                    load_proc(get_proc, $name)
                        .or_else(|| dlsym_fn(gles_lib, $name))
                        .ok_or_else(|| format!("brak funkcji {}", $name))?
                };
            }
            let get_string: GlGetString = glfn!("glGetString");
            let info = GpuInfo {
                renderer: cstr_u8(get_string(GL_RENDERER)),
                vendor: cstr_u8(get_string(GL_VENDOR)),
                version: cstr_u8(get_string(GL_VERSION)),
                egl_vendor: cstr(query(dpy, EGL_VENDOR)),
                egl_version: cstr(query(dpy, EGL_VERSION)),
                extensions: format!(
                    "{} {}",
                    cstr_u8(get_string(GL_EXTENSIONS)),
                    cstr(query(dpy, EGL_EXTENSIONS))
                ),
            };

            Ok((
                Self {
                    dpy,
                    _ctx: ctx,
                    get_error,
                    create_image: load_proc(get_proc, "eglCreateImageKHR"),
                    destroy_image: load_proc(get_proc, "eglDestroyImage")
                        .or_else(|| dlsym_fn(egl_lib, "eglDestroyImage")),
                    egl_image_texture: load_proc(get_proc, "glEGLImageTargetTexture2DOES"),
                    query_formats: load_proc(get_proc, "eglQueryDmaBufFormatsEXT"),
                    query_modifiers: load_proc(get_proc, "eglQueryDmaBufModifiersEXT"),
                    export_image: load_proc(get_proc, "eglExportDMABUFImageMESA"),
                    export_query: load_proc(get_proc, "eglExportDMABUFImageQueryMESA"),
                    gen_textures: glfn!("glGenTextures"),
                    delete_textures: glfn!("glDeleteTextures"),
                    bind_texture: glfn!("glBindTexture"),
                    tex_image_2d: glfn!("glTexImage2D"),
                    tex_sub_image_2d: glfn!("glTexSubImage2D"),
                    tex_parameteri: glfn!("glTexParameteri"),
                    active_texture: glfn!("glActiveTexture"),
                    pixel_storei: glfn!("glPixelStorei"),
                    gen_framebuffers: glfn!("glGenFramebuffers"),
                    delete_framebuffers: glfn!("glDeleteFramebuffers"),
                    bind_framebuffer: glfn!("glBindFramebuffer"),
                    framebuffer_texture_2d: glfn!("glFramebufferTexture2D"),
                    check_framebuffer_status: glfn!("glCheckFramebufferStatus"),
                    viewport: glfn!("glViewport"),
                    clear_color: glfn!("glClearColor"),
                    clear: glfn!("glClear"),
                    enable: glfn!("glEnable"),
                    disable: glfn!("glDisable"),
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
                    uniform_1i: glfn!("glUniform1i"),
                    uniform_1f: glfn!("glUniform1f"),
                    uniform_2f: glfn!("glUniform2f"),
                    uniform_4f: glfn!("glUniform4f"),
                    vertex_attrib_pointer: glfn!("glVertexAttribPointer"),
                    enable_vertex_attrib_array: glfn!("glEnableVertexAttribArray"),
                    draw_arrays: glfn!("glDrawArrays"),
                    read_pixels: glfn!("glReadPixels"),
                },
                info,
            ))
        }
    }

    fn error(&self) -> i32 {
        unsafe { (self.get_error)() }
    }
}

/// Czy GPU jest dostępne (bez tworzenia renderera). Wynik jest cache'owany.
pub fn available() -> bool {
    info().is_some()
}

/// Informacje o kontekście GPU (cache na cały proces).
pub fn info() -> Option<GpuInfo> {
    static INFO: std::sync::OnceLock<Option<GpuInfo>> = std::sync::OnceLock::new();
    INFO.get_or_init(|| Gl::open().ok().map(|(_, info)| info))
        .clone()
}

/// Czy import `dmabuf` jest dostępny (EGL + `GL_OES_EGL_image` + formaty).
pub fn dmabuf_supported() -> bool {
    !dmabuf_formats().is_empty()
}

/// Formaty DRM i ich modyfikatory obsługiwane przez EGL (cache na proces).
///
/// Lista jest pusta, gdy brak importu `dmabuf` — wtedy Selene nie ogłasza
/// `zwp_linux_dmabuf_v1` i klienci zostają przy `wl_shm`.
pub fn dmabuf_formats() -> Vec<(u32, Vec<u64>)> {
    static CACHE: std::sync::OnceLock<Vec<(u32, Vec<u64>)>> = std::sync::OnceLock::new();
    CACHE
        .get_or_init(|| {
            let Ok((gl, info)) = Gl::open() else {
                return Vec::new();
            };
            if !info.has_dma_buf_import() {
                return Vec::new();
            }
            let (Some(query_formats), Some(query_modifiers)) =
                (gl.query_formats, gl.query_modifiers)
            else {
                return Vec::new();
            };
            let dpy = gl.dpy;
            let mut count = 0i32;
            if unsafe { query_formats(dpy, 0, ptr::null_mut(), &mut count) } == 0 || count <= 0 {
                return Vec::new();
            }
            let mut formats = vec![0i32; count as usize];
            if unsafe { query_formats(dpy, count, formats.as_mut_ptr(), &mut count) } == 0 {
                return Vec::new();
            }
            formats.truncate(count.max(0) as usize);
            let mut out = Vec::with_capacity(formats.len());
            for format in formats {
                let format = format as u32;
                let mut n = 0i32;
                let mut mods = Vec::new();
                if unsafe {
                    query_modifiers(
                        dpy,
                        format as i32,
                        0,
                        ptr::null_mut(),
                        ptr::null_mut(),
                        &mut n,
                    )
                } != 0
                    && n > 0
                {
                    let mut values = vec![0u64; n as usize];
                    let mut external = vec![0u32; n as usize];
                    if unsafe {
                        query_modifiers(
                            dpy,
                            format as i32,
                            n,
                            values.as_mut_ptr(),
                            external.as_mut_ptr(),
                            &mut n,
                        )
                    } != 0
                    {
                        values.truncate(n.max(0) as usize);
                        external.truncate(n.max(0) as usize);
                        mods = values
                            .into_iter()
                            .zip(external)
                            .filter_map(|(m, ext)| (ext == 0).then_some(m))
                            .collect();
                    }
                }
                // Brak modyfikatorów = format obsługiwany tylko z niejawnym układem.
                if mods.is_empty() {
                    mods.push(u64::MAX);
                }
                out.push((format, mods));
            }
            out
        })
        .clone()
}

fn dlopen_cstr(name: &str) -> Option<*mut c_void> {
    let c = std::ffi::CString::new(name).ok()?;
    let h = unsafe { dlopen(c.as_ptr(), RTLD_NOW) };
    (!h.is_null()).then_some(h)
}

unsafe fn sym(handle: *mut c_void, name: &str) -> Result<*mut c_void, String> {
    let c = std::ffi::CString::new(name).map_err(|_| "zła nazwa symbolu".to_owned())?;
    let p = dlsym(handle, c.as_ptr());
    if p.is_null() {
        Err(format!("brak symbolu {name}"))
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
    cstr(p as *const c_char)
}

/// Shader teksturowany (obrazy + warstwy CPU) z clipem prostokątnym/zaokrąglonym.
struct TexProgram {
    id: Gluint,
    pos: Gluint,
    screen: Glint,
    rect: Glint,
    clip: Glint,
    round: Glint,
    round_r: Glint,
    tex: Glint,
}

/// Shader kształtów (prostokąty, zaokrąglenia, obwódki, okręgi) przez SDF.
struct ShapeProgram {
    id: Gluint,
    pos: Gluint,
    screen: Glint,
    rect: Glint,
    color: Glint,
    mode: Glint,
    radius: Glint,
    width: Glint,
    clip: Glint,
    round: Glint,
    round_r: Glint,
}

const VS: &str = "\
attribute vec2 pos;
uniform vec2 screen;
uniform vec4 rect;
varying vec2 local;
void main() {
    vec2 p = rect.xy + pos * rect.zw;
    local = p;
    // Y w dół: wiersz 0 framebuffera = górna krawędź ekranu (readback bez odwracania).
    gl_Position = vec4(p.x / screen.x * 2.0 - 1.0, p.y / screen.y * 2.0 - 1.0, 0.0, 1.0);
}";

const FS_TEX: &str = "\
precision mediump float;
varying vec2 local;
uniform sampler2D tex;
uniform vec4 rect;
uniform vec4 clip;
// Clip zaokrąglony: xy = początek, zw = rozmiar, round_r = promień.
uniform vec4 round;
uniform float round_r;
float sd_round(vec2 p, vec2 size, float r) {
    r = min(r, min(size.x, size.y) * 0.5);
    vec2 q = abs(p - size * 0.5) - (size * 0.5 - vec2(r));
    return length(max(q, 0.0)) + min(max(q.x, q.y), 0.0) - r;
}
void main() {
    if (local.x < clip.x || local.y < clip.y ||
        local.x > clip.x + clip.z || local.y > clip.y + clip.w) discard;
    vec2 uv = (local - rect.xy) / rect.zw;
    float a = 1.0;
    if (round.z > 0.0 && round.w > 0.0 && round_r > 0.0) {
        float d = sd_round(local - round.xy, round.zw, round_r);
        a = clamp(0.5 - d, 0.0, 1.0);
        if (a <= 0.0) discard;
    }
    gl_FragColor = texture2D(tex, uv) * a;
}";

const FS_SHAPE: &str = "\
precision mediump float;
varying vec2 local;
uniform vec4 rect;
uniform vec4 color;
uniform vec4 clip;
uniform vec4 round;
uniform float round_r;
uniform float radius;
uniform int mode;
uniform float border;
float sd_round(vec2 p, vec2 size, float r) {
    r = min(r, min(size.x, size.y) * 0.5);
    vec2 q = abs(p - size * 0.5) - (size * 0.5 - vec2(r));
    return length(max(q, 0.0)) + min(max(q.x, q.y), 0.0) - r;
}
void main() {
    if (local.x < clip.x || local.y < clip.y ||
        local.x > clip.x + clip.z || local.y > clip.y + clip.w) discard;
    float d;
    if (mode == 0) {
        // Zwykły prostokąt: pełne krycie w całym quadzie (bez SDF).
        d = -1.0;
    } else if (mode == 1) {
        d = sd_round(local - rect.xy, rect.zw, radius);
    } else if (mode == 2) {
        d = abs(sd_round(local - rect.xy, rect.zw, radius) + border * 0.5) - border * 0.5;
    } else {
        // Okrąg: rect.xy = środek, radius = promień (mode 3/4).
        float dd = length(local - rect.xy) - radius;
        if (mode == 3) {
            d = dd;
        } else {
            d = abs(dd + border * 0.5) - border * 0.5;
        }
    }
    float a = clamp(0.5 - d, 0.0, 1.0);
    if (round.z > 0.0 && round.w > 0.0 && round_r > 0.0) {
        float rd = sd_round(local - round.xy, round.zw, round_r);
        a *= clamp(0.5 - rd, 0.0, 1.0);
    }
    gl_FragColor = color * a;
}";

/// Shader glifów: próbkuje krycie z atlasu i mnoży przez kolor tekstu.
const FS_GLYPH: &str = "\
precision mediump float;
varying vec2 local;
uniform sampler2D tex;
uniform vec4 rect;
uniform vec4 uv;
uniform vec4 color;
uniform vec4 clip;
uniform vec4 round;
uniform float round_r;
float sd_round(vec2 p, vec2 size, float r) {
    r = min(r, min(size.x, size.y) * 0.5);
    vec2 q = abs(p - size * 0.5) - (size * 0.5 - vec2(r));
    return length(max(q, 0.0)) + min(max(q.x, q.y), 0.0) - r;
}
void main() {
    if (local.x < clip.x || local.y < clip.y ||
        local.x > clip.x + clip.z || local.y > clip.y + clip.w) discard;
    vec2 t = (local - rect.xy) / rect.zw;
    float a = texture2D(tex, uv.xy + t * uv.zw).a;
    if (round.z > 0.0 && round.w > 0.0 && round_r > 0.0) {
        float d = sd_round(local - round.xy, round.zw, round_r);
        a *= clamp(0.5 - d, 0.0, 1.0);
    }
    gl_FragColor = color * a;
}";

struct Tex {
    id: Gluint,
    w: u32,
    h: u32,
    /// Ostatnia klatka, w której tekstura była użyta (eviction).
    used: u64,
    /// `EGLImage` do zniszczenia razem z teksturą (`null` dla tekstur z CPU).
    egl_image: *mut c_void,
}

/// Shader glifów z atlasu (tekst bez warstw CPU).
struct GlyphProgram {
    id: Gluint,
    pos: Gluint,
    screen: Glint,
    rect: Glint,
    uv: Glint,
    color: Glint,
    clip: Glint,
    round: Glint,
    round_r: Glint,
    tex: Glint,
}

/// Backend GPU: EGL surfaceless + GLES2, wyjście przez readback.
pub struct GpuRenderer {
    gl: Gl,
    width: u32,
    height: u32,
    fbo: Gluint,
    out_tex: Gluint,
    vbo: Gluint,
    tex_prog: TexProgram,
    shape_prog: ShapeProgram,
    glyph_prog: GlyphProgram,
    text: TextEngine,
    /// Atlas glifów (krycie w kanale alfa) i cache glifów per (znak, rozmiar).
    atlas: Atlas,
    glyphs: HashMap<GlyphKey, Glyph>,
    /// Bufor do rasteryzacji warstw CPU (wektory). Trzymamy go, bo
    /// `SoftwareRenderer::new` ładuje font z dysku — tworzenie go per warstwa
    /// kosztowało setki ms na klatkę.
    layer: SoftwareRenderer,
    textures: HashMap<usize, Tex>,
    frame: u64,
    pixels: Vec<u8>,
    /// Bufor pomocniczy na odczyt fragmentu klatki (bez odstępu wierszy).
    scratch: Vec<u8>,
    /// Damage podany przez `draw_damage` (piksele logiczne), czeka na `finish`.
    next_damage: Option<Rect>,
    /// Region faktycznie odczytany z GPU w ostatniej klatce (piksele fizyczne).
    last_readback: Option<Rect>,
    /// Czas `glReadPixels` z ostatniej klatki (diagnostyka).
    readback_ms: f32,
}

impl GpuRenderer {
    /// Tworzy renderer (błąd = brak EGL/kontekstu — wołający użyj CPU).
    pub fn new(width: u32, height: u32) -> Result<Self, String> {
        Self::with_text(width, height, TextEngine::system(), false)
    }

    /// Jak [`GpuRenderer::new`], ale z fontem o stałej szerokości.
    pub fn new_mono(width: u32, height: u32) -> Result<Self, String> {
        Self::with_text(width, height, TextEngine::mono(), true)
    }

    fn with_text(width: u32, height: u32, text: TextEngine, mono: bool) -> Result<Self, String> {
        let (gl, _info) = Gl::open()?;
        unsafe {
            (gl.pixel_storei)(GL_UNPACK_ALIGNMENT, 1);
            (gl.disable)(0x0B44); // GL_CULL_FACE
        }
        let tex_prog = TexProgram::new(&gl)?;
        let shape_prog = ShapeProgram::new(&gl)?;
        let glyph_prog = GlyphProgram::new(&gl)?;
        let vbo = unsafe {
            let mut vbo = 0;
            (gl.gen_buffers)(1, &mut vbo);
            (gl.bind_buffer)(GL_ARRAY_BUFFER, vbo);
            let verts: [f32; 8] = [0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 1.0, 1.0];
            (gl.buffer_data)(
                GL_ARRAY_BUFFER,
                std::mem::size_of_val(&verts) as isize,
                verts.as_ptr() as *const c_void,
                GL_STATIC_DRAW,
            );
            vbo
        };
        let mut renderer = Self {
            gl,
            width: width.max(1),
            height: height.max(1),
            fbo: 0,
            out_tex: 0,
            vbo,
            tex_prog,
            shape_prog,
            glyph_prog,
            text,
            layer: if mono {
                SoftwareRenderer::new_mono(1, 1)
            } else {
                SoftwareRenderer::new(1, 1)
            },
            textures: HashMap::new(),
            atlas: Atlas::new(512),
            glyphs: HashMap::new(),
            frame: 0,
            pixels: Vec::new(),
            scratch: Vec::new(),
            next_damage: None,
            last_readback: None,
            readback_ms: 0.0,
        };
        renderer.create_targets();
        Ok(renderer)
    }

    fn create_targets(&mut self) {
        let (w, h) = (self.width, self.height);
        unsafe {
            if self.out_tex != 0 {
                (self.gl.delete_textures)(1, &self.out_tex);
            }
            if self.fbo != 0 {
                (self.gl.delete_framebuffers)(1, &self.fbo);
            }
            let mut tex = 0;
            (self.gl.gen_textures)(1, &mut tex);
            (self.gl.bind_texture)(GL_TEXTURE_2D, tex);
            (self.gl.tex_image_2d)(
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
            (self.gl.tex_parameteri)(GL_TEXTURE_2D, GL_TEXTURE_MIN_FILTER, GL_LINEAR as i32);
            (self.gl.tex_parameteri)(GL_TEXTURE_2D, GL_TEXTURE_MAG_FILTER, GL_LINEAR as i32);
            let mut fbo = 0;
            (self.gl.gen_framebuffers)(1, &mut fbo);
            (self.gl.bind_framebuffer)(GL_FRAMEBUFFER, fbo);
            (self.gl.framebuffer_texture_2d)(
                GL_FRAMEBUFFER,
                GL_COLOR_ATTACHMENT0,
                GL_TEXTURE_2D,
                tex,
                0,
            );
            self.out_tex = tex;
            self.fbo = fbo;
        }
        self.pixels = vec![0; (w * h * 4) as usize];
    }

    fn target_ok(&self) -> bool {
        unsafe { (self.gl.check_framebuffer_status)(GL_FRAMEBUFFER) == GL_FRAMEBUFFER_COMPLETE }
    }

    fn begin(&mut self, background: Color) {
        unsafe {
            (self.gl.bind_framebuffer)(GL_FRAMEBUFFER, self.fbo);
            (self.gl.viewport)(0, 0, self.width as i32, self.height as i32);
            (self.gl.disable)(0x0C11); // GL_SCISSOR_TEST
            let [r, g, b, a] = background.to_rgba8();
            let a = f32::from(a) / 255.0;
            (self.gl.clear_color)(
                f32::from(r) / 255.0 * a,
                f32::from(g) / 255.0 * a,
                f32::from(b) / 255.0 * a,
                a,
            );
            (self.gl.clear)(GL_COLOR_BUFFER_BIT);
            (self.gl.enable)(GL_BLEND);
            (self.gl.blend_func)(GL_ONE, GL_ONE_MINUS_SRC_ALPHA);
        }
    }

    /// Rysuje teksturę w prostokącie (współrzędne fizyczne).
    #[allow(clippy::too_many_arguments)]
    fn draw_texture(&self, tex: Gluint, rect: Rect, clip: Rect, round: Option<(Rect, f64)>) {
        let p = &self.tex_prog;
        unsafe {
            (self.gl.use_program)(p.id);
            (self.gl.bind_buffer)(GL_ARRAY_BUFFER, self.vbo);
            (self.gl.enable_vertex_attrib_array)(p.pos);
            (self.gl.vertex_attrib_pointer)(p.pos, 2, GL_FLOAT, 0, 0, ptr::null());
            (self.gl.active_texture)(GL_TEXTURE0);
            (self.gl.bind_texture)(GL_TEXTURE_2D, tex);
            (self.gl.uniform_1i)(p.tex, 0);
            (self.gl.uniform_2f)(p.screen, self.width as f32, self.height as f32);
            (self.gl.uniform_4f)(
                p.rect,
                rect.x() as f32,
                rect.y() as f32,
                rect.width() as f32,
                rect.height() as f32,
            );
            (self.gl.uniform_4f)(
                p.clip,
                clip.x() as f32,
                clip.y() as f32,
                clip.width() as f32,
                clip.height() as f32,
            );
            match round {
                Some((r, radius)) => {
                    (self.gl.uniform_4f)(
                        p.round,
                        r.x() as f32,
                        r.y() as f32,
                        r.width() as f32,
                        r.height() as f32,
                    );
                    (self.gl.uniform_1f)(p.round_r, radius as f32);
                }
                None => {
                    (self.gl.uniform_4f)(p.round, 0.0, 0.0, 0.0, 0.0);
                    (self.gl.uniform_1f)(p.round_r, 0.0);
                }
            }
            (self.gl.draw_arrays)(GL_TRIANGLE_STRIP, 0, 4);
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_shape(
        &self,
        rect: Rect,
        color: Color,
        mode: i32,
        radius: f64,
        border: f64,
        clip: Rect,
        round: Option<(Rect, f64)>,
    ) {
        let p = &self.shape_prog;
        let [r, g, b, a] = color.to_rgba8();
        let af = f32::from(a) / 255.0;
        unsafe {
            (self.gl.use_program)(p.id);
            (self.gl.bind_buffer)(GL_ARRAY_BUFFER, self.vbo);
            (self.gl.enable_vertex_attrib_array)(p.pos);
            (self.gl.vertex_attrib_pointer)(p.pos, 2, GL_FLOAT, 0, 0, ptr::null());
            (self.gl.uniform_2f)(p.screen, self.width as f32, self.height as f32);
            (self.gl.uniform_4f)(
                p.rect,
                rect.x() as f32,
                rect.y() as f32,
                rect.width() as f32,
                rect.height() as f32,
            );
            // Kolor premnożony — blending `ONE, ONE_MINUS_SRC_ALPHA`.
            (self.gl.uniform_4f)(
                p.color,
                f32::from(r) / 255.0 * af,
                f32::from(g) / 255.0 * af,
                f32::from(b) / 255.0 * af,
                af,
            );
            (self.gl.uniform_1i)(p.mode, mode);
            (self.gl.uniform_1f)(p.radius, radius as f32);
            (self.gl.uniform_1f)(p.width, border as f32);
            (self.gl.uniform_4f)(
                p.clip,
                clip.x() as f32,
                clip.y() as f32,
                clip.width() as f32,
                clip.height() as f32,
            );
            match round {
                Some((rr, radius)) => {
                    (self.gl.uniform_4f)(
                        p.round,
                        rr.x() as f32,
                        rr.y() as f32,
                        rr.width() as f32,
                        rr.height() as f32,
                    );
                    (self.gl.uniform_1f)(p.round_r, radius as f32);
                }
                None => {
                    (self.gl.uniform_4f)(p.round, 0.0, 0.0, 0.0, 0.0);
                    (self.gl.uniform_1f)(p.round_r, 0.0);
                }
            }
            (self.gl.draw_arrays)(GL_TRIANGLE_STRIP, 0, 4);
        }
    }

    fn image_texture(&mut self, data: &Arc<ImageData>) -> Gluint {
        let key = Arc::as_ptr(data) as usize;
        let frame = self.frame;
        if let Some(t) = self.textures.get_mut(&key) {
            if t.w == data.width
                && t.h == data.height
                && data.rgba.len() >= (t.w * t.h * 4) as usize
            {
                t.used = frame;
                return t.id;
            }
        }
        if data.width == 0
            || data.height == 0
            || data.rgba.len() < (data.width * data.height * 4) as usize
        {
            return 0;
        }
        let id = upload_texture(&self.gl, data.width, data.height, &data.rgba);
        self.textures.insert(
            key,
            Tex {
                id,
                w: data.width,
                h: data.height,
                used: frame,
                egl_image: ptr::null_mut(),
            },
        );
        id
    }

    /// Importuje `dmabuf` klienta jako teksturę (leniwie, z cache per `Arc`).
    fn dmabuf_texture(&mut self, data: &Arc<DmabufImage>) -> Gluint {
        let key = Arc::as_ptr(data) as usize;
        let frame = self.frame;
        if let Some(t) = self.textures.get_mut(&key) {
            t.used = frame;
            return t.id;
        }
        let (Some(create), Some(target)) = (self.gl.create_image, self.gl.egl_image_texture) else {
            return 0;
        };
        let mut attribs: Vec<i32> = vec![
            EGL_WIDTH as i32,
            data.width as i32,
            EGL_HEIGHT as i32,
            data.height as i32,
            EGL_LINUX_DRM_FOURCC_EXT as i32,
            data.format as i32,
        ];
        // Kolejne plany mają stałe offsety w enumie EGL (0x3272, 0x3275, ...).
        const FD: [u32; 4] = [
            EGL_DMA_BUF_PLANE0_FD_EXT,
            EGL_DMA_BUF_PLANE1_FD_EXT,
            EGL_DMA_BUF_PLANE2_FD_EXT,
            EGL_DMA_BUF_PLANE3_FD_EXT,
        ];
        const OFF: [u32; 4] = [
            EGL_DMA_BUF_PLANE0_OFFSET_EXT,
            EGL_DMA_BUF_PLANE1_OFFSET_EXT,
            EGL_DMA_BUF_PLANE2_OFFSET_EXT,
            EGL_DMA_BUF_PLANE3_OFFSET_EXT,
        ];
        const PITCH: [u32; 4] = [
            EGL_DMA_BUF_PLANE0_PITCH_EXT,
            EGL_DMA_BUF_PLANE1_PITCH_EXT,
            EGL_DMA_BUF_PLANE2_PITCH_EXT,
            EGL_DMA_BUF_PLANE3_PITCH_EXT,
        ];
        for (i, plane) in data.planes.iter().take(4).enumerate() {
            attribs.extend_from_slice(&[
                FD[i] as i32,
                plane.fd.as_raw_fd(),
                OFF[i] as i32,
                plane.offset as i32,
                PITCH[i] as i32,
                plane.stride as i32,
            ]);
        }
        // Modyfikator tylko dla planu 0 (protokół wymaga wspólnego modyfikatora).
        if data.modifier != u64::MAX {
            attribs.extend_from_slice(&[
                EGL_DMA_BUF_PLANE0_MODIFIER_LO_EXT as i32,
                (data.modifier & 0xffff_ffff) as i32,
                EGL_DMA_BUF_PLANE0_MODIFIER_HI_EXT as i32,
                (data.modifier >> 32) as i32,
            ]);
        }
        attribs.push(EGL_NONE as i32);
        let egl_image = unsafe {
            create(
                self.gl.dpy,
                EGL_NO_CONTEXT,
                EGL_LINUX_DMA_BUF_EXT,
                ptr::null_mut(),
                attribs.as_ptr(),
            )
        };
        if egl_image.is_null() {
            if std::env::var("LUNA_GPU_STATS").is_ok() {
                eprintln!(
                    "libluna-render: import dmabuf nieudany (format 0x{:08x}, modifier 0x{:016x})",
                    data.format, data.modifier
                );
            }
            return 0;
        }
        let id = unsafe {
            let mut tex = 0;
            (self.gl.gen_textures)(1, &mut tex);
            (self.gl.bind_texture)(GL_TEXTURE_2D, tex);
            target(GL_TEXTURE_2D, egl_image);
            (self.gl.tex_parameteri)(GL_TEXTURE_2D, GL_TEXTURE_MIN_FILTER, GL_LINEAR as i32);
            (self.gl.tex_parameteri)(GL_TEXTURE_2D, GL_TEXTURE_MAG_FILTER, GL_LINEAR as i32);
            (self.gl.tex_parameteri)(GL_TEXTURE_2D, GL_TEXTURE_WRAP_S, GL_CLAMP_TO_EDGE as i32);
            (self.gl.tex_parameteri)(GL_TEXTURE_2D, GL_TEXTURE_WRAP_T, GL_CLAMP_TO_EDGE as i32);
            tex
        };
        self.textures.insert(
            key,
            Tex {
                id,
                w: data.width,
                h: data.height,
                used: frame,
                egl_image,
            },
        );
        if std::env::var("LUNA_GPU_STATS").is_ok() {
            eprintln!(
                "libluna-render: dmabuf {}x{} format=0x{:08x} → tekstura {id}",
                data.width, data.height, data.format
            );
        }
        id
    }

    fn evict_textures(&mut self) {
        let frame = self.frame;
        let mut dead: Vec<(usize, Gluint, *mut c_void)> = Vec::new();
        for (key, t) in &self.textures {
            // 120 klatek bez użycia = okno zamknięte/przewinięte.
            if frame.saturating_sub(t.used) > 120 {
                dead.push((*key, t.id, t.egl_image));
            }
        }
        for (key, id, egl_image) in dead {
            unsafe {
                (self.gl.delete_textures)(1, &id);
                if !egl_image.is_null() {
                    if let Some(destroy) = self.gl.destroy_image {
                        destroy(self.gl.dpy, egl_image);
                    }
                }
            }
            self.textures.remove(&key);
        }
    }

    /// Rasteryzuje tekst/wektory przez CPU do małej warstwy i rysuje ją jako quad.
    #[allow(clippy::too_many_arguments)]
    /// Rysuje tekst glifami z atlasu (pozycje w pikselach fizycznych).
    fn draw_text(&mut self, run: &TextRun, k: f64, clip_stack: &[(Rect, f64)]) {
        if run.text.is_empty() {
            return;
        }
        let px = run.size * k;
        if px < 1.0 {
            return;
        }
        let screen = Rect::from_xywh(0.0, 0.0, self.width as f64, self.height as f64);
        let (clip, round) = clip_for(clip_stack, screen);
        let [cr, cg, cb, ca] = run.color.to_rgba8();
        let af = f32::from(ca) / 255.0;
        let color = (
            f32::from(cr) / 255.0 * af,
            f32::from(cg) / 255.0 * af,
            f32::from(cb) / 255.0 * af,
            af,
        );
        let mut pen = run.origin.x * k;
        let baseline = run.origin.y * k;
        for ch in run.text.chars() {
            let Some(g) = self.glyph(ch, px as f32, run.weight) else {
                continue;
            };
            if g.tw > 0 && g.th > 0 {
                let x = pen + f64::from(g.xmin);
                let y = baseline - f64::from(g.th as i32 + g.ymin);
                let rect = Rect::from_xywh(x, y, f64::from(g.tw), f64::from(g.th));
                self.draw_glyph(g, rect, color, clip, round);
            }
            pen += f64::from(g.advance);
        }
    }

    /// Zwraca glif z cache, w razie potrzeby rasteryzując go do atlasu.
    /// `None` = brak fontu (tryb `TextEngine::none`).
    fn glyph(&mut self, ch: char, px: f32, weight: u16) -> Option<Glyph> {
        let bold = weight >= libluna_core::style::weight::BOLD;
        let size_key = (px.max(1.0) * 2.0).round() as u32;
        let key = (ch as u32, size_key, bold);
        if let Some(g) = self.glyphs.get(&key) {
            return Some(*g);
        }
        let raster = self.text.glyph(ch, px, weight)?;
        let (tw, th) = (raster.width as u32, raster.height as u32);
        let mut glyph = Glyph {
            u: 0.0,
            v: 0.0,
            tw: 0,
            th: 0,
            xmin: raster.xmin,
            ymin: raster.ymin,
            advance: raster.advance,
        };
        // Glif bez bitmapy (spacja) zapamiętujemy od razu — tylko `advance`.
        if tw == 0 || th == 0 {
            self.glyphs.insert(key, glyph);
            return Some(glyph);
        }
        if tw + 1 > self.atlas.size || th + 1 > self.atlas.size {
            return None;
        }
        if self.atlas.x + tw + 1 > self.atlas.size {
            self.atlas.x = 0;
            self.atlas.y += self.atlas.row_h + 1;
            self.atlas.row_h = 0;
        }
        if self.atlas.y + th + 1 > self.atlas.size {
            // Atlas pełny — czyścimy i pakujemy od nowa; glify wrócą leniwie.
            self.atlas.reset();
            self.glyphs.clear();
        }
        let (ax, ay) = (self.atlas.x, self.atlas.y);
        let size = self.atlas.size;
        // Tymczasowo pakujemy krycie jako RGBA (RGB=0, A=krycie), bo
        // `glTexSubImage2D` w GLES2 czyta wiersze bez odstępu.
        let mut tmp = vec![0u8; (tw * th * 4) as usize];
        for i in 0..(tw * th) as usize {
            tmp[i * 4 + 3] = raster.coverage[i];
        }
        self.atlas_ensure();
        unsafe {
            (self.gl.bind_texture)(GL_TEXTURE_2D, self.atlas.tex);
            (self.gl.tex_sub_image_2d)(
                GL_TEXTURE_2D,
                0,
                ax as i32,
                ay as i32,
                tw as i32,
                th as i32,
                GL_RGBA,
                GL_UNSIGNED_BYTE,
                tmp.as_ptr() as *const c_void,
            );
        }
        self.atlas.x += tw + 1;
        self.atlas.row_h = self.atlas.row_h.max(th);
        glyph.u = ax as f32 / size as f32;
        glyph.v = ay as f32 / size as f32;
        glyph.tw = tw;
        glyph.th = th;
        self.glyphs.insert(key, glyph);
        Some(glyph)
    }

    /// Tworzy teksturę atlasu przy pierwszym glifie.
    fn atlas_ensure(&mut self) {
        if self.atlas.tex != 0 {
            return;
        }
        let size = self.atlas.size;
        unsafe {
            let mut tex = 0;
            (self.gl.gen_textures)(1, &mut tex);
            (self.gl.bind_texture)(GL_TEXTURE_2D, tex);
            (self.gl.tex_image_2d)(
                GL_TEXTURE_2D,
                0,
                GL_RGBA as i32,
                size as i32,
                size as i32,
                0,
                GL_RGBA,
                GL_UNSIGNED_BYTE,
                std::ptr::null(),
            );
            (self.gl.tex_parameteri)(GL_TEXTURE_2D, GL_TEXTURE_MIN_FILTER, GL_LINEAR as i32);
            (self.gl.tex_parameteri)(GL_TEXTURE_2D, GL_TEXTURE_MAG_FILTER, GL_LINEAR as i32);
            (self.gl.tex_parameteri)(GL_TEXTURE_2D, GL_TEXTURE_WRAP_S, GL_CLAMP_TO_EDGE as i32);
            (self.gl.tex_parameteri)(GL_TEXTURE_2D, GL_TEXTURE_WRAP_T, GL_CLAMP_TO_EDGE as i32);
            self.atlas.tex = tex;
        }
    }

    /// Rysuje pojedynczy glif (kolor premnożony, jak w `draw_shape`).
    fn draw_glyph(
        &self,
        g: Glyph,
        rect: Rect,
        color: (f32, f32, f32, f32),
        clip: Rect,
        round: Option<(Rect, f64)>,
    ) {
        let p = &self.glyph_prog;
        let size = self.atlas.size as f32;
        unsafe {
            (self.gl.use_program)(p.id);
            (self.gl.bind_buffer)(GL_ARRAY_BUFFER, self.vbo);
            (self.gl.enable_vertex_attrib_array)(p.pos);
            (self.gl.vertex_attrib_pointer)(p.pos, 2, GL_FLOAT, 0, 0, std::ptr::null());
            (self.gl.active_texture)(GL_TEXTURE0);
            (self.gl.bind_texture)(GL_TEXTURE_2D, self.atlas.tex);
            (self.gl.uniform_1i)(p.tex, 0);
            (self.gl.uniform_2f)(p.screen, self.width as f32, self.height as f32);
            (self.gl.uniform_4f)(
                p.rect,
                rect.x() as f32,
                rect.y() as f32,
                rect.width() as f32,
                rect.height() as f32,
            );
            (self.gl.uniform_4f)(p.uv, g.u, g.v, g.tw as f32 / size, g.th as f32 / size);
            (self.gl.uniform_4f)(p.color, color.0, color.1, color.2, color.3);
            (self.gl.uniform_4f)(
                p.clip,
                clip.x() as f32,
                clip.y() as f32,
                clip.width() as f32,
                clip.height() as f32,
            );
            match round {
                Some((r, radius)) => {
                    (self.gl.uniform_4f)(
                        p.round,
                        r.x() as f32,
                        r.y() as f32,
                        r.width() as f32,
                        r.height() as f32,
                    );
                    (self.gl.uniform_1f)(p.round_r, radius as f32);
                }
                None => {
                    (self.gl.uniform_4f)(p.round, 0.0, 0.0, 0.0, 0.0);
                    (self.gl.uniform_1f)(p.round_r, 0.0);
                }
            }
            (self.gl.draw_arrays)(GL_TRIANGLE_STRIP, 0, 4);
        }
    }

    fn flush_cpu_layer(
        &mut self,
        commands: &[DrawCommand],
        bounds: Rect,
        scale: f64,
        clip: Rect,
        round: Option<(Rect, f64)>,
    ) {
        if commands.is_empty() || bounds.width() <= 0.0 || bounds.height() <= 0.0 {
            return;
        }
        let k = scale.max(0.01);
        let px0 = (bounds.x() * k).floor();
        let py0 = (bounds.y() * k).floor();
        let px1 = ((bounds.x() + bounds.width()) * k).ceil();
        let py1 = ((bounds.y() + bounds.height()) * k).ceil();
        // Ogranicz warstwę do ekranu — reszta i tak zostałaby obcięta.
        let px0 = px0.max(0.0);
        let py0 = py0.max(0.0);
        let px1 = px1.min(self.width as f64);
        let py1 = py1.min(self.height as f64);
        let bw = (px1 - px0).round() as u32;
        let bh = (py1 - py0).round() as u32;
        if bw == 0 || bh == 0 {
            return;
        }
        let dx = px0 / k;
        let dy = py0 / k;
        let mut layer = Scene::new();
        for cmd in commands {
            layer.push(translate(cmd, dx, dy));
        }
        self.layer.resize(bw, bh);
        self.layer.draw(&layer, k, Color::rgba(0.0, 0.0, 0.0, 0.0));
        let tex = upload_texture(&self.gl, bw, bh, self.layer.data());
        let rect = Rect::from_xywh(px0, py0, f64::from(bw), f64::from(bh));
        self.draw_texture(tex, rect, clip, round);
        unsafe { (self.gl.delete_textures)(1, &tex) };
    }

    /// Kończy klatkę: odczyt z GPU do `data()`. Gdy znany jest damage, czyta tylko
    /// jego prostokąt — reszta `pixels` zostaje z poprzedniej klatki.
    fn finish(&mut self, damage: Option<Rect>, k: f64) {
        let t = std::time::Instant::now();
        let full = Rect::from_xywh(0.0, 0.0, self.width as f64, self.height as f64);
        // `None` = brak wiedzy o zmianach → cała klatka.
        let region = match damage {
            None => Some(full),
            Some(d) => phys_rect(&d, k).intersection(&full),
        };
        let Some(region) = region else {
            // Zmiana poza powierzchnią — nic nie trzeba odczytywać.
            self.last_readback = Some(Rect::ZERO);
            self.readback_ms = 0.0;
            return;
        };
        let x0 = region.x().floor().max(0.0) as i32;
        let y0 = region.y().floor().max(0.0) as i32;
        let x1 = region.right().ceil().min(self.width as f64) as i32;
        let y1 = region.bottom().ceil().min(self.height as f64) as i32;
        let (w, h) = ((x1 - x0).max(0), (y1 - y0).max(0));
        if w == 0 || h == 0 {
            self.last_readback = Some(Rect::ZERO);
            self.readback_ms = 0.0;
            return;
        }
        if w == self.width as i32 && h == self.height as i32 {
            unsafe {
                (self.gl.bind_framebuffer)(GL_FRAMEBUFFER, self.fbo);
                (self.gl.read_pixels)(
                    0,
                    0,
                    w,
                    h,
                    GL_RGBA,
                    GL_UNSIGNED_BYTE,
                    self.pixels.as_mut_ptr() as *mut c_void,
                );
            }
            self.last_readback = Some(full);
        } else {
            let (wu, hu) = (w as usize, h as usize);
            self.scratch.clear();
            self.scratch.resize(wu * hu * 4, 0);
            unsafe {
                (self.gl.bind_framebuffer)(GL_FRAMEBUFFER, self.fbo);
                (self.gl.read_pixels)(
                    x0,
                    y0,
                    w,
                    h,
                    GL_RGBA,
                    GL_UNSIGNED_BYTE,
                    self.scratch.as_mut_ptr() as *mut c_void,
                );
            }
            let stride = self.width as usize * 4;
            for row in 0..hu {
                let dst = (y0 as usize + row) * stride + x0 as usize * 4;
                let src = row * wu * 4;
                self.pixels[dst..dst + wu * 4].copy_from_slice(&self.scratch[src..src + wu * 4]);
            }
            self.last_readback = Some(Rect::from_xywh(
                f64::from(x0),
                f64::from(y0),
                f64::from(w),
                f64::from(h),
            ));
        }
        self.readback_ms = t.elapsed().as_secs_f32() * 1000.0;
    }

    /// Czas `glReadPixels` z ostatniej klatki (ms).
    pub fn readback_ms(&self) -> f32 {
        self.readback_ms
    }
}

impl Renderer for GpuRenderer {
    fn name(&self) -> &'static str {
        "gpu"
    }

    fn draw_damage(&mut self, scene: &Scene, scale: f64, background: Color, damage: Option<Rect>) {
        self.next_damage = damage;
        self.draw(scene, scale, background);
    }

    fn last_readback(&self) -> Option<Rect> {
        self.last_readback
    }

    fn resize(&mut self, width: u32, height: u32) {
        let (w, h) = (width.max(1), height.max(1));
        if (w, h) != (self.width, self.height) {
            self.width = w;
            self.height = h;
            self.create_targets();
        }
    }

    fn width(&self) -> u32 {
        self.width
    }

    fn height(&self) -> u32 {
        self.height
    }

    fn data(&self) -> &[u8] {
        &self.pixels
    }

    fn draw(&mut self, scene: &Scene, scale: f64, background: Color) {
        let damage = self.next_damage.take();
        self.last_readback = None;
        unsafe { (self.gl.bind_framebuffer)(GL_FRAMEBUFFER, self.fbo) };
        if !self.target_ok() {
            return;
        }
        self.frame = self.frame.wrapping_add(1);
        self.begin(background);

        let k = scale.max(0.01);
        let screen = Rect::from_xywh(0.0, 0.0, self.width as f64, self.height as f64);
        let mut clip_stack: Vec<(Rect, f64)> = Vec::new();
        let mut run: Vec<DrawCommand> = Vec::new();
        let mut run_bounds: Option<Rect> = None;

        for cmd in scene.commands() {
            match cmd {
                DrawCommand::PushClip { rect } => {
                    self.flush_run(&mut run, &mut run_bounds, k, &clip_stack);
                    let effective = clip_stack
                        .last()
                        .and_then(|(c, _)| c.intersection(rect))
                        .unwrap_or(*rect);
                    clip_stack.push((effective, 0.0));
                }
                DrawCommand::PushClipRounded { rect, radius } => {
                    self.flush_run(&mut run, &mut run_bounds, k, &clip_stack);
                    let (effective, r) = match clip_stack.last() {
                        Some((c, cr)) => (c.intersection(rect).unwrap_or(*rect), radius.min(*cr)),
                        None => (*rect, *radius),
                    };
                    clip_stack.push((effective, r));
                }
                DrawCommand::PopClip => {
                    self.flush_run(&mut run, &mut run_bounds, k, &clip_stack);
                    clip_stack.pop();
                }
                DrawCommand::Text(text_run) => {
                    // Tekst idzie glifami z atlasu — bez rasteryzacji warstwy CPU.
                    self.flush_run(&mut run, &mut run_bounds, k, &clip_stack);
                    self.draw_text(text_run, k, &clip_stack);
                }
                DrawCommand::StrokeLine { .. }
                | DrawCommand::StrokePolyline { .. }
                | DrawCommand::FillPolygon { .. } => {
                    extend(&mut run_bounds, crate::scene::command_area(cmd, &self.text));
                    run.push(cmd.clone());
                }
                // Reszta idzie na GPU; bieżący bieg CPU musi być najpierw wstawiony.
                DrawCommand::Background { data } => {
                    self.flush_run(&mut run, &mut run_bounds, k, &clip_stack);
                    let tex = self.image_texture(data);
                    if tex != 0 {
                        self.draw_texture(tex, screen, screen, None);
                    }
                }
                DrawCommand::Image { rect, data } => {
                    self.flush_run(&mut run, &mut run_bounds, k, &clip_stack);
                    if rect.width() <= 0.0 || rect.height() <= 0.0 {
                        continue;
                    }
                    let tex = self.image_texture(data);
                    if tex != 0 {
                        let (clip, round) = clip_for(&clip_stack, screen);
                        let phys = phys_rect(rect, k);
                        self.draw_texture(tex, phys, clip, round);
                    }
                }
                DrawCommand::ImageDmabuf { rect, data } => {
                    self.flush_run(&mut run, &mut run_bounds, k, &clip_stack);
                    if rect.width() <= 0.0 || rect.height() <= 0.0 {
                        continue;
                    }
                    let tex = self.dmabuf_texture(data);
                    if tex != 0 {
                        let (clip, round) = clip_for(&clip_stack, screen);
                        let phys = phys_rect(rect, k);
                        self.draw_texture(tex, phys, clip, round);
                    }
                }
                DrawCommand::FillRect { rect, color } => {
                    self.flush_run(&mut run, &mut run_bounds, k, &clip_stack);
                    let (clip, round) = clip_for(&clip_stack, screen);
                    self.draw_shape(phys_rect(rect, k), *color, 0, 0.0, 0.0, clip, round);
                }
                DrawCommand::FillRoundedRect {
                    rect,
                    radius,
                    color,
                } => {
                    self.flush_run(&mut run, &mut run_bounds, k, &clip_stack);
                    let (clip, round) = clip_for(&clip_stack, screen);
                    let mode = if *radius <= 0.0 { 0 } else { 1 };
                    self.draw_shape(
                        phys_rect(rect, k),
                        *color,
                        mode,
                        radius * k,
                        0.0,
                        clip,
                        round,
                    );
                }
                DrawCommand::StrokeRoundedRect {
                    rect,
                    radius,
                    width,
                    color,
                } => {
                    self.flush_run(&mut run, &mut run_bounds, k, &clip_stack);
                    let (clip, round) = clip_for(&clip_stack, screen);
                    // SDF rysuje obwódkę wokół ścieżki — poszerzamy prostokąt o pół
                    // grubości, żeby zewnętrzna krawędź wypadła jak w tiny-skia.
                    let half = width * k * 0.5;
                    let outer = Rect::from_xywh(
                        rect.x() * k - half,
                        rect.y() * k - half,
                        rect.width() * k + 2.0 * half,
                        rect.height() * k + 2.0 * half,
                    );
                    self.draw_shape(
                        outer,
                        *color,
                        2,
                        (radius + *width * 0.5) * k,
                        width * k,
                        clip,
                        round,
                    );
                }
                DrawCommand::FillCircle {
                    center,
                    radius,
                    color,
                } => {
                    self.flush_run(&mut run, &mut run_bounds, k, &clip_stack);
                    let (clip, round) = clip_for(&clip_stack, screen);
                    let r = radius * k;
                    let c = Point::new(center.x * k, center.y * k);
                    self.draw_shape(
                        Rect::from_xywh(c.x - r, c.y - r, 2.0 * r, 2.0 * r),
                        *color,
                        3,
                        r,
                        0.0,
                        clip,
                        round,
                    );
                }
                DrawCommand::StrokeCircle {
                    center,
                    radius,
                    width,
                    color,
                } => {
                    self.flush_run(&mut run, &mut run_bounds, k, &clip_stack);
                    let (clip, round) = clip_for(&clip_stack, screen);
                    let r = radius * k;
                    let c = Point::new(center.x * k, center.y * k);
                    self.draw_shape(
                        Rect::from_xywh(c.x - r, c.y - r, 2.0 * r, 2.0 * r),
                        *color,
                        4,
                        r,
                        width * k,
                        clip,
                        round,
                    );
                }
            }
        }
        self.flush_run(&mut run, &mut run_bounds, k, &clip_stack);
        self.finish(damage, k);
        self.evict_textures();
        if self.gl.error() != 0 {
            // Błąd GL nie może zatrzymać sesji — raportujemy raz na klatkę w `LUNA_GPU_STATS`.
            if std::env::var("LUNA_GPU_STATS").is_ok() {
                eprintln!("libluna-render: GL error 0x{:x}", self.gl.error());
            }
        }
    }

    fn encode_png(&self) -> Option<Vec<u8>> {
        let mut pixmap = Pixmap::new(self.width, self.height)?;
        if pixmap.data_mut().len() != self.pixels.len() {
            return None;
        }
        pixmap.data_mut().copy_from_slice(&self.pixels);
        pixmap.encode_png().ok()
    }

    fn text_engine(&self) -> &TextEngine {
        &self.text
    }
}

impl GpuRenderer {
    fn flush_run(
        &mut self,
        run: &mut Vec<DrawCommand>,
        bounds: &mut Option<Rect>,
        scale: f64,
        clip_stack: &[(Rect, f64)],
    ) {
        let Some(b) = bounds.take() else {
            run.clear();
            return;
        };
        let screen = Rect::from_xywh(0.0, 0.0, self.width as f64, self.height as f64);
        let (clip, round) = clip_for(clip_stack, screen);
        let cmds = std::mem::take(run);
        self.flush_cpu_layer(&cmds, b, scale, clip, round);
    }
}

/// Klucz cache glifów: znak, rozmiar w px×2 (albo 0 dla „bez bitmapy”) i pogrubienie.
type GlyphKey = (u32, u32, bool);

/// Wpis w atlasie: pozycja UV, rozmiar bitmapy i metryki wstawienia.
#[derive(Clone, Copy)]
struct Glyph {
    u: f32,
    v: f32,
    tw: u32,
    th: u32,
    xmin: i32,
    ymin: i32,
    advance: f32,
}

/// Atlas glifów: jedna tekstura RGBA, krycie w alfie, pakowanie półkowe.
struct Atlas {
    tex: Gluint,
    size: u32,
    x: u32,
    y: u32,
    row_h: u32,
}

impl Atlas {
    fn new(size: u32) -> Self {
        Self {
            tex: 0,
            size,
            x: 0,
            y: 0,
            row_h: 0,
        }
    }

    fn reset(&mut self) {
        self.x = 0;
        self.y = 0;
        self.row_h = 0;
    }
}

fn upload_texture(gl: &Gl, w: u32, h: u32, rgba: &[u8]) -> Gluint {
    unsafe {
        let mut tex = 0;
        (gl.gen_textures)(1, &mut tex);
        (gl.bind_texture)(GL_TEXTURE_2D, tex);
        (gl.tex_image_2d)(
            GL_TEXTURE_2D,
            0,
            GL_RGBA as i32,
            w as i32,
            h as i32,
            0,
            GL_RGBA,
            GL_UNSIGNED_BYTE,
            rgba.as_ptr() as *const c_void,
        );
        (gl.tex_parameteri)(GL_TEXTURE_2D, GL_TEXTURE_MIN_FILTER, GL_LINEAR as i32);
        (gl.tex_parameteri)(GL_TEXTURE_2D, GL_TEXTURE_MAG_FILTER, GL_LINEAR as i32);
        (gl.tex_parameteri)(GL_TEXTURE_2D, GL_TEXTURE_WRAP_S, GL_CLAMP_TO_EDGE as i32);
        (gl.tex_parameteri)(GL_TEXTURE_2D, GL_TEXTURE_WRAP_T, GL_CLAMP_TO_EDGE as i32);
        tex
    }
}

fn clip_for(stack: &[(Rect, f64)], screen: Rect) -> (Rect, Option<(Rect, f64)>) {
    let Some((rect, radius)) = stack.last().copied() else {
        return (screen, None);
    };
    let rect = rect.intersection(&screen).unwrap_or(Rect::ZERO);
    if radius > 0.0 {
        (rect, Some((rect, radius)))
    } else {
        (rect, None)
    }
}

fn phys_rect(r: &Rect, k: f64) -> Rect {
    Rect::from_xywh(r.x() * k, r.y() * k, r.width() * k, r.height() * k)
}

/// Przesuwa polecenie o `(dx, dy)` — do rasteryzacji warstwy CPU.
fn translate(cmd: &DrawCommand, dx: f64, dy: f64) -> DrawCommand {
    match cmd {
        DrawCommand::FillRect { rect, color } => DrawCommand::FillRect {
            rect: rect.offset(-dx, -dy),
            color: *color,
        },
        DrawCommand::FillRoundedRect {
            rect,
            radius,
            color,
        } => DrawCommand::FillRoundedRect {
            rect: rect.offset(-dx, -dy),
            radius: *radius,
            color: *color,
        },
        DrawCommand::StrokeRoundedRect {
            rect,
            radius,
            width,
            color,
        } => DrawCommand::StrokeRoundedRect {
            rect: rect.offset(-dx, -dy),
            radius: *radius,
            width: *width,
            color: *color,
        },
        DrawCommand::StrokeLine {
            from,
            to,
            width,
            color,
        } => DrawCommand::StrokeLine {
            from: Point::new(from.x - dx, from.y - dy),
            to: Point::new(to.x - dx, to.y - dy),
            width: *width,
            color: *color,
        },
        DrawCommand::FillCircle {
            center,
            radius,
            color,
        } => DrawCommand::FillCircle {
            center: Point::new(center.x - dx, center.y - dy),
            radius: *radius,
            color: *color,
        },
        DrawCommand::StrokeCircle {
            center,
            radius,
            width,
            color,
        } => DrawCommand::StrokeCircle {
            center: Point::new(center.x - dx, center.y - dy),
            radius: *radius,
            width: *width,
            color: *color,
        },
        DrawCommand::StrokePolyline {
            points,
            width,
            color,
        } => DrawCommand::StrokePolyline {
            points: points
                .iter()
                .map(|p| Point::new(p.x - dx, p.y - dy))
                .collect(),
            width: *width,
            color: *color,
        },
        DrawCommand::FillPolygon { points, color } => DrawCommand::FillPolygon {
            points: points
                .iter()
                .map(|p| Point::new(p.x - dx, p.y - dy))
                .collect(),
            color: *color,
        },
        DrawCommand::Text(run) => DrawCommand::Text(TextRun {
            origin: Point::new(run.origin.x - dx, run.origin.y - dy),
            text: run.text.clone(),
            size: run.size,
            weight: run.weight,
            color: run.color,
        }),
        DrawCommand::Image { rect, data } => DrawCommand::Image {
            rect: rect.offset(-dx, -dy),
            data: Arc::clone(data),
        },
        DrawCommand::ImageDmabuf { rect, data } => DrawCommand::ImageDmabuf {
            rect: rect.offset(-dx, -dy),
            data: Arc::clone(data),
        },
        DrawCommand::Background { data } => DrawCommand::Background {
            data: Arc::clone(data),
        },
        other => other.clone(),
    }
}

/// Przybliżone granice polecenia (piksele logiczne) — do rozmiaru warstwy CPU.
fn extend(acc: &mut Option<Rect>, r: Option<Rect>) {
    let Some(r) = r else { return };
    *acc = match *acc {
        Some(a) => Some(a.union(&r)),
        None => Some(r),
    };
}

impl TexProgram {
    fn new(gl: &Gl) -> Result<Self, String> {
        let id = link(gl, VS, FS_TEX)?;
        unsafe {
            (gl.use_program)(id);
            Ok(Self {
                id,
                pos: (gl.get_attrib_location)(id, b"pos\0".as_ptr() as *const Glchar) as Gluint,
                screen: (gl.get_uniform_location)(id, b"screen\0".as_ptr() as *const Glchar),
                rect: (gl.get_uniform_location)(id, b"rect\0".as_ptr() as *const Glchar),
                clip: (gl.get_uniform_location)(id, b"clip\0".as_ptr() as *const Glchar),
                round: (gl.get_uniform_location)(id, b"round\0".as_ptr() as *const Glchar),
                round_r: (gl.get_uniform_location)(id, b"round_r\0".as_ptr() as *const Glchar),
                tex: (gl.get_uniform_location)(id, b"tex\0".as_ptr() as *const Glchar),
            })
        }
    }
}

impl ShapeProgram {
    fn new(gl: &Gl) -> Result<Self, String> {
        let id = link(gl, VS, FS_SHAPE)?;
        unsafe {
            (gl.use_program)(id);
            Ok(Self {
                id,
                pos: (gl.get_attrib_location)(id, b"pos\0".as_ptr() as *const Glchar) as Gluint,
                screen: (gl.get_uniform_location)(id, b"screen\0".as_ptr() as *const Glchar),
                rect: (gl.get_uniform_location)(id, b"rect\0".as_ptr() as *const Glchar),
                color: (gl.get_uniform_location)(id, b"color\0".as_ptr() as *const Glchar),
                mode: (gl.get_uniform_location)(id, b"mode\0".as_ptr() as *const Glchar),
                radius: (gl.get_uniform_location)(id, b"radius\0".as_ptr() as *const Glchar),
                width: (gl.get_uniform_location)(id, b"width\0".as_ptr() as *const Glchar),
                clip: (gl.get_uniform_location)(id, b"clip\0".as_ptr() as *const Glchar),
                round: (gl.get_uniform_location)(id, b"round\0".as_ptr() as *const Glchar),
                round_r: (gl.get_uniform_location)(id, b"round_r\0".as_ptr() as *const Glchar),
            })
        }
    }
}

impl GlyphProgram {
    fn new(gl: &Gl) -> Result<Self, String> {
        let id = link(gl, VS, FS_GLYPH)?;
        unsafe {
            (gl.use_program)(id);
            Ok(Self {
                id,
                pos: (gl.get_attrib_location)(id, b"pos\0".as_ptr() as *const Glchar) as Gluint,
                screen: (gl.get_uniform_location)(id, b"screen\0".as_ptr() as *const Glchar),
                rect: (gl.get_uniform_location)(id, b"rect\0".as_ptr() as *const Glchar),
                uv: (gl.get_uniform_location)(id, b"uv\0".as_ptr() as *const Glchar),
                color: (gl.get_uniform_location)(id, b"color\0".as_ptr() as *const Glchar),
                clip: (gl.get_uniform_location)(id, b"clip\0".as_ptr() as *const Glchar),
                round: (gl.get_uniform_location)(id, b"round\0".as_ptr() as *const Glchar),
                round_r: (gl.get_uniform_location)(id, b"round_r\0".as_ptr() as *const Glchar),
                tex: (gl.get_uniform_location)(id, b"tex\0".as_ptr() as *const Glchar),
            })
        }
    }
}

fn link(gl: &Gl, vs_src: &str, fs_src: &str) -> Result<Gluint, String> {
    let vs = shader(gl, GL_VERTEX_SHADER, vs_src)?;
    let fs = shader(gl, GL_FRAGMENT_SHADER, fs_src)?;
    let id = unsafe { (gl.create_program)() };
    unsafe {
        (gl.attach_shader)(id, vs);
        (gl.attach_shader)(id, fs);
        (gl.link_program)(id);
    }
    let mut ok = 0;
    unsafe { (gl.get_programiv)(id, GL_LINK_STATUS, &mut ok) };
    if ok == 0 {
        let mut log = [0 as Glchar; 2048];
        unsafe { (gl.get_program_info_log)(id, 2048, ptr::null_mut(), log.as_mut_ptr()) };
        return Err(unsafe { CStr::from_ptr(log.as_ptr()) }
            .to_string_lossy()
            .into_owned());
    }
    Ok(id)
}

fn shader(gl: &Gl, kind: u32, src: &str) -> Result<Gluint, String> {
    let id = unsafe { (gl.create_shader)(kind) };
    let c = std::ffi::CString::new(src).map_err(|_| "NUL w shaderze".to_owned())?;
    let ptrs = [c.as_ptr() as *const Glchar];
    unsafe {
        (gl.shader_source)(id, 1, ptrs.as_ptr(), ptr::null());
        (gl.compile_shader)(id);
    }
    let mut ok = 0;
    unsafe { (gl.get_shaderiv)(id, GL_COMPILE_STATUS, &mut ok) };
    if ok == 0 {
        let mut log = [0 as Glchar; 2048];
        unsafe { (gl.get_shader_info_log)(id, 2048, ptr::null_mut(), log.as_mut_ptr()) };
        return Err(unsafe { CStr::from_ptr(log.as_ptr()) }
            .to_string_lossy()
            .into_owned());
    }
    Ok(id)
}

impl Scene {
    /// Wewnętrzny helper: budowa warstwy CPU z przetłumaczonych poleceń.
    fn push(&mut self, cmd: DrawCommand) {
        match cmd {
            DrawCommand::FillRect { rect, color } => {
                self.fill_rect(rect, color);
            }
            DrawCommand::FillRoundedRect {
                rect,
                radius,
                color,
            } => {
                self.fill_rounded_rect(rect, radius, color);
            }
            DrawCommand::StrokeRoundedRect {
                rect,
                radius,
                width,
                color,
            } => {
                self.stroke_rounded_rect(rect, radius, width, color);
            }
            DrawCommand::StrokeLine {
                from,
                to,
                width,
                color,
            } => {
                self.stroke_line(from, to, width, color);
            }
            DrawCommand::FillCircle {
                center,
                radius,
                color,
            } => {
                self.fill_circle(center, radius, color);
            }
            DrawCommand::StrokeCircle {
                center,
                radius,
                width,
                color,
            } => {
                self.stroke_circle(center, radius, width, color);
            }
            DrawCommand::StrokePolyline {
                points,
                width,
                color,
            } => {
                self.stroke_polyline(&points, width, color);
            }
            DrawCommand::FillPolygon { points, color } => {
                self.fill_polygon(&points, color);
            }
            DrawCommand::PushClip { rect } => {
                self.push_clip(rect);
            }
            DrawCommand::PushClipRounded { rect, radius } => {
                self.push_clip_rounded(rect, radius);
            }
            DrawCommand::PopClip => {
                self.pop_clip();
            }
            DrawCommand::Text(run) => {
                self.text(run);
            }
            DrawCommand::Image { rect, data } => {
                self.image(rect, data);
            }
            DrawCommand::ImageDmabuf { rect, data } => {
                self.image_dmabuf(rect, data);
            }
            DrawCommand::Background { data } => {
                self.background(data);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::DmabufPlane;
    use libluna_core::Color;

    /// Renderer GPU musi dać dokładnie `width*height*4` bajtów i rysować obraz
    /// w tym samym miejscu co CPU (z tolerancją na antyaliasing).
    #[test]
    fn gpu_matches_cpu_for_solid_rect() {
        if !available() {
            eprintln!("pominięto: brak EGL/GPU");
            return;
        }
        let mut gpu = match GpuRenderer::new(64, 32) {
            Ok(r) => r,
            Err(e) => {
                eprintln!("pominięto: {e}");
                return;
            }
        };
        let mut scene = Scene::new();
        scene.fill_rect(
            Rect::from_xywh(8.0, 8.0, 16.0, 16.0),
            Color::from_rgba8(0xff, 0x40, 0x20, 0xff),
        );
        gpu.draw(&scene, 1.0, Color::from_rgba8(0x10, 0x20, 0x30, 0xff));
        assert_eq!(gpu.data().len(), 64 * 32 * 4);
        let px = |x: usize, y: usize| {
            let i = (y * 64 + x) * 4;
            (gpu.data()[i], gpu.data()[i + 1], gpu.data()[i + 2])
        };
        assert_eq!(px(12, 12), (0xff, 0x40, 0x20));
        assert_eq!(px(0, 0), (0x10, 0x20, 0x30));
        assert!(gpu.encode_png().is_some());
    }

    /// Tekstura z `wl_shm` (`DrawCommand::Image`) musi być widoczna — to treść
    /// okien klientów. Test pilnuje, żeby GPU nie gubił obrazów w clipie.
    #[test]
    fn gpu_draws_shm_image() {
        if !available() {
            eprintln!("pominięto: brak EGL/GPU");
            return;
        }
        let mut gpu = match GpuRenderer::new(64, 64) {
            Ok(r) => r,
            Err(e) => {
                eprintln!("pominięto: {e}");
                return;
            }
        };
        let mut rgba = vec![0u8; 16 * 16 * 4];
        for px in rgba.chunks_exact_mut(4) {
            px.copy_from_slice(&[0xff, 0x00, 0x00, 0xff]);
        }
        let mut scene = Scene::new();
        scene.image(
            Rect::from_xywh(8.0, 8.0, 16.0, 16.0),
            std::sync::Arc::new(ImageData::new(16, 16, rgba.clone())),
        );
        gpu.draw(&scene, 1.0, Color::BLACK);
        let no_clip = pixel(&gpu, 16, 16);
        assert_eq!(no_clip, (0xff, 0x00, 0x00), "obraz shm bez clipu");

        let mut scene = Scene::new();
        scene.push_clip_rounded(Rect::from_xywh(8.0, 8.0, 16.0, 16.0), 4.0);
        scene.image(
            Rect::from_xywh(0.0, 0.0, 64.0, 64.0),
            std::sync::Arc::new(ImageData::new(
                64,
                64,
                vec![0xff, 0x00, 0x00, 0xff].repeat(64 * 64),
            )),
        );
        scene.pop_clip();
        gpu.draw(&scene, 1.0, Color::BLACK);
        assert_eq!(
            pixel(&gpu, 16, 16),
            (0xff, 0x00, 0x00),
            "obraz shm w clipie"
        );
        assert_eq!(pixel(&gpu, 0, 0), (0, 0, 0));
    }

    fn pixel(gpu: &GpuRenderer, x: usize, y: usize) -> (u8, u8, u8) {
        let i = (y * 64 + x) * 4;
        (gpu.data()[i], gpu.data()[i + 1], gpu.data()[i + 2])
    }

    /// Odczyt z damage: tylko wskazany prostokąt trafia do bufora, reszta zostaje
    /// z poprzedniej klatki (tak działa kompozycja przy małych zmianach).
    #[test]
    fn partial_readback_keeps_previous_pixels() {
        if !available() {
            eprintln!("pominięto: brak EGL/GPU");
            return;
        }
        let mut gpu = match GpuRenderer::new(64, 64) {
            Ok(r) => r,
            Err(e) => {
                eprintln!("pominięto: {e}");
                return;
            }
        };
        let red = Color::from_rgba8(0xff, 0x00, 0x00, 0xff);
        let blue = Color::from_rgba8(0x00, 0x00, 0xff, 0xff);
        let mut first = Scene::new();
        first.fill_rect(Rect::from_xywh(4.0, 4.0, 16.0, 16.0), red);
        gpu.draw(&first, 1.0, Color::BLACK);
        assert_eq!(pixel(&gpu, 8, 8), (0xff, 0x00, 0x00));

        // Druga klatka zmienia tylko prawy dolny róg.
        let mut second = Scene::new();
        second.fill_rect(Rect::from_xywh(4.0, 4.0, 16.0, 16.0), red);
        second.fill_rect(Rect::from_xywh(40.0, 40.0, 8.0, 8.0), blue);
        gpu.draw_damage(
            &second,
            1.0,
            Color::BLACK,
            Some(Rect::from_xywh(36.0, 36.0, 16.0, 16.0)),
        );
        assert_eq!(pixel(&gpu, 44, 44), (0x00, 0x00, 0xff), "nowy piksel");
        assert_eq!(
            pixel(&gpu, 8, 8),
            (0xff, 0x00, 0x00),
            "piksel poza damage musi zostać z poprzedniej klatki"
        );
        let region = gpu.last_readback().expect("region odczytu");
        assert!(region.width() <= 16.0 && region.height() <= 16.0);

        // Pusty damage = brak odczytu, bufor bez zmian.
        gpu.draw_damage(&second, 1.0, Color::BLACK, Some(Rect::ZERO));
        assert_eq!(pixel(&gpu, 8, 8), (0xff, 0x00, 0x00));
        assert_eq!(gpu.last_readback(), Some(Rect::ZERO));
    }

    /// Tekst musi iść z atlasu glifów (bez warstwy CPU) i być widoczny także
    /// w clipie zaokrąglonym — to tekst okien i pasków tytułu.
    #[test]
    fn gpu_draws_text_from_atlas() {
        if !available() {
            eprintln!("pominięto: brak EGL/GPU");
            return;
        }
        let mut gpu = match GpuRenderer::new(160, 48) {
            Ok(r) => r,
            Err(e) => {
                eprintln!("pominięto: {e}");
                return;
            }
        };
        if !gpu.text.has_font() {
            eprintln!("pominięto: brak fontu systemowego");
            return;
        }
        let run = TextRun::new(Point::new(6.0, 30.0), "Luna", 20.0)
            .with_color(Color::from_rgba8(0xff, 0xff, 0xff, 0xff));
        let mut scene = Scene::new();
        scene.text(run.clone());
        gpu.draw(&scene, 1.0, Color::BLACK);
        let lit = |gpu: &GpuRenderer| {
            gpu.data()
                .chunks_exact(4)
                .filter(|p| p[0] > 100 && p[1] > 100 && p[2] > 100)
                .count()
        };
        assert!(lit(&gpu) > 40, "tekst bez clipu niewidoczny");

        // Ten sam tekst w zaokrąglonym clipie (jak w oknie).
        let mut scene = Scene::new();
        scene.push_clip_rounded(Rect::from_xywh(0.0, 0.0, 160.0, 48.0), 10.0);
        scene.text(run);
        scene.pop_clip();
        gpu.draw(&scene, 1.0, Color::BLACK);
        assert!(lit(&gpu) > 40, "tekst w clipie zaokrąglonym niewidoczny");
    }

    /// Pełny round-trip: tekstura GL → eksport MESA do `dmabuf` → import przez
    /// `dmabuf_texture` → rysowanie. Sprawdza atrybuty EGL (fd/offset/stride/
    /// modifier) i to, że kolory nie są zamieniane.
    #[test]
    fn dmabuf_import_roundtrip_from_egl_export() {
        use std::os::fd::{FromRawFd, OwnedFd};

        if !available() {
            eprintln!("pominięto: brak EGL/GPU");
            return;
        }
        let mut gpu = match GpuRenderer::new(4, 4) {
            Ok(r) => r,
            Err(e) => {
                eprintln!("pominięto: {e}");
                return;
            }
        };
        let (Some(export), Some(query)) = (gpu.gl.export_image, gpu.gl.export_query) else {
            eprintln!("pominięto: brak EGL_MESA_image_dma_buf_export");
            return;
        };
        // Źródło: 4×4 w wyrazistych kolorach (premultiplied, alpha=255).
        let mut src = vec![0u8; 4 * 4 * 4];
        for y in 0..4u32 {
            for x in 0..4u32 {
                let i = ((y * 4 + x) * 4) as usize;
                src[i] = (x * 60) as u8;
                src[i + 1] = (y * 60) as u8;
                src[i + 2] = 200;
                src[i + 3] = 255;
            }
        }
        let tex = upload_texture(&gpu.gl, 4, 4, &src);
        // EGLImage z tekstury GL (wymaga bieżącego kontekstu).
        const EGL_GL_TEXTURE_2D_KHR: u32 = 0x30B1;
        const EGL_GL_TEXTURE_LEVEL_KHR: u32 = 0x30BC;
        let Some(create) = gpu.gl.create_image else {
            eprintln!("pominięto: brak eglCreateImage");
            return;
        };
        let attrs: [i32; 3] = [EGL_GL_TEXTURE_LEVEL_KHR as i32, 0, EGL_NONE as i32];
        let egl_image = unsafe {
            create(
                gpu.gl.dpy,
                gpu.gl._ctx,
                EGL_GL_TEXTURE_2D_KHR,
                tex as *mut c_void,
                attrs.as_ptr(),
            )
        };
        if egl_image.is_null() {
            eprintln!("pominięto: eglCreateImage(EGL_GL_TEXTURE_2D_KHR) nie powiodło się");
            return;
        }
        let (mut fourcc, mut planes, mut modifier) = (0i32, 1i32, 0u64);
        let ok = unsafe {
            query(
                gpu.gl.dpy,
                egl_image,
                &mut fourcc,
                &mut planes,
                &mut modifier,
            )
        };
        let (mut fd, mut stride, mut offset) = (0i32, 0i32, 0i32);
        if ok == 0
            || unsafe { export(gpu.gl.dpy, egl_image, &mut fd, &mut stride, &mut offset) } == 0
        {
            eprintln!("pominięto: eksport dmabuf nie powiódł się");
            return;
        }
        let image = Arc::new(DmabufImage {
            width: 4,
            height: 4,
            format: fourcc as u32,
            modifier,
            planes: vec![DmabufPlane {
                fd: unsafe { OwnedFd::from_raw_fd(fd) },
                offset: offset as u32,
                stride: stride as u32,
            }],
        });
        let mut scene = Scene::new();
        scene.image_dmabuf(Rect::from_xywh(0.0, 0.0, 4.0, 4.0), Arc::clone(&image));
        gpu.draw(&scene, 1.0, Color::from_rgba8(0, 0, 0, 255));
        // Import nie może dać czarnego/niebieskiego — porównujemy kilka pikseli.
        let px = |x: usize, y: usize| {
            let i = (y * 4 + x) * 4;
            (gpu.data()[i], gpu.data()[i + 1], gpu.data()[i + 2])
        };
        let want = |x: usize, y: usize| {
            let i = (y * 4 + x) * 4;
            (src[i], src[i + 1], src[i + 2])
        };
        for (x, y) in [(0, 0), (2, 1), (3, 3)] {
            assert_eq!(px(x, y), want(x, y), "piksel ({x},{y}) po round-tripie");
        }
        // Sprzątanie: EGLImage jest trzymany w cache tylko do eviction, więc
        // niszczymy go jawnie (tekstura źródłowa żyje do końca testu).
        if let Some(destroy) = gpu.gl.destroy_image {
            unsafe { destroy(gpu.gl.dpy, egl_image) };
        }
    }
}
