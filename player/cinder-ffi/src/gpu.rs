//! GPU present path — EGL + OpenGL ES 2.0 on the device's Mali fbdev driver.
//!
//! WHY: cinder_ui rasterizes the whole frame into a software `Canvas` (a `Vec<u32>`,
//! `0x00RRGGBB`). The original presentation step (`Framebuffer::blit` in lib.rs) memcpy'd that
//! buffer into the fb mmap on every page and forced a mode re-apply via `FBIOPUT_VSCREENINFO`.
//! That is a ~4.6 MB CPU copy per changed frame with no hardware vsync (uneven pacing / tearing).
//!
//! This module keeps the software rasterization unchanged but replaces the *present* with the GPU:
//! upload the Canvas to a single RGBA texture (`glTexSubImage2D`), draw one full-screen textured
//! quad, and `eglSwapBuffers` — which on this Mali fbdev build does the page-flip AND blocks on
//! vsync internally (`__egl_platform_supports_vsync_fbdev` / `_mali_uku_vsync_event_report` are
//! present in libMali_linux.so). Result: no manual flip ioctl, no triple memcpy, vsync'd pacing,
//! and headroom for GPU transitions/scaling later.
//!
//! Device stack (confirmed from the extracted rootfs, 2026-07-26):
//!   - GPU driver = `libMali_linux.so` (the glibc-native ARM Mali "linux"/fbdev build). It exports
//!     every `egl*`/`gl*` symbol directly; `libEGL.so.1` and `libGLESv2.so.2` are just symlinks to
//!     it. So we link `-l:libMali_linux.so` (see cinder-home/build.sh) and the symbols resolve.
//!   - The stock `egl_test` binary (also glibc) uses exactly `eglGetDisplay(EGL_DEFAULT_DISPLAY)`
//!     → `eglInitialize` → `eglBindAPI` → `eglChooseConfig` → `eglCreateWindowSurface` →
//!     `eglSwapBuffers` on `/dev/graphics/fb0`. We mirror that. The native window is the standard
//!     Mali `fbdev_window { u16 width; u16 height; }`.
//!
//! SAFETY MODEL: `GlPresenter::open` returns `Err` on ANY failure. lib.rs then falls back to the
//! proven software `Framebuffer`. So a GPU that misbehaves degrades to the old path, never a black
//! screen. `CINDER_GPU=0` in the environment forces the software path (debug escape hatch).
//!
//! Pixel format: the Canvas u32 is `0x00RRGGBB`, i.e. little-endian bytes `[B, G, R, 0]`. Uploaded
//! as `GL_RGBA`/`GL_UNSIGNED_BYTE` the sampler reads that as (r=B, g=G, b=R, a=0), so the fragment
//! shader swizzles `.bgr` and forces alpha 1.0 — no reliance on a BGRA texture extension.

// The real implementation is ARM-only: it references the device's EGL/GLES symbols, which don't
// exist on the host. Host builds (incl. `cargo test -p cinder-ffi`) get the stub below so the test
// binary links with no GPU libs. On the device the stub is never compiled.
#[cfg(target_arch = "arm")]
mod imp {
    use cinder_ui::Canvas;
    use std::ffi::c_void;
    use std::os::raw::c_char;

    // ── EGL / GLES2 opaque handles & scalar aliases ─────────────────────────────────────────────
    type EGLDisplay = *mut c_void;
    type EGLConfig = *mut c_void;
    type EGLSurface = *mut c_void;
    type EGLContext = *mut c_void;
    type EGLNativeDisplayType = *mut c_void;
    type EGLNativeWindowType = *mut c_void;
    type EGLint = i32;
    type EGLenum = u32;
    type EGLBoolean = u32;
    type GLuint = u32;
    type GLint = i32;
    type GLenum = u32;
    type GLsizei = i32;
    type GLchar = c_char;

    const EGL_FALSE: EGLBoolean = 0;
    const EGL_NONE: EGLint = 0x3038;
    const EGL_SURFACE_TYPE: EGLint = 0x3033;
    const EGL_WINDOW_BIT: EGLint = 0x0004;
    const EGL_RENDERABLE_TYPE: EGLint = 0x3040;
    const EGL_OPENGL_ES2_BIT: EGLint = 0x0004;
    const EGL_RED_SIZE: EGLint = 0x3024;
    const EGL_GREEN_SIZE: EGLint = 0x3023;
    const EGL_BLUE_SIZE: EGLint = 0x3022;
    const EGL_ALPHA_SIZE: EGLint = 0x3021;
    const EGL_CONTEXT_CLIENT_VERSION: EGLint = 0x3098;
    const EGL_OPENGL_ES_API: EGLenum = 0x30A0;

    const GL_TEXTURE_2D: GLenum = 0x0DE1;
    const GL_TEXTURE_MIN_FILTER: GLenum = 0x2801;
    const GL_TEXTURE_MAG_FILTER: GLenum = 0x2800;
    const GL_TEXTURE_WRAP_S: GLenum = 0x2802;
    const GL_TEXTURE_WRAP_T: GLenum = 0x2803;
    const GL_LINEAR: GLint = 0x2601;
    const GL_CLAMP_TO_EDGE: GLint = 0x812F;
    const GL_RGBA: GLenum = 0x1908;
    const GL_UNSIGNED_BYTE: GLenum = 0x1401;
    const GL_FRAGMENT_SHADER: GLenum = 0x8B30;
    const GL_VERTEX_SHADER: GLenum = 0x8B31;
    const GL_COMPILE_STATUS: GLenum = 0x8B81;
    const GL_LINK_STATUS: GLenum = 0x8B82;
    const GL_FLOAT: GLenum = 0x1406;
    const GL_FALSE_GL: u8 = 0;
    const GL_TRIANGLE_STRIP: GLenum = 0x0005;
    const GL_COLOR_BUFFER_BIT: GLenum = 0x0000_4000;
    const GL_TEXTURE0: GLenum = 0x84C0;

    #[allow(non_snake_case)]
    extern "C" {
        fn eglGetDisplay(display_id: EGLNativeDisplayType) -> EGLDisplay;
        fn eglInitialize(dpy: EGLDisplay, major: *mut EGLint, minor: *mut EGLint) -> EGLBoolean;
        fn eglBindAPI(api: EGLenum) -> EGLBoolean;
        fn eglChooseConfig(
            dpy: EGLDisplay,
            attrib_list: *const EGLint,
            configs: *mut EGLConfig,
            config_size: EGLint,
            num_config: *mut EGLint,
        ) -> EGLBoolean;
        fn eglCreateWindowSurface(
            dpy: EGLDisplay,
            config: EGLConfig,
            win: EGLNativeWindowType,
            attrib_list: *const EGLint,
        ) -> EGLSurface;
        fn eglCreateContext(
            dpy: EGLDisplay,
            config: EGLConfig,
            share_context: EGLContext,
            attrib_list: *const EGLint,
        ) -> EGLContext;
        fn eglMakeCurrent(
            dpy: EGLDisplay,
            draw: EGLSurface,
            read: EGLSurface,
            ctx: EGLContext,
        ) -> EGLBoolean;
        fn eglSwapBuffers(dpy: EGLDisplay, surface: EGLSurface) -> EGLBoolean;
        fn eglSwapInterval(dpy: EGLDisplay, interval: EGLint) -> EGLBoolean;
        fn eglGetError() -> EGLint;

        fn glGenTextures(n: GLsizei, textures: *mut GLuint);
        fn glBindTexture(target: GLenum, texture: GLuint);
        fn glTexParameteri(target: GLenum, pname: GLenum, param: GLint);
        fn glTexImage2D(
            target: GLenum,
            level: GLint,
            internalformat: GLint,
            width: GLsizei,
            height: GLsizei,
            border: GLint,
            format: GLenum,
            type_: GLenum,
            pixels: *const c_void,
        );
        fn glTexSubImage2D(
            target: GLenum,
            level: GLint,
            xoffset: GLint,
            yoffset: GLint,
            width: GLsizei,
            height: GLsizei,
            format: GLenum,
            type_: GLenum,
            pixels: *const c_void,
        );
        fn glClearColor(r: f32, g: f32, b: f32, a: f32);
        fn glClear(mask: GLenum);
        fn glViewport(x: GLint, y: GLint, width: GLsizei, height: GLsizei);
        fn glActiveTexture(texture: GLenum);
        fn glCreateShader(shader_type: GLenum) -> GLuint;
        fn glShaderSource(
            shader: GLuint,
            count: GLsizei,
            string: *const *const GLchar,
            length: *const GLint,
        );
        fn glCompileShader(shader: GLuint);
        fn glGetShaderiv(shader: GLuint, pname: GLenum, params: *mut GLint);
        fn glCreateProgram() -> GLuint;
        fn glAttachShader(program: GLuint, shader: GLuint);
        fn glLinkProgram(program: GLuint);
        fn glGetProgramiv(program: GLuint, pname: GLenum, params: *mut GLint);
        fn glUseProgram(program: GLuint);
        fn glGetAttribLocation(program: GLuint, name: *const GLchar) -> GLint;
        fn glGetUniformLocation(program: GLuint, name: *const GLchar) -> GLint;
        fn glUniform1i(location: GLint, v0: GLint);
        fn glVertexAttribPointer(
            index: GLuint,
            size: GLint,
            type_: GLenum,
            normalized: u8,
            stride: GLsizei,
            pointer: *const c_void,
        );
        fn glEnableVertexAttribArray(index: GLuint);
        fn glDrawArrays(mode: GLenum, first: GLint, count: GLsizei);
    }

    // Mali fbdev native window: the driver reads {width,height} and opens /dev/graphics/fb0 itself.
    #[repr(C)]
    struct FbdevWindow {
        width: u16,
        height: u16,
    }

    // ── Device-node preflight ───────────────────────────────────────────────────────────────────
    // The 2026-07-26 device session proved that uid-100 EGL init doesn't fail cleanly when a node
    // it needs is unopenable — it HANGS inside the driver (which wedged the boot and tripped the
    // launcher's bad-boot counter). So we never enter eglInitialize until every node the driver
    // touches is confirmed R+W-accessible to our real uid. If any is blocked we exec the
    // setuid-root cinder-gpunode helper (chmod 0666 on the four root-only nodes) and re-check;
    // still blocked → clean Err → lib.rs falls back to the software framebuffer. Hang class gone.
    extern "C" {
        fn access(pathname: *const c_char, mode: i32) -> i32;
    }
    const R_OK: i32 = 4;
    const W_OK: i32 = 2;

    /// Every device node the Mali fbdev EGL stack opens for a window surface. The first two are
    /// system-owned (always fine for uid 100); the last four ship root-only — cinder-gpunode's list.
    const GPU_NODES: [&str; 6] = [
        "/dev/mali",
        "/dev/graphics/fb0",
        "/dev/ion",
        "/dev/mtkfb_vsync",
        "/dev/mtk_disp",
        "/dev/sw_sync",
    ];
    const GPUNODE_HELPER: &str = "/system/vendor/unknown321/bin/cinder-gpunode";

    /// First node NOT R+W-accessible to the real uid, or None if all are open to us.
    fn blocked_node() -> Option<&'static str> {
        GPU_NODES.iter().copied().find(|p| {
            let c = std::ffi::CString::new(*p).unwrap();
            unsafe { access(c.as_ptr(), R_OK | W_OK) != 0 }
        })
    }

    /// Preflight: verify node access, running the setuid helper once if needed.
    fn ensure_node_access() -> Result<(), String> {
        if blocked_node().is_none() {
            return Ok(());
        }
        match std::process::Command::new(GPUNODE_HELPER).status() {
            Ok(st) if st.success() => {}
            Ok(st) => eprintln!("cinder-ffi: {GPUNODE_HELPER} exited {st} (continuing to re-check)"),
            Err(e) => eprintln!("cinder-ffi: cannot exec {GPUNODE_HELPER}: {e}"),
        }
        match blocked_node() {
            None => Ok(()),
            Some(p) => Err(format!(
                "GPU node {p} not accessible to uid 100 (helper missing/failed) — refusing EGL init"
            )),
        }
    }

    // Full-screen quad as a triangle strip: (pos.x, pos.y, tex.u, tex.v). The V is flipped so
    // Canvas row 0 (top) maps to the top of the screen (GL's texture origin is bottom-left).
    #[rustfmt::skip]
    const QUAD: [f32; 16] = [
        -1.0, -1.0, 0.0, 1.0, // bottom-left  -> canvas bottom
         1.0, -1.0, 1.0, 1.0, // bottom-right
        -1.0,  1.0, 0.0, 0.0, // top-left     -> canvas top
         1.0,  1.0, 1.0, 0.0, // top-right
    ];

    const VERT_SRC: &[u8] = b"attribute vec2 aPos;\n\
        attribute vec2 aTex;\n\
        varying vec2 vTex;\n\
        void main() {\n\
            vTex = aTex;\n\
            gl_Position = vec4(aPos, 0.0, 1.0);\n\
        }\n\0";

    // Swizzle .bgr (Canvas is BGRX in memory) and force opaque alpha.
    const FRAG_SRC: &[u8] = b"precision mediump float;\n\
        varying vec2 vTex;\n\
        uniform sampler2D uTex;\n\
        void main() {\n\
            vec4 c = texture2D(uTex, vTex);\n\
            gl_FragColor = vec4(c.b, c.g, c.r, 1.0);\n\
        }\n\0";

    pub struct GlPresenter {
        dpy: EGLDisplay,
        surf: EGLSurface,
        // The context is made current once and held for the whole process life (there is no runtime
        // shutdown path that drops the presenter), so it is never read again after `eglMakeCurrent`.
        _ctx: EGLContext,
        win: *mut FbdevWindow, // heap-owned; the driver keeps this pointer for the surface's life
        tex: GLuint,
        prog: GLuint,
        a_pos: GLint,
        a_tex: GLint,
        w: GLsizei,
        h: GLsizei,
    }

    // Only ever touched under lib.rs's global Mutex (like Framebuffer). The raw EGL/GL handles make
    // it non-Send by default; the access discipline is identical to the software path.
    unsafe impl Send for GlPresenter {}

    unsafe fn compile_shader(kind: GLenum, src: &[u8]) -> Result<GLuint, String> {
        let sh = glCreateShader(kind);
        if sh == 0 {
            return Err("glCreateShader==0".into());
        }
        let ptr = src.as_ptr() as *const GLchar;
        // length NULL => the source is NUL-terminated (our literals end in \0).
        glShaderSource(sh, 1, &ptr, std::ptr::null());
        glCompileShader(sh);
        let mut ok: GLint = 0;
        glGetShaderiv(sh, GL_COMPILE_STATUS, &mut ok);
        if ok == 0 {
            return Err(format!("shader {kind:#x} compile failed"));
        }
        Ok(sh)
    }

    impl GlPresenter {
        pub fn open(w: i32, h: i32) -> Result<Self, String> {
            // MUST come first: entering EGL with an unopenable node hangs the driver (no Err).
            ensure_node_access()?;
            unsafe {
                let dpy = eglGetDisplay(std::ptr::null_mut()); // EGL_DEFAULT_DISPLAY
                if dpy.is_null() {
                    return Err("eglGetDisplay -> EGL_NO_DISPLAY".into());
                }
                let (mut major, mut minor): (EGLint, EGLint) = (0, 0);
                if eglInitialize(dpy, &mut major, &mut minor) == EGL_FALSE {
                    return Err(format!("eglInitialize failed (egl err {:#x})", eglGetError()));
                }
                if eglBindAPI(EGL_OPENGL_ES_API) == EGL_FALSE {
                    return Err("eglBindAPI(ES) failed".into());
                }
                let cfg_attrs: [EGLint; 13] = [
                    EGL_SURFACE_TYPE, EGL_WINDOW_BIT,
                    EGL_RENDERABLE_TYPE, EGL_OPENGL_ES2_BIT,
                    EGL_RED_SIZE, 8,
                    EGL_GREEN_SIZE, 8,
                    EGL_BLUE_SIZE, 8,
                    EGL_ALPHA_SIZE, 0,
                    EGL_NONE,
                ];
                let mut cfg: EGLConfig = std::ptr::null_mut();
                let mut n: EGLint = 0;
                if eglChooseConfig(dpy, cfg_attrs.as_ptr(), &mut cfg, 1, &mut n) == EGL_FALSE
                    || n < 1
                {
                    return Err(format!("eglChooseConfig: no config (n={n})"));
                }

                // Heap-allocate the native window so its address is stable regardless of where the
                // enclosing Render struct lives (the driver retains this pointer).
                let win = Box::into_raw(Box::new(FbdevWindow {
                    width: w as u16,
                    height: h as u16,
                }));
                let surf =
                    eglCreateWindowSurface(dpy, cfg, win as EGLNativeWindowType, std::ptr::null());
                if surf.is_null() {
                    drop(Box::from_raw(win));
                    return Err(format!(
                        "eglCreateWindowSurface failed (egl err {:#x})",
                        eglGetError()
                    ));
                }

                let ctx_attrs: [EGLint; 3] = [EGL_CONTEXT_CLIENT_VERSION, 2, EGL_NONE];
                let ctx =
                    eglCreateContext(dpy, cfg, std::ptr::null_mut(), ctx_attrs.as_ptr());
                if ctx.is_null() {
                    drop(Box::from_raw(win));
                    return Err(format!("eglCreateContext failed (egl err {:#x})", eglGetError()));
                }
                if eglMakeCurrent(dpy, surf, surf, ctx) == EGL_FALSE {
                    drop(Box::from_raw(win));
                    return Err(format!("eglMakeCurrent failed (egl err {:#x})", eglGetError()));
                }
                let _ = eglSwapInterval(dpy, 1); // vsync (best-effort; not fatal if unsupported)

                // Shader program.
                let vs = compile_shader(GL_VERTEX_SHADER, VERT_SRC)?;
                let fs = compile_shader(GL_FRAGMENT_SHADER, FRAG_SRC)?;
                let prog = glCreateProgram();
                if prog == 0 {
                    return Err("glCreateProgram==0".into());
                }
                glAttachShader(prog, vs);
                glAttachShader(prog, fs);
                glLinkProgram(prog);
                let mut linked: GLint = 0;
                glGetProgramiv(prog, GL_LINK_STATUS, &mut linked);
                if linked == 0 {
                    return Err("program link failed".into());
                }
                let a_pos = glGetAttribLocation(prog, b"aPos\0".as_ptr() as *const GLchar);
                let a_tex = glGetAttribLocation(prog, b"aTex\0".as_ptr() as *const GLchar);
                let u_tex = glGetUniformLocation(prog, b"uTex\0".as_ptr() as *const GLchar);
                if a_pos < 0 || a_tex < 0 {
                    return Err("attribute location not found".into());
                }

                // The streaming texture (Canvas-sized, RGBA). Allocated once; updated per frame with
                // glTexSubImage2D. GLES2 supports non-power-of-two textures with CLAMP_TO_EDGE + no
                // mipmaps, which is exactly this case.
                let mut tex: GLuint = 0;
                glGenTextures(1, &mut tex);
                glBindTexture(GL_TEXTURE_2D, tex);
                glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_MIN_FILTER, GL_LINEAR);
                glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_MAG_FILTER, GL_LINEAR);
                glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_WRAP_S, GL_CLAMP_TO_EDGE);
                glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_WRAP_T, GL_CLAMP_TO_EDGE);
                glTexImage2D(
                    GL_TEXTURE_2D,
                    0,
                    GL_RGBA as GLint,
                    w,
                    h,
                    0,
                    GL_RGBA,
                    GL_UNSIGNED_BYTE,
                    std::ptr::null(),
                );

                glViewport(0, 0, w, h);
                glUseProgram(prog);
                glUniform1i(u_tex, 0); // sampler uses texture unit 0

                // Clear both/all back buffers to black so no boot-animation garbage flashes through
                // the alternating buffers before the first real frame lands.
                glClearColor(0.0, 0.0, 0.0, 1.0);
                for _ in 0..3 {
                    glClear(GL_COLOR_BUFFER_BIT);
                    eglSwapBuffers(dpy, surf);
                }

                Ok(GlPresenter { dpy, surf, _ctx: ctx, win, tex, prog, a_pos, a_tex, w, h })
            }
        }

        pub fn present(&mut self, canvas: &Canvas) {
            unsafe {
                glUseProgram(self.prog);
                glActiveTexture(GL_TEXTURE0);
                glBindTexture(GL_TEXTURE_2D, self.tex);
                // Upload the freshly-rasterized Canvas. buf is W*H u32 (BGRX); the fragment shader
                // swizzles to RGB. One full-frame upload replaces the old 3x fb memcpy.
                glTexSubImage2D(
                    GL_TEXTURE_2D,
                    0,
                    0,
                    0,
                    self.w,
                    self.h,
                    GL_RGBA,
                    GL_UNSIGNED_BYTE,
                    canvas.buf.as_ptr() as *const c_void,
                );

                let stride = (4 * std::mem::size_of::<f32>()) as GLsizei;
                let base = QUAD.as_ptr() as *const u8;
                glEnableVertexAttribArray(self.a_pos as GLuint);
                glVertexAttribPointer(
                    self.a_pos as GLuint,
                    2,
                    GL_FLOAT,
                    GL_FALSE_GL,
                    stride,
                    base as *const c_void,
                );
                glEnableVertexAttribArray(self.a_tex as GLuint);
                glVertexAttribPointer(
                    self.a_tex as GLuint,
                    2,
                    GL_FLOAT,
                    GL_FALSE_GL,
                    stride,
                    base.add(2 * std::mem::size_of::<f32>()) as *const c_void,
                );

                glDrawArrays(GL_TRIANGLE_STRIP, 0, 4);
                // Presents the frame and blocks on vsync (Mali fbdev). No manual FBIOPUT flip.
                eglSwapBuffers(self.dpy, self.surf);
            }
        }
    }

    impl Drop for GlPresenter {
        fn drop(&mut self) {
            // Best-effort teardown. On process exit the driver reclaims everything anyway; this
            // keeps a clean shutdown if the presenter is ever dropped while the process lives.
            unsafe {
                if !self.win.is_null() {
                    drop(Box::from_raw(self.win));
                    self.win = std::ptr::null_mut();
                }
            }
        }
    }
}

// Host / non-ARM stub: no GPU, always returns Err so lib.rs uses the software framebuffer. Keeps
// `cargo test -p cinder-ffi` linkable with no EGL/GLES libraries present.
#[cfg(not(target_arch = "arm"))]
mod imp {
    use cinder_ui::Canvas;

    pub struct GlPresenter;

    impl GlPresenter {
        pub fn open(_w: i32, _h: i32) -> Result<Self, String> {
            Err("GPU present path is ARM-only (host build)".into())
        }
        pub fn present(&mut self, _canvas: &Canvas) {}
    }
}

pub use imp::GlPresenter;
