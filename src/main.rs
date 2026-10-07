#![windows_subsystem = "windows"]
//! Tray app: Win+Shift+S (or left-click tray icon) -> drag a rectangle -> image on clipboard.
//! Esc / right-click cancels. Right-click tray icon -> Exit.

use std::{cell::Cell, mem::zeroed, os::windows::ffi::OsStrExt, ptr::null_mut as null, slice};
use windows_sys::{
    Win32::{
        Foundation::*,
        Graphics::Gdi::*,
        System::{DataExchange::*, LibraryLoader::*, Memory::*, Ole::CF_DIB, Registry::*},
        UI::{HiDpi::*, Input::KeyboardAndMouse::*, Shell::*, WindowsAndMessaging::*},
    },
    core::w,
};

const WM_TRAY: u32 = WM_APP + 1;

/// Frozen screen grab: full-brightness copy + dimmed copy, both top-down 32bpp DIBs.
#[derive(Clone, Copy)]
struct Shot {
    bright: HDC,
    dim: HDC,
    bmps: [HBITMAP; 2],
    bits: *const u8,
    w: i32,
    h: i32,
}

thread_local! {
    static SHOT: Cell<Option<Shot>> = const { Cell::new(None) };
    static DRAG: Cell<Option<(i32, i32, i32, i32)>> = const { Cell::new(None) };
}

fn main() {
    unsafe {
        SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        let inst = GetModuleHandleW(null());
        let class = w!("snip");
        let wc = WNDCLASSW {
            lpfnWndProc: Some(wndproc),
            hInstance: inst,
            lpszClassName: class,
            hCursor: LoadCursorW(null(), IDC_CROSS),
            ..zeroed()
        };
        if !FindWindowW(class, null()).is_null() {
            return; // already running
        }
        RegisterClassW(&wc);
        // One window does everything: hidden while idle, fullscreen overlay while selecting.
        let hwnd = CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_TOOLWINDOW,
            class, class, WS_POPUP, 0, 0, 0, 0, null(), null(), inst, null(),
        );
        // Win+Shift+S belongs to Windows' Snipping Tool, so RegisterHotKey can't have it;
        // a low-level hook sees the keys first and swallows them.
        SetWindowsHookExW(WH_KEYBOARD_LL, Some(kbd), inst, 0);

        let mut nid = NOTIFYICONDATAW {
            cbSize: size_of::<NOTIFYICONDATAW>() as u32,
            hWnd: hwnd,
            uID: 1,
            uFlags: NIF_ICON | NIF_MESSAGE | NIF_TIP,
            uCallbackMessage: WM_TRAY,
            // Embedded icon (assets/snip.rc), loaded at small-icon size so the tray stays crisp.
            hIcon: LoadImageW(
                inst, 1 as _, IMAGE_ICON,
                GetSystemMetrics(SM_CXSMICON), GetSystemMetrics(SM_CYSMICON), 0,
            ),
            ..zeroed()
        };
        for (d, s) in nid.szTip.iter_mut().zip("Snip (Win+Shift+S)".encode_utf16()) {
            *d = s;
        }
        Shell_NotifyIconW(NIM_ADD, &nid);

        // Run at login. Disable via Task Manager > Startup apps.
        if let Ok(exe) = std::env::current_exe() {
            let v: Vec<u16> = exe.as_os_str().encode_wide().chain([0]).collect();
            RegSetKeyValueW(
                HKEY_CURRENT_USER, w!("Software\\Microsoft\\Windows\\CurrentVersion\\Run"),
                w!("snip"), REG_SZ, v.as_ptr().cast(), (v.len() * 2) as u32,
            );
        }

        let mut msg = zeroed();
        while GetMessageW(&mut msg, null(), 0, 0) > 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        Shell_NotifyIconW(NIM_DELETE, &nid);
    }
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    unsafe {
        match msg {
            WM_HOTKEY => start(hwnd),
            WM_TRAY => match lp as u32 {
                WM_LBUTTONUP => start(hwnd),
                WM_RBUTTONUP => menu(hwnd),
                _ => {}
            },
            WM_LBUTTONDOWN => {
                let (x, y) = xy(lp);
                SetCapture(hwnd);
                DRAG.set(Some((x, y, x, y)));
            }
            WM_MOUSEMOVE => {
                if let Some((x0, y0, ..)) = DRAG.get() {
                    let (x, y) = xy(lp);
                    DRAG.set(Some((x0, y0, x, y)));
                    InvalidateRect(hwnd, null(), 0);
                }
            }
            WM_LBUTTONUP => {
                if let (Some(d), Some(s)) = (DRAG.get(), SHOT.get()) {
                    let r = norm(d, s.w, s.h);
                    if r.right > r.left && r.bottom > r.top {
                        copy(hwnd, s, r);
                    }
                }
                stop(hwnd);
            }
            WM_RBUTTONUP => stop(hwnd),
            WM_KEYDOWN if wp == VK_ESCAPE as usize => stop(hwnd),
            WM_ACTIVATE if wp & 0xffff == WA_INACTIVE as usize => stop(hwnd),
            WM_PAINT => paint(hwnd),
            WM_ERASEBKGND => return 1,
            _ => return DefWindowProcW(hwnd, msg, wp, lp),
        }
        0
    }
}

/// Win+Shift+S starts a capture; Esc cancels one even if the overlay didn't get keyboard focus.
unsafe extern "system" fn kbd(code: i32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    unsafe {
        let vk = (*(lp as *const KBDLLHOOKSTRUCT)).vkCode;
        let down = wp == WM_KEYDOWN as usize || wp == WM_SYSKEYDOWN as usize;
        let held = |k: VIRTUAL_KEY| GetAsyncKeyState(k as i32) < 0;
        let hwnd = FindWindowW(w!("snip"), null());
        if code >= 0 && vk == 'S' as u32 && held(VK_SHIFT) && (held(VK_LWIN) || held(VK_RWIN)) {
            if down {
                // Tap an unassigned key so releasing Win doesn't open the Start menu.
                keybd_event(0xE8, 0, 0, 0);
                keybd_event(0xE8, 0, KEYEVENTF_KEYUP, 0);
                PostMessageW(hwnd, WM_HOTKEY, 0, 0);
            }
            return 1;
        }
        if code >= 0 && vk == VK_ESCAPE as u32 && SHOT.get().is_some() {
            if down {
                PostMessageW(hwnd, WM_KEYDOWN, VK_ESCAPE as usize, 0);
            }
            return 1;
        }
        CallNextHookEx(null(), code, wp, lp)
    }
}

unsafe fn start(hwnd: HWND) {
    unsafe {
        if SHOT.get().is_some() {
            return;
        }
        let (x, y) = (GetSystemMetrics(SM_XVIRTUALSCREEN), GetSystemMetrics(SM_YVIRTUALSCREEN));
        let (w, h) = (GetSystemMetrics(SM_CXVIRTUALSCREEN), GetSystemMetrics(SM_CYVIRTUALSCREEN));
        let (bright, b1, bits) = dib(w, h);
        let (dim, b2, dbits) = dib(w, h);
        let scr = GetDC(null());
        BitBlt(bright, 0, 0, w, h, scr, x, y, SRCCOPY | CAPTUREBLT);
        ReleaseDC(null(), scr);
        GdiFlush();
        let n = (w * h * 4) as usize;
        for (d, s) in slice::from_raw_parts_mut(dbits, n).iter_mut().zip(slice::from_raw_parts(bits, n)) {
            *d = s >> 1;
        }
        SHOT.set(Some(Shot { bright, dim, bmps: [b1, b2], bits, w, h }));
        SetWindowPos(hwnd, HWND_TOPMOST, x, y, w, h, SWP_SHOWWINDOW);
        SetForegroundWindow(hwnd);
    }
}

unsafe fn stop(hwnd: HWND) {
    unsafe {
        DRAG.set(None);
        ReleaseCapture();
        if let Some(s) = SHOT.take() {
            ShowWindow(hwnd, SW_HIDE);
            DeleteDC(s.bright);
            DeleteDC(s.dim);
            s.bmps.iter().for_each(|&b| _ = DeleteObject(b));
        }
    }
}

unsafe fn paint(hwnd: HWND) {
    unsafe {
        let mut ps = zeroed();
        let dc = BeginPaint(hwnd, &mut ps);
        if let Some(s) = SHOT.get() {
            // Bright selection, then dim everything else; each pixel drawn once, no flicker.
            if let Some(d) = DRAG.get() {
                let r = norm(d, s.w, s.h);
                BitBlt(dc, r.left, r.top, r.right - r.left, r.bottom - r.top, s.bright, r.left, r.top, SRCCOPY);
                FrameRect(dc, &r, GetStockObject(WHITE_BRUSH));
                ExcludeClipRect(dc, r.left, r.top, r.right, r.bottom);
            }
            BitBlt(dc, 0, 0, s.w, s.h, s.dim, 0, 0, SRCCOPY);
        }
        EndPaint(hwnd, &ps);
    }
}

unsafe fn menu(hwnd: HWND) {
    unsafe {
        let m = CreatePopupMenu();
        AppendMenuW(m, MF_STRING, 1, w!("Exit"));
        let mut pt = zeroed();
        GetCursorPos(&mut pt);
        SetForegroundWindow(hwnd); // so the menu closes when clicking elsewhere
        if TrackPopupMenu(m, TPM_RETURNCMD | TPM_NONOTIFY, pt.x, pt.y, 0, hwnd, null()) == 1 {
            PostQuitMessage(0);
        }
        DestroyMenu(m);
    }
}

unsafe fn dib(w: i32, h: i32) -> (HDC, HBITMAP, *mut u8) {
    unsafe {
        let bi = BITMAPINFO { bmiHeader: header(w, -h), ..zeroed() };
        let mut bits = null();
        let bmp = CreateDIBSection(null(), &bi, DIB_RGB_COLORS, &mut bits, null(), 0);
        let dc = CreateCompatibleDC(null());
        SelectObject(dc, bmp);
        (dc, bmp, bits.cast())
    }
}

/// Puts the selected region on the clipboard as CF_DIB.
unsafe fn copy(hwnd: HWND, s: Shot, r: RECT) {
    unsafe {
        let src = slice::from_raw_parts(s.bits, (s.w * s.h * 4) as usize);
        let px = crop(src, s.w, r);
        let hdr = header(r.right - r.left, r.bottom - r.top);
        let hsz = size_of::<BITMAPINFOHEADER>();
        let mem = GlobalAlloc(GMEM_MOVEABLE, hsz + px.len());
        if mem.is_null() {
            return;
        }
        let p = GlobalLock(mem).cast::<u8>();
        p.cast::<BITMAPINFOHEADER>().write_unaligned(hdr);
        p.add(hsz).copy_from_nonoverlapping(px.as_ptr(), px.len());
        GlobalUnlock(mem);
        // Another app (clipboard history, RDP) may hold the clipboard for a moment.
        if !(0..10).any(|_| OpenClipboard(hwnd) != 0 || { std::thread::sleep(std::time::Duration::from_millis(20)); false }) {
            GlobalFree(mem);
            return;
        }
        EmptyClipboard();
        if SetClipboardData(CF_DIB as u32, mem).is_null() {
            GlobalFree(mem);
        }
        CloseClipboard();
    }
}

fn header(w: i32, h: i32) -> BITMAPINFOHEADER {
    BITMAPINFOHEADER {
        biSize: size_of::<BITMAPINFOHEADER>() as u32,
        biWidth: w,
        biHeight: h,
        biPlanes: 1,
        biBitCount: 32,
        ..unsafe { zeroed() }
    }
}

/// Crops a top-down BGRA buffer to `r`, returning bottom-up rows (DIB order) with opaque alpha.
fn crop(src: &[u8], src_w: i32, r: RECT) -> Vec<u8> {
    let row = ((r.right - r.left) * 4) as usize;
    let mut out = Vec::with_capacity(row * (r.bottom - r.top) as usize);
    for y in (r.top..r.bottom).rev() {
        let off = ((y * src_w + r.left) * 4) as usize;
        out.extend_from_slice(&src[off..off + row]);
    }
    out.chunks_exact_mut(4).for_each(|p| p[3] = 255);
    out
}

/// Drag endpoints -> rect clamped to the capture.
fn norm((x0, y0, x1, y1): (i32, i32, i32, i32), w: i32, h: i32) -> RECT {
    RECT {
        left: x0.min(x1).clamp(0, w),
        top: y0.min(y1).clamp(0, h),
        right: x0.max(x1).clamp(0, w),
        bottom: y0.max(y1).clamp(0, h),
    }
}

fn xy(lp: LPARAM) -> (i32, i32) {
    ((lp & 0xffff) as i16 as i32, ((lp >> 16) & 0xffff) as i16 as i32)
}

#[test]
fn crop_flips_and_clamps() {
    // 3x2 image, pixel value = index.
    let src: Vec<u8> = (0..6u8).flat_map(|i| [i, i, i, 0]).collect();
    let r = norm((3, 2, 1, -5), 3, 2); // reversed drag, out of bounds
    assert_eq!((r.left, r.top, r.right, r.bottom), (1, 0, 3, 2));
    assert_eq!(crop(&src, 3, r), [4, 4, 4, 255, 5, 5, 5, 255, 1, 1, 1, 255, 2, 2, 2, 255]);
}
