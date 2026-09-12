//! The window.
//!
//! WHY THERE IS NO TOOLKIT HERE. This binary is the one artefact an end user runs, it ships as a
//! single file from a GitHub release, and `Cargo.toml` has an empty `[dependencies]` on purpose:
//! every crate added is a crate somebody has to trust in order to plug in a music player. A GUI
//! framework would be the largest dependency in the project by an order of magnitude, and it would
//! be carrying a window, six buttons and a log pane. Windows already ships those. So this talks to
//! user32/gdi32/comctl32 directly — more code here, nothing new to trust, and the .exe stays
//! small enough to be worth checksumming.
//!
//! STRUCTURE. One window, five pages, one state struct in thread-local storage (the window
//! procedure only ever runs on the thread that made the window). The actual work — staging files
//! and handing off to Sony's updater — runs on a worker thread so the window keeps painting, and
//! talks back through a queue plus `PostMessage`. Everything it does goes through `stage`, the
//! same module the text front end uses, so there is exactly one implementation of "install".

#![allow(non_snake_case)]

use std::ffi::c_void;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicI32, AtomicUsize, Ordering};
use std::sync::Mutex;

use crate::catalogue::{Comp, Kind};
use crate::device::{self, Installed};
use crate::stage::{self, Action};

// ── the thin slice of Win32 this needs ─────────────────────────────────────────────────────

type HWND = *mut c_void;
type HINSTANCE = *mut c_void;
type HDC = *mut c_void;
type HBRUSH = *mut c_void;
type HFONT = *mut c_void;
type HICON = *mut c_void;
type HMENU = *mut c_void;
type WPARAM = usize;
type LPARAM = isize;
type LRESULT = isize;

#[repr(C)]
struct WNDCLASSEXW {
    cbSize: u32,
    style: u32,
    lpfnWndProc: Option<unsafe extern "system" fn(HWND, u32, WPARAM, LPARAM) -> LRESULT>,
    cbClsExtra: i32,
    cbWndExtra: i32,
    hInstance: HINSTANCE,
    hIcon: HICON,
    hCursor: *mut c_void,
    hbrBackground: HBRUSH,
    lpszMenuName: *const u16,
    lpszClassName: *const u16,
    hIconSm: HICON,
}

#[repr(C)]
#[derive(Default, Clone, Copy)]
struct RECT {
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
}

#[repr(C)]
struct MSG {
    hwnd: HWND,
    message: u32,
    wParam: WPARAM,
    lParam: LPARAM,
    time: u32,
    pt: [i32; 2],
}

#[repr(C)]
struct PAINTSTRUCT {
    hdc: HDC,
    fErase: i32,
    rcPaint: RECT,
    fRestore: i32,
    fIncUpdate: i32,
    rgbReserved: [u8; 32],
}

/// Only `ptMinTrackSize` is written; the rest is layout so the offset is right.
#[repr(C)]
struct MINMAXINFO {
    ptReserved: [i32; 2],
    ptMaxSize: [i32; 2],
    ptMaxPosition: [i32; 2],
    ptMinTrackSize: [i32; 2],
    ptMaxTrackSize: [i32; 2],
}

#[repr(C)]
struct INITCOMMONCONTROLSEX {
    dwSize: u32,
    dwICC: u32,
}

#[link(name = "user32")]
extern "system" {
    fn RegisterClassExW(c: *const WNDCLASSEXW) -> u16;
    fn CreateWindowExW(
        ex: u32,
        class: *const u16,
        title: *const u16,
        style: u32,
        x: i32,
        y: i32,
        w: i32,
        h: i32,
        parent: HWND,
        menu: HMENU,
        inst: HINSTANCE,
        param: *mut c_void,
    ) -> HWND;
    fn DefWindowProcW(h: HWND, m: u32, w: WPARAM, l: LPARAM) -> LRESULT;
    fn ShowWindow(h: HWND, cmd: i32) -> i32;
    fn UpdateWindow(h: HWND) -> i32;
    fn GetMessageW(m: *mut MSG, h: HWND, min: u32, max: u32) -> i32;
    fn TranslateMessage(m: *const MSG) -> i32;
    fn DispatchMessageW(m: *const MSG) -> LRESULT;
    fn PostMessageW(h: HWND, m: u32, w: WPARAM, l: LPARAM) -> i32;
    fn SendMessageW(h: HWND, m: u32, w: WPARAM, l: LPARAM) -> LRESULT;
    fn PostQuitMessage(code: i32);
    fn DestroyWindow(h: HWND) -> i32;
    fn SetWindowTextW(h: HWND, s: *const u16) -> i32;
    fn MoveWindow(h: HWND, x: i32, y: i32, w: i32, hgt: i32, repaint: i32) -> i32;
    fn GetClientRect(h: HWND, r: *mut RECT) -> i32;
    fn InvalidateRect(h: HWND, r: *const RECT, erase: i32) -> i32;
    fn BeginPaint(h: HWND, ps: *mut PAINTSTRUCT) -> HDC;
    fn EndPaint(h: HWND, ps: *const PAINTSTRUCT) -> i32;
    fn FillRect(dc: HDC, r: *const RECT, br: HBRUSH) -> i32;
    fn DrawTextW(dc: HDC, s: *const u16, len: i32, r: *mut RECT, fmt: u32) -> i32;
    fn LoadCursorW(inst: HINSTANCE, name: *const u16) -> *mut c_void;
    fn MessageBoxW(h: HWND, text: *const u16, cap: *const u16, ty: u32) -> i32;
    fn EnableWindow(h: HWND, on: i32) -> i32;
    fn SetFocus(h: HWND) -> HWND;
    fn IsDialogMessageW(h: HWND, m: *mut MSG) -> i32;
    fn SetProcessDPIAware() -> i32;
    fn GetDlgCtrlID(h: HWND) -> i32;
    fn GetDC(h: HWND) -> HDC;
    fn ReleaseDC(h: HWND, dc: HDC) -> i32;
    fn GetSystemMetrics(i: i32) -> i32;
}

#[link(name = "gdi32")]
extern "system" {
    fn CreateSolidBrush(color: u32) -> HBRUSH;
    fn GetDeviceCaps(dc: HDC, index: i32) -> i32;
    fn DeleteObject(o: *mut c_void) -> i32;
    fn SetTextColor(dc: HDC, c: u32) -> u32;
    fn SetBkMode(dc: HDC, mode: i32) -> i32;
    fn SelectObject(dc: HDC, o: *mut c_void) -> *mut c_void;
    fn CreateFontW(
        h: i32,
        w: i32,
        esc: i32,
        orient: i32,
        weight: i32,
        italic: u32,
        underline: u32,
        strikeout: u32,
        charset: u32,
        outprec: u32,
        clipprec: u32,
        quality: u32,
        pitch: u32,
        face: *const u16,
    ) -> HFONT;
}

#[link(name = "comctl32")]
extern "system" {
    fn InitCommonControlsEx(p: *const INITCOMMONCONTROLSEX) -> i32;
}

#[link(name = "shell32")]
extern "system" {
    fn ShellExecuteW(
        h: HWND,
        verb: *const u16,
        file: *const u16,
        params: *const u16,
        dir: *const u16,
        show: i32,
    ) -> *mut c_void;
}

#[link(name = "kernel32")]
extern "system" {
    fn GetModuleHandleW(name: *const u16) -> HINSTANCE;
    fn GetConsoleProcessList(list: *mut u32, count: u32) -> u32;
    fn FreeConsole() -> i32;
}

const WS_CHILD: u32 = 0x4000_0000;
const WS_VISIBLE: u32 = 0x1000_0000;
const WS_TABSTOP: u32 = 0x0001_0000;
const WS_VSCROLL: u32 = 0x0020_0000;
const WS_BORDER: u32 = 0x0080_0000;
const WS_OVERLAPPEDWINDOW: u32 = 0x00CF_0000;
const WS_CLIPCHILDREN: u32 = 0x0200_0000;
const BS_AUTOCHECKBOX: u32 = 0x0003;
const BS_DEFPUSHBUTTON: u32 = 0x0001;
const BS_MULTILINE: u32 = 0x2000;
const ES_MULTILINE: u32 = 0x0004;
const ES_READONLY: u32 = 0x0800;
const ES_AUTOVSCROLL: u32 = 0x0040;
const CBS_DROPDOWNLIST: u32 = 0x0003;
const SS_NOPREFIX: u32 = 0x0080;

const WM_DESTROY: u32 = 0x0002;
const WM_SIZE: u32 = 0x0005;
const WM_PAINT: u32 = 0x000F;
const WM_CLOSE: u32 = 0x0010;
const WM_ERASEBKGND: u32 = 0x0014;
const WM_SETFONT: u32 = 0x0030;
const WM_COMMAND: u32 = 0x0111;
const WM_CTLCOLORSTATIC: u32 = 0x0138;
const WM_GETMINMAXINFO: u32 = 0x0024;
const WM_APP: u32 = 0x8000;
/// The worker pushed lines into `QUEUE`.
const WM_APP_LOG: u32 = WM_APP + 1;
/// The worker finished; wParam is 1 for success.
const WM_APP_DONE: u32 = WM_APP + 2;
/// The release check came back.
const WM_APP_RELEASE: u32 = WM_APP + 3;

const BM_GETCHECK: u32 = 0x00F0;
const BM_SETCHECK: u32 = 0x00F1;
const CB_ADDSTRING: u32 = 0x0143;
const CB_SETCURSEL: u32 = 0x014E;
const CB_GETCURSEL: u32 = 0x0147;
const EM_SETSEL: u32 = 0x00B1;
const EM_REPLACESEL: u32 = 0x00C2;

const SW_SHOW: i32 = 5;
const MB_ICONWARNING: u32 = 0x0030;
const MB_YESNO: u32 = 0x0004;
const IDYES: i32 = 6;
const TRANSPARENT: i32 = 1;
const DT_LEFT: u32 = 0x0000;
const DT_END_ELLIPSIS: u32 = 0x8000;
const LOGPIXELSX: i32 = 88;
const SM_CXSCREEN: i32 = 0;
const SM_CYSCREEN: i32 = 1;

fn w(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Win32 colours are 0x00BBGGRR, which is the reverse of every hex colour anyone writes down.
const fn rgb(r: u8, g: u8, b: u8) -> u32 {
    (r as u32) | ((g as u32) << 8) | ((b as u32) << 16)
}

// The palette: Cinder's ember accent on a dark header band, over a light body.
//
// The body is deliberately LIGHT. Every control on these pages is a stock Windows one — the whole
// point of not shipping a toolkit — and stock controls paint themselves in the system theme. A
// dark background under them gives white buttons and white combo boxes floating on black, which
// reads as a rendering fault rather than as a design. The band carries the brand instead.
const C_BG: u32 = rgb(0xF6, 0xF6, 0xF8);
const C_PANEL: u32 = rgb(0x1A, 0x18, 0x18);
const C_TEXT: u32 = rgb(0x1A, 0x1A, 0x1E);
const C_DIM: u32 = rgb(0x6C, 0x6C, 0x74);
const C_BAND_TEXT: u32 = rgb(0xF2, 0xEF, 0xEC);
const C_BAND_DIM: u32 = rgb(0x9A, 0x94, 0x90);
const C_ACCENT: u32 = rgb(0xE0, 0x55, 0x1B);
const C_WARN: u32 = rgb(0xB3, 0x2E, 0x14);

// ── control ids ────────────────────────────────────────────────────────────────────────────

const ID_DRIVE: usize = 100;
const ID_RESCAN: usize = 101;
const ID_INSTALL: usize = 102;
const ID_UPDATE: usize = 103;
const ID_UNINSTALL: usize = 104;
const ID_CHECK: usize = 105;
const ID_BACK: usize = 106;
const ID_NEXT: usize = 107;
const ID_LOG: usize = 108;
const ID_STATUS: usize = 109;
const ID_BODY: usize = 110;
const ID_HINT: usize = 111;
const ID_CLEAN: usize = 112;
const ID_COMP_BASE: usize = 200;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Page {
    Home,
    Options,
    Confirm,
    Working,
    Done,
}

struct App {
    hwnd: HWND,
    page: Page,
    action: Action,
    comps: Vec<Comp>,
    players: Vec<PathBuf>,
    target: Option<PathBuf>,
    state: Installed,
    /// Controls belonging to the current page, destroyed on every page change.
    kids: Vec<HWND>,
    /// (component index, its control, its label or null) for the options page.
    comp_ctl: Vec<(usize, HWND, HWND)>,
    /// Home-page buttons, held by name. Reaching them by position in `kids` was one inserted
    /// control away from moving the wrong window, and would have panicked on the index.
    b_rescan: HWND,
    b_install: HWND,
    b_update: HWND,
    b_uninstall: HWND,
    b_check: HWND,
    b_clean: HWND,
    log: HWND,
    /// Everything the worker has printed, kept OUTSIDE the control.
    ///
    /// Finishing moves Working -> Done, and building a page destroys its predecessor's controls.
    /// Without this the edit box is recreated empty at exactly the moment the user needs to read
    /// it — including, on a failure, the lines saying what went wrong.
    log_text: String,
    body: HWND,
    hint: HWND,
    status: HWND,
    drive: HWND,
    back: HWND,
    next: HWND,
    font: HFONT,
    font_big: HFONT,
    font_bold: HFONT,
    font_mono: HFONT,
    brush_bg: HBRUSH,
    brush_panel: HBRUSH,
    dpi: u32,
    ok: bool,
    /// Walk everything, touch nothing. Set from --dry-run and shown in the header.
    dry: bool,
}

thread_local! {
    static APP: std::cell::RefCell<Option<Box<App>>> = const { std::cell::RefCell::new(None) };
}

/// Lines the worker thread has produced and the window has not yet drawn.
static QUEUE: Mutex<Vec<String>> = Mutex::new(Vec::new());
static BUSY: AtomicBool = AtomicBool::new(false);
static EXIT_CODE: AtomicI32 = AtomicI32::new(0);
/// The release check's answer, waiting to be shown.
static RELEASE_MSG: Mutex<String> = Mutex::new(String::new());
/// Set only when the check found something newer, so the prompt to open a browser is never shown
/// for a version the user already has.
static RELEASE_URL: Mutex<String> = Mutex::new(String::new());
/// The background brush, kept OUTSIDE the app state.
///
/// WM_CTLCOLORSTATIC arrives while a control paints, and a control can paint from inside
/// `CreateWindowExW` — i.e. from inside `build_page`, which already holds the app borrow. Reading
/// the brush out of the app there would be a re-entrant `borrow_mut` and a panic in a message
/// handler, which on Windows means a silently dead window. This handle is written once at startup
/// and only read.
static BG_BRUSH: AtomicUsize = AtomicUsize::new(0);
/// Smallest useful window, in physical pixels, set once the DPI is known. Held outside the app
/// state for the same reason as `BG_BRUSH`: WM_GETMINMAXINFO arrives during window creation.
static MIN_W: AtomicI32 = AtomicI32::new(560);
static MIN_H: AtomicI32 = AtomicI32::new(460);

fn push_log(s: impl Into<String>) {
    if let Ok(mut q) = QUEUE.lock() {
        q.push(s.into());
    }
}

// ── entry ──────────────────────────────────────────────────────────────────────────────────

/// Was this process given a console of its own — i.e. double-clicked rather than run from a shell?
///
/// `GetConsoleProcessList` reports every process attached to this console. A double-clicked
/// console app is alone in one; anything launched from cmd, PowerShell, an SSH session or a script
/// shares the console with its parent. That distinction is the only reliable way for one binary to
/// be both a GUI app and a command-line tool without shipping two of them.
pub fn owns_its_console() -> bool {
    let mut list = [0u32; 4];
    // SAFETY: the buffer and its declared length agree.
    let n = unsafe { GetConsoleProcessList(list.as_mut_ptr(), list.len() as u32) };
    n == 1
}

pub fn run(action: Option<Action>, dry: bool) -> i32 {
    // SAFETY: all three take no arguments or a well-formed local, and are safe to call once at
    // startup before any window exists.
    unsafe {
        FreeConsole();
        SetProcessDPIAware();
        let icc = INITCOMMONCONTROLSEX { dwSize: std::mem::size_of::<INITCOMMONCONTROLSEX>() as u32, dwICC: 0x0000_00FF };
        InitCommonControlsEx(&icc);
    }

    let comps = match crate::CATALOGUE.and_then(|t| crate::catalogue::parse_catalogue(t).ok()) {
        Some(c) => c,
        None => {
            fatal("This build has no component catalogue embedded, so it cannot install anything.\n\ncinder-home/deploy/components.conf was missing when it was compiled.");
            return 2;
        }
    };
    if !crate::MISSING.is_empty() {
        fatal(&format!(
            "This build is incomplete — {} payload file(s) were missing when it was compiled:\n\n{}\n\nDownload a release build rather than one made from a checkout with no dist/.",
            crate::MISSING.len(),
            crate::MISSING.join("\n")
        ));
        return 2;
    }

    // SAFETY: every pointer handed to Win32 below outlives its call; the class name and window
    // title are NUL-terminated locals held for the duration.
    unsafe {
        let inst = GetModuleHandleW(std::ptr::null());
        let class = w("CinderInstallerWindow");
        let cls = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            style: 0x0002 | 0x0001, // CS_HREDRAW | CS_VREDRAW
            lpfnWndProc: Some(wndproc),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: inst,
            hIcon: std::ptr::null_mut(),
            hCursor: LoadCursorW(std::ptr::null_mut(), 32512 as *const u16), // IDC_ARROW
            hbrBackground: std::ptr::null_mut(),
            lpszMenuName: std::ptr::null(),
            lpszClassName: class.as_ptr(),
            hIconSm: std::ptr::null_mut(),
        };
        if RegisterClassExW(&cls) == 0 {
            fatal("Could not register the window class. Run with --console for the text interface.");
            return 2;
        }

        // Read the DPI BEFORE the window exists, because the window's own size depends on it.
        // Every control below is placed through `App::s()`, which multiplies by dpi/96; a window
        // created at a fixed 760x620 physical pixels therefore loses the bottom row of buttons on
        // any display scaled past 100%, which is most laptops.
        let dpi = {
            let dc = GetDC(std::ptr::null_mut());
            let d = GetDeviceCaps(dc, LOGPIXELSX);
            ReleaseDC(std::ptr::null_mut(), dc);
            if d <= 0 { 96u32 } else { d as u32 }
        };
        let sc = |px: i32| px * dpi as i32 / 96;
        let (ww, wh) = (sc(760), sc(620));
        // MEASURED FROM THE LAYOUT, not guessed: the Home page needs the band (84), the drive
        // row and the state line (74), three 72 px cards, two lines of footer and the bottom
        // button row with its padding. At the old 460 the cards, the footer and the buttons were
        // laid out on top of each other; a minimum that cannot show the page is not a minimum.
        MIN_W.store(sc(620), Ordering::SeqCst);
        MIN_H.store(sc(560), Ordering::SeqCst);
        let x = (GetSystemMetrics(SM_CXSCREEN) - ww) / 2;
        let y = ((GetSystemMetrics(SM_CYSCREEN) - wh) / 2).max(0);
        let title = w(&format!(
            "Cinder installer {}{}",
            crate::VERSION,
            if dry { "  —  DRY RUN" } else { "" }
        ));
        let hwnd = CreateWindowExW(
            0,
            class.as_ptr(),
            title.as_ptr(),
            WS_OVERLAPPEDWINDOW | WS_CLIPCHILDREN,
            x,
            y,
            ww,
            wh,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            inst,
            std::ptr::null_mut(),
        );
        if hwnd.is_null() {
            // FreeConsole has already run, so there is nowhere left to print. Without this the
            // program would vanish on a double-click with no window and no message.
            fatal("Could not create the window. Run with --console for the text interface.");
            return 2;
        }

        let mk = |px: i32, weight: i32, face: &str| -> HFONT {
            let f = w(face);
            CreateFontW(-(px * dpi as i32 / 96), 0, 0, 0, weight, 0, 0, 0, 1, 0, 0, 5, 0, f.as_ptr())
        };

        let mut app = Box::new(App {
            hwnd,
            page: Page::Home,
            action: action.unwrap_or(Action::Install),
            comps,
            players: Vec::new(),
            target: None,
            state: Installed::default(),
            kids: Vec::new(),
            comp_ctl: Vec::new(),
            b_rescan: std::ptr::null_mut(),
            b_install: std::ptr::null_mut(),
            b_update: std::ptr::null_mut(),
            b_uninstall: std::ptr::null_mut(),
            b_check: std::ptr::null_mut(),
            b_clean: std::ptr::null_mut(),
            log: std::ptr::null_mut(),
            log_text: String::new(),
            body: std::ptr::null_mut(),
            hint: std::ptr::null_mut(),
            status: std::ptr::null_mut(),
            drive: std::ptr::null_mut(),
            back: std::ptr::null_mut(),
            next: std::ptr::null_mut(),
            font: mk(15, 400, "Segoe UI"),
            font_big: mk(26, 600, "Segoe UI"),
            font_bold: mk(15, 700, "Segoe UI"),
            font_mono: mk(13, 400, "Consolas"),
            brush_bg: CreateSolidBrush(C_BG),
            brush_panel: CreateSolidBrush(C_PANEL),
            dpi,
            ok: false,
            dry,
        });
        app.rescan();
        BG_BRUSH.store(app.brush_bg as usize, Ordering::SeqCst);
        APP.with(|a| *a.borrow_mut() = Some(app));

        with_app(|a| a.build_page());
        ShowWindow(hwnd, SW_SHOW);
        UpdateWindow(hwnd);

        let mut msg: MSG = std::mem::zeroed();
        while GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) > 0 {
            // Without this, Tab does not move between controls and Enter does not press the
            // default button — the two things every Windows user tries first.
            if IsDialogMessageW(hwnd, &mut msg) == 0 {
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
    }
    EXIT_CODE.load(Ordering::SeqCst)
}

fn fatal(msg: &str) {
    let (m, c) = (w(msg), w("Cinder installer"));
    // SAFETY: both strings are NUL-terminated and outlive the call.
    unsafe { MessageBoxW(std::ptr::null_mut(), m.as_ptr(), c.as_ptr(), MB_ICONWARNING) };
}

/// Run `f` against the app state, unless a handler further up the stack already holds it.
///
/// `try_borrow_mut`, not `borrow_mut`: Win32 sends messages re-entrantly (creating a control can
/// dispatch straight back into this window procedure), and a `RefCell` panic inside a message
/// handler leaves a window that no longer responds to anything. Dropping a redundant paint or
/// resize is the right answer — the message that caused it is already being handled.
/// Ask, then delete. Called with no app borrow held — see the WM_COMMAND handler.
fn confirm_and_clean(parent: HWND, target: &std::path::Path, names: &[String]) {
    let text = format!(
        "Delete {} staged file(s) from {}?\n\n{}\n\nThese are the copies the last install left behind, not the Cinder running on the player. Deleting them frees space and changes nothing about the install.\n\nKeep them if you may need to repeat the flash.",
        names.len(),
        target.display(),
        names.join("\n")
    );
    let (t, c) = (w(&text), w("Cinder installer"));
    // SAFETY: both strings are NUL-terminated locals held across the call.
    if unsafe { MessageBoxW(parent, t.as_ptr(), c.as_ptr(), MB_YESNO | MB_ICONWARNING) } != IDYES {
        return;
    }
    let failed: Vec<String> = stage::clean_leftovers(target, names)
        .into_iter()
        .filter_map(|(n, r)| r.err().map(|e| format!("{n}: {e}")))
        .collect();
    if !failed.is_empty() {
        let t = w(&format!("Some files could not be removed:\n\n{}", failed.join("\n")));
        unsafe { MessageBoxW(parent, t.as_ptr(), c.as_ptr(), MB_ICONWARNING) };
    }
}

fn with_app<R>(f: impl FnOnce(&mut App) -> R) -> Option<R> {
    APP.with(|a| match a.try_borrow_mut() {
        Ok(mut b) => b.as_mut().map(|app| f(app)),
        Err(_) => None,
    })
}

// ── window procedure ───────────────────────────────────────────────────────────────────────

unsafe extern "system" fn wndproc(h: HWND, m: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match m {
        WM_ERASEBKGND => 1,
        WM_PAINT => {
            // SAFETY: ps is a live local for the whole Begin/End pair.
            let mut ps: PAINTSTRUCT = std::mem::zeroed();
            let dc = BeginPaint(h, &mut ps);
            with_app(|a| a.paint(dc));
            EndPaint(h, &ps);
            0
        }
        WM_CTLCOLORSTATIC => {
            let dc = wp as HDC;
            SetBkMode(dc, TRANSPARENT);
            let id = GetDlgCtrlID(lp as HWND) as usize;
            SetTextColor(dc, if id == ID_HINT { C_DIM } else { C_TEXT });
            BG_BRUSH.load(Ordering::SeqCst) as LRESULT
        }
        WM_GETMINMAXINFO => {
            // SAFETY: for this message lParam is a MINMAXINFO the caller owns and keeps alive.
            let mmi = &mut *(lp as *mut MINMAXINFO);
            mmi.ptMinTrackSize = [MIN_W.load(Ordering::SeqCst), MIN_H.load(Ordering::SeqCst)];
            0
        }
        WM_SIZE => {
            with_app(|a| a.layout());
            InvalidateRect(h, std::ptr::null(), 1);
            0
        }
        WM_COMMAND => {
            let id = (wp & 0xFFFF) as usize;
            // The cleanup confirm is modal, and a modal dialog runs a NESTED message loop: every
            // WM_PAINT for this window is dispatched while the click that opened it is still on
            // the stack. Showing it from inside `on_command` would mean showing it while the app
            // borrow is held, so those repaints get dropped and the window sits unpainted behind
            // the dialog. Gather under a short borrow, release, then ask.
            if id == ID_CLEAN {
                if let Some((target, names)) = with_app(|a| a.clean_plan()).flatten() {
                    confirm_and_clean(h, &target, &names);
                    with_app(|a| a.after_clean(&target));
                }
                return 0;
            }
            with_app(|a| a.on_command(id));
            0
        }
        WM_APP_LOG => {
            with_app(|a| a.drain_log());
            0
        }
        WM_APP_DONE => {
            with_app(|a| a.on_done(wp == 1));
            0
        }
        WM_APP_RELEASE => {
            let msg = RELEASE_MSG.lock().map(|m| m.clone()).unwrap_or_default();
            let url = RELEASE_URL.lock().map(|m| m.clone()).unwrap_or_default();
            let (t, c) = (w(&msg), w("Cinder installer"));
            let flags = if url.is_empty() { 0 } else { MB_YESNO };
            // SAFETY: both strings are NUL-terminated locals held across the call.
            if MessageBoxW(h, t.as_ptr(), c.as_ptr(), flags) == IDYES && !url.is_empty() {
                let (verb, file) = (w("open"), w(&url));
                ShellExecuteW(h, verb.as_ptr(), file.as_ptr(), std::ptr::null(), std::ptr::null(), SW_SHOW);
            }
            0
        }
        WM_CLOSE => {
            // A flash in flight must not be abandoned by closing the window: the player is being
            // driven by Sony's updater and the payload on it is mid-write.
            if BUSY.load(Ordering::SeqCst) {
                let (t, c) = (
                    w("The player is being written to right now.\n\nClosing during a flash can leave it holding a partial package. Close anyway?"),
                    w("Cinder installer"),
                );
                if MessageBoxW(h, t.as_ptr(), c.as_ptr(), MB_YESNO | MB_ICONWARNING) != IDYES {
                    return 0;
                }
            }
            DestroyWindow(h);
            0
        }
        WM_DESTROY => {
            with_app(|a| a.dispose());
            APP.with(|a| *a.borrow_mut() = None);
            PostQuitMessage(0);
            0
        }
        _ => DefWindowProcW(h, m, wp, lp),
    }
}

// ── the app ────────────────────────────────────────────────────────────────────────────────

impl App {
    fn s(&self, px: i32) -> i32 {
        px * self.dpi as i32 / 96
    }

    fn rescan(&mut self) {
        self.players = device::find_players();
        self.target = self.players.first().cloned();
        self.state = match &self.target {
            Some(t) => device::read_installed(t, &stage::payload_names()),
            None => Installed::default(),
        };
    }

    fn dispose(&mut self) {
        // SAFETY: each handle was created by this struct and is deleted exactly once.
        unsafe {
            for f in [self.font, self.font_big, self.font_bold, self.font_mono] {
                DeleteObject(f);
            }
            DeleteObject(self.brush_bg);
            DeleteObject(self.brush_panel);
        }
    }

    fn mk(&mut self, class: &str, text: &str, style: u32, id: usize, font: HFONT) -> HWND {
        let (c, t) = (w(class), w(text));
        // SAFETY: both strings outlive the call; the parent handle is live.
        let h = unsafe {
            CreateWindowExW(
                0,
                c.as_ptr(),
                t.as_ptr(),
                WS_CHILD | WS_VISIBLE | style,
                0,
                0,
                10,
                10,
                self.hwnd,
                id as HMENU,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            )
        };
        unsafe { SendMessageW(h, WM_SETFONT, font as WPARAM, 1) };
        self.kids.push(h);
        h
    }

    fn clear_page(&mut self) {
        // SAFETY: every handle in `kids` was created by `mk` and is destroyed once.
        for h in self.kids.drain(..) {
            unsafe { DestroyWindow(h) };
        }
        self.comp_ctl.clear();
        for h in [
            &mut self.b_rescan,
            &mut self.b_install,
            &mut self.b_update,
            &mut self.b_uninstall,
            &mut self.b_check,
            &mut self.b_clean,
        ] {
            *h = std::ptr::null_mut();
        }
        self.log = std::ptr::null_mut();
        self.body = std::ptr::null_mut();
        self.hint = std::ptr::null_mut();
        self.status = std::ptr::null_mut();
        self.drive = std::ptr::null_mut();
        self.back = std::ptr::null_mut();
        self.next = std::ptr::null_mut();
    }

    fn go(&mut self, page: Page) {
        self.page = page;
        self.build_page();
        // SAFETY: the window handle is live for the lifetime of this struct.
        unsafe { InvalidateRect(self.hwnd, std::ptr::null(), 1) };
    }

    fn build_page(&mut self) {
        self.clear_page();
        let f = self.font;
        let fb = self.font_bold;
        match self.page {
            Page::Home => {
                self.drive = self.mk("COMBOBOX", "", CBS_DROPDOWNLIST | WS_TABSTOP | WS_VSCROLL, ID_DRIVE, f);
                for p in &self.players.clone() {
                    let t = w(&p.display().to_string());
                    // SAFETY: t outlives the message.
                    unsafe { SendMessageW(self.drive, CB_ADDSTRING, 0, t.as_ptr() as LPARAM) };
                }
                if self.players.is_empty() {
                    let t = w("No Walkman found — plug one in and press Rescan");
                    unsafe { SendMessageW(self.drive, CB_ADDSTRING, 0, t.as_ptr() as LPARAM) };
                }
                unsafe { SendMessageW(self.drive, CB_SETCURSEL, 0, 0) };
                self.b_rescan = self.mk("BUTTON", "Rescan", WS_TABSTOP, ID_RESCAN, f);

                let status = self.state.summary();
                self.status = self.mk("STATIC", &status, SS_NOPREFIX, ID_STATUS, f);

                let have = self.target.is_some();
                let present = self.state.present();
                self.b_install = self.mk("BUTTON", "Install Cinder\nFresh install: choose the optional parts, then flash.", BS_MULTILINE | WS_TABSTOP, ID_INSTALL, fb);
                self.b_update = self.mk("BUTTON", "Update Cinder\nSame components as last time, new build.", BS_MULTILINE | WS_TABSTOP, ID_UPDATE, fb);
                self.b_uninstall = self.mk("BUTTON", "Uninstall\nPut the stock Sony player back.", BS_MULTILINE | WS_TABSTOP, ID_UNINSTALL, fb);
                // SAFETY: all three handles were just created by `mk`.
                unsafe {
                    EnableWindow(self.b_install, i32::from(have));
                    EnableWindow(self.b_update, i32::from(have && present));
                    EnableWindow(self.b_uninstall, i32::from(have));
                }
                self.b_check = self.mk("BUTTON", "Check for a newer release", WS_TABSTOP, ID_CHECK, f);
                if !self.state.leftovers.is_empty() {
                    let n = self.state.leftovers.len();
                    self.b_clean =
                        self.mk("BUTTON", &format!("Clean up {n} staged files"), WS_TABSTOP, ID_CLEAN, f);
                }
                self.hint = self.mk(
                    "STATIC",
                    "The player must be connected by USB in mass-storage mode. Nothing is written until you confirm.",
                    SS_NOPREFIX,
                    ID_HINT,
                    f,
                );
            }
            Page::Options => {
                let title = if self.action == Action::Update {
                    "Update — these are the choices already on the player. Change any of them."
                } else {
                    "Choose the optional parts. Everything not listed here is part of every install."
                };
                self.body = self.mk("STATIC", title, SS_NOPREFIX, ID_BODY, fb);

                for i in 0..self.comps.len() {
                    let c = self.comps[i].clone();
                    let id = ID_COMP_BASE + i;
                    let mut label: HWND = std::ptr::null_mut();
                    let h = match &c.kind {
                        Kind::Bool => {
                            let h = self.mk("BUTTON", &c.title, BS_AUTOCHECKBOX | WS_TABSTOP, id, f);
                            // SAFETY: h is live.
                            unsafe { SendMessageW(h, BM_SETCHECK, usize::from(c.is_on()), 0) };
                            h
                        }
                        Kind::Enum(vals) => {
                            label = self.mk("STATIC", &format!("{}:", c.title), SS_NOPREFIX, id + 1000, f);
                            let h = self.mk("COMBOBOX", "", CBS_DROPDOWNLIST | WS_TABSTOP | WS_VSCROLL, id, f);
                            for v in vals {
                                let t = w(v);
                                unsafe { SendMessageW(h, CB_ADDSTRING, 0, t.as_ptr() as LPARAM) };
                            }
                            let sel = vals.iter().position(|v| *v == c.value).unwrap_or(0);
                            unsafe { SendMessageW(h, CB_SETCURSEL, sel, 0) };
                            h
                        }
                    };
                    self.comp_ctl.push((i, h, label));
                }
                // A READ-ONLY EDIT, NOT A STATIC. The descriptions in components.conf are
                // paragraphs — the `fm` one runs to nine lines at this width — and a STATIC
                // silently clips whatever does not fit its box, with no scrollbar and no
                // ellipsis to say so. Two thirds of what `fm` and `signature` do was
                // unreadable. An EDIT scrolls. It is not a tabstop, so it does not appear in
                // the keyboard order between the checkboxes and the buttons.
                self.hint = self.mk(
                    "EDIT",
                    "Select a component to read what it does.",
                    ES_MULTILINE | ES_READONLY | ES_AUTOVSCROLL | WS_VSCROLL,
                    ID_HINT,
                    f,
                );
                self.back = self.mk("BUTTON", "Back", WS_TABSTOP, ID_BACK, f);
                self.next = self.mk("BUTTON", "Continue", BS_DEFPUSHBUTTON | WS_TABSTOP, ID_NEXT, f);
                self.show_desc(0);
            }
            Page::Confirm => {
                let text = self.confirm_text();
                self.body = self.mk("STATIC", &text, SS_NOPREFIX, ID_BODY, f);
                self.back = self.mk("BUTTON", "Back", WS_TABSTOP, ID_BACK, f);
                let go = if self.dry {
                    "Dry run".to_string()
                } else if self.action.is_removal() {
                    "Uninstall Cinder".to_string()
                } else {
                    self.action.verb().to_string()
                };
                let go = go.as_str();
                self.next = self.mk("BUTTON", go, BS_DEFPUSHBUTTON | WS_TABSTOP, ID_NEXT, fb);
            }
            Page::Working | Page::Done => {
                self.log = self.mk(
                    "EDIT",
                    "",
                    ES_MULTILINE | ES_READONLY | ES_AUTOVSCROLL | WS_VSCROLL | WS_BORDER | WS_TABSTOP,
                    ID_LOG,
                    self.font_mono,
                );
                if !self.log_text.is_empty() {
                    let t = w(&self.log_text);
                    // SAFETY: t outlives the call; the control was created immediately above.
                    unsafe { SetWindowTextW(self.log, t.as_ptr()) };
                }
                self.next = self.mk("BUTTON", "Close", BS_DEFPUSHBUTTON | WS_TABSTOP, ID_NEXT, f);
                // SAFETY: the handle was just created.
                unsafe { EnableWindow(self.next, i32::from(self.page == Page::Done)) };
                if self.page == Page::Done {
                    self.back = self.mk("BUTTON", "Start again", WS_TABSTOP, ID_BACK, f);
                }
            }
        }
        self.layout();
    }

    fn confirm_text(&self) -> String {
        let target = self.target.as_ref().map(|p| p.display().to_string()).unwrap_or_default();
        if self.action.is_removal() {
            let mut s = String::from(
                "UNINSTALL\r\n\r\n\
                 This removes Cinder and puts Sony's own player back. It restores the launch \
                 config from the backup the install made, and deletes Cinder's binaries.\r\n\r\n\
                 Your music, playlists and settings on the data partition are NOT touched.\r\n\r\n",
            );
            s.push_str(&format!("Player:  {target}\r\n"));
            s.push_str(&format!("Status:  {}\r\n\r\n", self.state.summary()));
            if !self.state.present() {
                s.push_str(
                    "This player does not look like it has Cinder on it. Running the uninstall \
                     anyway is harmless — it is a no-op on a stock device.\r\n\r\n",
                );
            }
            s.push_str(
                "The player reboots into Sony's updater, drops off USB while it works, and comes \
                 back on its own. Do not unplug it.",
            );
            return s;
        }

        let plan = stage::plan(self.action, &self.comps, crate::CHANNEL);
        let mut s = format!("{}\r\n\r\nPlayer:  {target}\r\n\r\n", self.action.verb().to_uppercase());
        for c in &self.comps {
            let mark = match c.kind {
                Kind::Bool => if c.is_on() { "on ".to_string() } else { "off".to_string() },
                Kind::Enum(_) => c.value.clone(),
            };
            s.push_str(&format!("    {:<7}  {}\r\n", mark, c.title));
        }
        match plan {
            Ok(p) => s.push_str(&format!(
                "\r\n{} files ({} KB) are copied to the player's storage root.\r\n",
                p.count(),
                p.total_bytes() / 1024
            )),
            Err(e) => s.push_str(&format!("\r\nERROR: {e}\r\n")),
        }
        s.push_str(
            "\r\nNothing is flashed by this program. The player reboots into Sony's updater and \
             applies the package itself, then comes back on its own. Do not unplug it.\r\n\r\n\
             If a boot ever goes wrong: hold the USB cable in at power-on to get the stock player \
             back, and see RECOVERY.md.",
        );
        s
    }

    fn show_desc(&mut self, i: usize) {
        if self.hint.is_null() {
            return;
        }
        let Some(c) = self.comps.get(i) else { return };
        let text = format!("{} — {}\r\n{}", c.title, c.id, c.desc.replace('\n', " ").trim());
        let t = w(&text);
        // SAFETY: t outlives the call and the handle is live.
        unsafe { SetWindowTextW(self.hint, t.as_ptr()) };
    }

    fn read_controls(&mut self) {
        for (i, h, _) in self.comp_ctl.clone() {
            let kind = self.comps[i].kind.clone();
            match kind {
                Kind::Bool => {
                    // SAFETY: h is a live checkbox created on this page.
                    let on = unsafe { SendMessageW(h, BM_GETCHECK, 0, 0) } == 1;
                    self.comps[i].value = if on { "1".into() } else { "0".into() };
                }
                Kind::Enum(vals) => {
                    let sel = unsafe { SendMessageW(h, CB_GETCURSEL, 0, 0) };
                    if sel >= 0 {
                        if let Some(v) = vals.get(sel as usize) {
                            self.comps[i].value = v.clone();
                        }
                    }
                }
            }
        }
    }

    fn on_command(&mut self, id: usize) {
        match id {
            ID_RESCAN => {
                self.rescan();
                self.go(Page::Home);
            }
            ID_DRIVE => {
                // SAFETY: the combo is live while the Home page is up.
                let sel = unsafe { SendMessageW(self.drive, CB_GETCURSEL, 0, 0) };
                if sel >= 0 {
                    if let Some(p) = self.players.get(sel as usize).cloned() {
                        self.target = Some(p.clone());
                        self.state = device::read_installed(&p, &stage::payload_names());
                        self.go(Page::Home);
                    }
                }
            }
            ID_INSTALL | ID_UPDATE | ID_UNINSTALL => {
                if self.target.is_none() {
                    return;
                }
                self.action = match id {
                    ID_INSTALL => Action::Install,
                    ID_UPDATE => Action::Update,
                    _ => Action::Uninstall,
                };
                if self.action == Action::Update {
                    if let Some(text) = self.state.conf.clone() {
                        crate::catalogue::apply_saved(&mut self.comps, &text);
                    }
                }
                self.go(if self.action.is_removal() { Page::Confirm } else { Page::Options });
            }
            ID_CHECK => self.check_release(),
            ID_BACK => match self.page {
                Page::Options | Page::Confirm if self.action.is_removal() => self.go(Page::Home),
                Page::Options => self.go(Page::Home),
                Page::Confirm => self.go(Page::Options),
                Page::Done => {
                    self.rescan();
                    self.go(Page::Home);
                }
                _ => {}
            },
            ID_NEXT => match self.page {
                Page::Options => {
                    self.read_controls();
                    self.go(Page::Confirm);
                }
                Page::Confirm => self.start(),
                Page::Working | Page::Done => {
                    // SAFETY: the window handle is live.
                    unsafe { PostMessageW(self.hwnd, WM_CLOSE, 0, 0) };
                }
                Page::Home => {}
            },
            other if (ID_COMP_BASE..ID_COMP_BASE + self.comps.len()).contains(&other) => {
                self.show_desc(other - ID_COMP_BASE);
            }
            _ => {}
        }
    }

    /// What a cleanup would delete, gathered under the app borrow so the dialog can be shown
    /// after it is released. `None` means there is nothing to do.
    ///
    /// Never automatic. Those files ARE the fallback if a flash has to be repeated, and the
    /// device's own installer calls them "safe to delete once cinder-home is confirmed" —
    /// confirmed means after the player has booted into it, which only the user can judge.
    fn clean_plan(&mut self) -> Option<(PathBuf, Vec<String>)> {
        let target = self.target.clone()?;
        let names = self.state.leftovers.clone();
        if names.is_empty() {
            return None;
        }
        Some((target, names))
    }

    /// Re-read the player and redraw, after a cleanup has run.
    fn after_clean(&mut self, target: &std::path::Path) {
        self.state = device::read_installed(target, &stage::payload_names());
        self.go(Page::Home);
    }

    /// Ask GitHub on a worker thread. The check is explicit, so say where it is going.
    fn check_release(&mut self) {
        let hwnd = self.hwnd as usize;
        std::thread::spawn(move || {
            let (msg, url) = match crate::release::latest() {
                Ok(r) if crate::release::is_newer(&r.tag, crate::VERSION) => (
                    format!(
                        "A newer release is out.\n\nThis installer:  {}\nLatest release:  {} ({})\n\nThis file carries its own copy of Cinder and will install {} no matter what the release page says — to get the new one, download the installer from that release.\n\nOpen the release page now?",
                        crate::VERSION, r.tag, r.name, crate::VERSION
                    ),
                    r.url,
                ),
                Ok(r) => (
                    format!("Up to date.\n\nThis installer:  {}\nLatest release:  {}", crate::VERSION, r.tag),
                    String::new(),
                ),
                Err(e) => (
                    format!("Could not check for updates.\n\n{e}\n\nThis does not affect installing — the payload is built into this file and needs no network."),
                    String::new(),
                ),
            };
            if let Ok(mut m) = RELEASE_MSG.lock() {
                *m = msg;
            }
            if let Ok(mut u) = RELEASE_URL.lock() {
                *u = url;
            }
            // SAFETY: the window outlives the message loop; a post to a destroyed window fails
            // harmlessly rather than executing anything.
            unsafe { PostMessageW(hwnd as HWND, WM_APP_RELEASE, 0, 0) };
        });
    }

    fn start(&mut self) {
        let Some(target) = self.target.clone() else { return };
        let action = self.action;
        let comps = self.comps.clone();
        let dry = self.dry;
        self.go(Page::Working);
        BUSY.store(true, Ordering::SeqCst);
        let hwnd = self.hwnd as usize;

        std::thread::spawn(move || {
            let ok = carry_out(action, &comps, &target, dry, hwnd);
            BUSY.store(false, Ordering::SeqCst);
            EXIT_CODE.store(i32::from(!ok), Ordering::SeqCst);
            // SAFETY: see check_release.
            unsafe { PostMessageW(hwnd as HWND, WM_APP_DONE, usize::from(ok), 0) };
        });
    }

    fn drain_log(&mut self) {
        if self.log.is_null() {
            return;
        }
        let lines: Vec<String> = QUEUE.lock().map(|mut q| q.drain(..).collect()).unwrap_or_default();
        for line in lines {
            self.log_text.push_str(&line);
            self.log_text.push_str("\r\n");
            let t = w(&format!("{line}\r\n"));
            // SAFETY: the edit control is live and t outlives both messages. EM_SETSEL to
            // (-1,-1) puts the caret at the end so EM_REPLACESEL appends instead of overwriting.
            unsafe {
                SendMessageW(self.log, EM_SETSEL, usize::MAX, -1);
                SendMessageW(self.log, EM_REPLACESEL, 0, t.as_ptr() as LPARAM);
            }
        }
    }

    fn on_done(&mut self, ok: bool) {
        self.drain_log();
        self.ok = ok;
        self.page = Page::Done;
        if let Some(t) = self.target.clone() {
            self.state = device::read_installed(&t, &stage::payload_names());
        }
        self.build_page();
        self.drain_log();
        // SAFETY: both handles are live on the Done page.
        unsafe {
            EnableWindow(self.next, 1);
            SetFocus(self.next);
            InvalidateRect(self.hwnd, std::ptr::null(), 1);
        }
    }

    // ── layout ─────────────────────────────────────────────────────────────────────────────

    fn layout(&mut self) {
        let mut rc = RECT::default();
        // SAFETY: the window handle is live and rc is a live local.
        unsafe { GetClientRect(self.hwnd, &mut rc) };
        let (wd, ht) = (rc.right, rc.bottom);
        let pad = self.s(22);
        let head = self.s(84);
        let row = self.s(30);
        let btn_h = self.s(34);

        let mv = |h: HWND, x: i32, y: i32, w_: i32, h_: i32| {
            if !h.is_null() {
                // SAFETY: h was created by `mk` on the current page and has not been destroyed.
                unsafe { MoveWindow(h, x, y, w_, h_, 1) };
            }
        };

        match self.page {
            Page::Home => {
                let mut y = head + pad;
                mv(self.drive, pad, y, wd - pad * 2 - self.s(108), self.s(240));
                mv(self.b_rescan, wd - pad - self.s(100), y, self.s(100), self.s(26));
                y += row + self.s(4);
                mv(self.status, pad, y, wd - pad * 2, row);
                y += row + self.s(10);

                let card = self.s(62);
                for (n, h) in [self.b_install, self.b_update, self.b_uninstall].into_iter().enumerate() {
                    mv(h, pad, y + n as i32 * (card + self.s(10)), wd - pad * 2, card);
                }
                let cards_end = y + 3 * (card + self.s(10));
                let bottom = ht - pad - btn_h;
                mv(self.b_check, pad, bottom, self.s(220), btn_h);
                mv(self.b_clean, pad + self.s(230), bottom, self.s(200), btn_h);
                // The footer sits above the bottom row, but never on top of the Uninstall card:
                // at the old minimum window size it was drawn across it, which put grey body text
                // over a button's own label and made both unreadable.
                let hint_top = (bottom - row * 2 - self.s(8)).max(cards_end + self.s(4));
                mv(self.hint, pad, hint_top, wd - pad * 2, (bottom - hint_top - self.s(4)).max(row));
            }
            Page::Options => {
                let mut y = head + pad;
                mv(self.body, pad, y, wd - pad * 2, row);
                y += row + self.s(6);
                let bottom = ht - pad - btn_h;

                // NOTHING HERE IS PLACED AT A FIXED OFFSET ANY MORE. The rows used to step by a
                // constant 30 px from the top while the description panel sat at a constant
                // offset from the bottom, so on a short window — or with one component more than
                // the catalogue happened to have — the last rows were drawn underneath the
                // description and the buttons, which is not a clipped label but an invisible
                // control the user can still click. The panel takes the slack that is left, the
                // rows take what remains, and the step tightens before anything overlaps.
                let n = self.comp_ctl.len().max(1) as i32;
                let rows_h = n * self.s(30);
                let hint_h = (bottom - self.s(10) - (y + rows_h + self.s(8)))
                    .clamp(self.s(52), self.s(150));
                let hint_top = bottom - hint_h - self.s(10);
                let space = (hint_top - y - self.s(8)).max(self.s(24));
                let step = if rows_h > space { (space / n).max(self.s(22)) } else { self.s(30) };

                for (_, h, label) in self.comp_ctl.clone() {
                    if label.is_null() {
                        mv(h, pad + self.s(4), y, wd - pad * 2 - self.s(8), self.s(24));
                    } else {
                        // An enum row is "Label:  [combo]" — the label takes the left, the combo a
                        // fixed slot on the right so the drop-downs line up down the page.
                        let cw = self.s(150);
                        mv(label, pad + self.s(4), y + self.s(4), wd - pad * 2 - cw - self.s(16), self.s(20));
                        mv(h, wd - pad - cw, y, cw, self.s(240));
                    }
                    y += step;
                }
                mv(self.hint, pad, hint_top, wd - pad * 2, hint_h);
                mv(self.back, pad, bottom, self.s(110), btn_h);
                mv(self.next, wd - pad - self.s(150), bottom, self.s(150), btn_h);
            }
            Page::Confirm => {
                let bottom = ht - pad - btn_h;
                mv(self.body, pad, head + pad, wd - pad * 2, bottom - head - pad * 2);
                mv(self.back, pad, bottom, self.s(110), btn_h);
                mv(self.next, wd - pad - self.s(190), bottom, self.s(190), btn_h);
            }
            Page::Working | Page::Done => {
                let bottom = ht - pad - btn_h;
                // The Done page paints one line under the band — "Done — the player reboots into
                // Cinder", or where it stopped. `paint` drew it from head+4 to head+28 while the
                // log started at head+22, so the sentence that reports the OUTCOME of the whole
                // install was half-covered by the control on top of it. The log starts below it.
                let top = head + pad + if self.page == Page::Done { self.s(26) } else { 0 };
                mv(self.log, pad, top, wd - pad * 2, bottom - top - pad);
                mv(self.back, pad, bottom, self.s(130), btn_h);
                mv(self.next, wd - pad - self.s(130), bottom, self.s(130), btn_h);
            }
        }
    }

    /// The header band, painted rather than built from controls so it can carry the product name
    /// at a size no stock control offers.
    fn paint(&mut self, dc: HDC) {
        let mut rc = RECT::default();
        // SAFETY: dc is the one BeginPaint just returned; every handle below belongs to self.
        unsafe {
            GetClientRect(self.hwnd, &mut rc);
            FillRect(dc, &rc, self.brush_bg);

            let head = self.s(84);
            let band = RECT { left: 0, top: 0, right: rc.right, bottom: head };
            FillRect(dc, &band, self.brush_panel);
            let rule = RECT { left: 0, top: head - self.s(2), right: rc.right, bottom: head };
            let accent = CreateSolidBrush(C_ACCENT);
            FillRect(dc, &rule, accent);
            DeleteObject(accent);

            SetBkMode(dc, TRANSPARENT);
            let pad = self.s(22);

            SelectObject(dc, self.font_big);
            SetTextColor(dc, C_BAND_TEXT);
            let mut r = RECT { left: pad, top: self.s(14), right: rc.right - pad, bottom: self.s(52) };
            let t = w("Cinder");
            DrawTextW(dc, t.as_ptr(), -1, &mut r, DT_LEFT);

            SelectObject(dc, self.font);
            SetTextColor(dc, C_BAND_DIM);
            let mut r = RECT { left: pad, top: self.s(50), right: rc.right - pad, bottom: head - self.s(6) };
            let sub = w("Custom firmware for the Sony NW-A50 series");
            DrawTextW(dc, sub.as_ptr(), -1, &mut r, DT_LEFT | DT_END_ELLIPSIS);

            let stamp = w(&if self.dry {
                format!("{}  ·  {} channel  ·  DRY RUN", crate::VERSION, crate::CHANNEL)
            } else {
                format!("{}   ·   {} channel", crate::VERSION, crate::CHANNEL)
            });
            let mut r = RECT { left: rc.right / 2, top: self.s(52), right: rc.right - pad, bottom: head - self.s(6) };
            SetTextColor(dc, C_ACCENT);
            DrawTextW(dc, stamp.as_ptr(), -1, &mut r, 0x0002 | DT_END_ELLIPSIS); // DT_RIGHT

            // The one line that changes with the page, under the band.
            if self.page == Page::Done {
                SelectObject(dc, self.font_bold);
                SetTextColor(dc, if self.ok { C_ACCENT } else { C_WARN });
                let msg = if self.ok && self.dry {
                    "Dry run finished — the player was not touched."
                } else if self.ok {
                    if self.action.is_removal() {
                        "Done — the player reboots into the stock Sony player."
                    } else {
                        "Done — the player reboots into Cinder."
                    }
                } else {
                    "That did not finish. The log below says where it stopped."
                };
                let t = w(msg);
                let mut r = RECT { left: pad, top: head + self.s(8), right: rc.right - pad, bottom: head + self.s(34) };
                DrawTextW(dc, t.as_ptr(), -1, &mut r, DT_LEFT | DT_END_ELLIPSIS);
            }
        }
    }
}

// ── the worker ─────────────────────────────────────────────────────────────────────────────

/// Runs off the UI thread. Every line it produces goes through `push_log` + `WM_APP_LOG`, so the
/// window shows progress live instead of freezing until the flash is over.
fn carry_out(action: Action, comps: &[Comp], target: &std::path::Path, dry: bool, hwnd: usize) -> bool {
    let tick = || {
        // SAFETY: posting to a window that has gone away fails; it does not execute anything.
        unsafe { PostMessageW(hwnd as HWND, WM_APP_LOG, 0, 0) };
    };

    push_log(format!("{} — {}", action.verb(), target.display()));
    if dry {
        push_log("DRY RUN — nothing is written and the player is not told to flash.");
    }
    push_log(String::new());
    tick();

    let verb = if dry { "would write" } else { "wrote" };
    let r = stage::write_payload(action, comps, crate::CHANNEL, target, dry, |name, n| {
        push_log(format!("  {verb} {name:<24} {n:>9} bytes"));
        tick();
    });
    if let Err(e) = r {
        push_log(String::new());
        push_log(format!("FAILED: {e}"));
        push_log("The player may hold a partial copy. Reconnect it and run this again before");
        push_log("letting it update.");
        tick();
        return false;
    }

    push_log(String::new());
    if dry {
        push_log("DRY RUN complete. Nothing was written; the player was not touched.");
        tick();
        return true;
    }
    push_log("Everything is staged and read back clean.");
    tick();

    push_log(String::new());
    push_log("Telling the player to reboot into its updater. It applies the package and");
    push_log("restarts itself. Do not unplug it.");
    tick();
    match stage::trigger_fw_upgrade(target, |m| {
        push_log(format!("  {m}"));
        tick();
    }) {
        Ok(()) => {
            push_log(String::new());
            push_log("The player has been told to update. Its own updater takes over now.");
            push_log(String::new());
            push_log("If a boot ever goes wrong: hold the USB cable in at power-on to get the");
            push_log("stock player back, and see RECOVERY.md.");
            tick();
            true
        }
        Err(e) => {
            push_log(String::new());
            push_log(format!("FAILED to send the update command: {e}"));
            push_log("The files ARE staged and verified. Nothing on the player was damaged.");
            push_log("Reconnect it and run this again; if it says access was refused, run the");
            push_log("installer as administrator.");
            tick();
            false
        }
    }
}
