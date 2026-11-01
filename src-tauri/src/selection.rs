// 唤起窗口前，把别的程序里选中的文字取过来。
//
// Windows 没有"读任意程序选区"的通用接口（UI Automation 只有部分程序支持），
// 所以和 uTools、划词翻译这类工具一样：趁焦点还在原程序里，模拟一次 Ctrl+C，
// 读剪贴板，再把用户原来的剪贴板内容放回去。
//
// 直接声明 user32 / kernel32 的函数，不引 windows crate：
// 用到的就这十几个，而 windows crate 的版本要和 tauri 内部用的对齐，否则会多编一份

// wait_ms：模拟 Ctrl+C 后最多等多久剪贴板有变化，来自 config.toml 的 [selection] wait_ms。
// 没选中文字时，窗口会晚这么久弹出来
#[cfg(windows)]
pub fn grab(wait_ms: u64) -> Option<String> {
    unsafe { imp::grab(std::time::Duration::from_millis(wait_ms)) }
}


#[cfg(not(windows))]
pub fn grab(_wait_ms: u64) -> Option<String> {
    None
}

#[cfg(windows)]
mod imp {
    use std::ffi::c_void;
    use std::ptr::null_mut;
    use std::thread::sleep;
    use std::time::{Duration, Instant};

    type Handle = *mut c_void;

    const CF_UNICODETEXT: u32 = 13;
    const CF_DIB: u32 = 8;
    const CF_HDROP: u32 = 15;
    const GMEM_MOVEABLE: u32 = 0x0002;

    const INPUT_KEYBOARD: u32 = 1;
    const KEYEVENTF_EXTENDEDKEY: u32 = 0x0001;
    const KEYEVENTF_KEYUP: u32 = 0x0002;

    const VK_CONTROL: u16 = 0x11;
    const VK_C: u16 = 0x43;
    const VK_LSHIFT: u16 = 0xA0;
    const VK_RSHIFT: u16 = 0xA1;
    const VK_LMENU: u16 = 0xA4;
    const VK_RMENU: u16 = 0xA5;
    const VK_LWIN: u16 = 0x5B;
    const VK_RWIN: u16 = 0x5C;

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct MouseInput {
        dx: i32,
        dy: i32,
        data: u32,
        flags: u32,
        time: u32,
        extra: usize,
    }

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct KeybdInput {
        vk: u16,
        scan: u16,
        flags: u32,
        time: u32,
        extra: usize,
    }

    // INPUT 里是个联合体，大小按最大的 MOUSEINPUT 算。
    // 只放 KEYBDINPUT 的话结构体偏小，SendInput 会因为 cbSize 不对直接失败
    #[repr(C)]
    union InputUnion {
        mi: MouseInput,
        ki: KeybdInput,
    }

    #[repr(C)]
    struct Input {
        kind: u32,
        u: InputUnion,
    }

    #[link(name = "user32")]
    extern "system" {
        fn GetForegroundWindow() -> Handle;
        fn GetWindowThreadProcessId(hwnd: Handle, pid: *mut u32) -> u32;
        fn GetAsyncKeyState(vk: i32) -> i16;
        fn SendInput(n: u32, inputs: *const Input, size: i32) -> u32;
        fn GetClipboardSequenceNumber() -> u32;
        fn OpenClipboard(owner: Handle) -> i32;
        fn CloseClipboard() -> i32;
        fn EmptyClipboard() -> i32;
        fn IsClipboardFormatAvailable(fmt: u32) -> i32;
        fn GetClipboardData(fmt: u32) -> Handle;
        fn SetClipboardData(fmt: u32, h: Handle) -> Handle;
        fn RegisterClipboardFormatW(name: *const u16) -> u32;
    }

    #[link(name = "kernel32")]
    extern "system" {
        fn GlobalAlloc(flags: u32, bytes: usize) -> Handle;
        fn GlobalFree(h: Handle) -> Handle;
        fn GlobalLock(h: Handle) -> *mut c_void;
        fn GlobalUnlock(h: Handle) -> i32;
        fn GlobalSize(h: Handle) -> usize;
    }

    pub unsafe fn grab(wait: Duration) -> Option<String> {
        // 前台是我们自己（比如固定状态下窗口还开着）就不取，免得把自己输入框里的字复制一遍
        let fg = GetForegroundWindow();
        if fg.is_null() {
            return None;
        }
        let mut pid = 0u32;
        GetWindowThreadProcessId(fg, &mut pid);
        if pid == std::process::id() {
            return None;
        }

        let saved = save_clipboard();
        let before = GetClipboardSequenceNumber();
        send_ctrl_c();

        // 剪贴板序号变了才说明原程序真的复制了东西；
        // 没选中文字时多数程序什么都不做，序号不变，这时绝不能把剪贴板里的旧内容当成选区
        let start = Instant::now();
        while GetClipboardSequenceNumber() == before {
            if start.elapsed() >= wait {
                return None;
            }
            sleep(Duration::from_millis(10));
        }

        // 序号在对方 EmptyClipboard 时就变了，数据可能还没写完，稍等再读；读不到再补一次
        sleep(Duration::from_millis(20));
        let mut text = read_text();
        if text.is_none() {
            sleep(Duration::from_millis(60));
            text = read_text();
        }

        if let Some(saved) = saved {
            restore_clipboard(&saved);
        }

        text.map(|t| t.trim().to_string()).filter(|t| !t.is_empty())
    }

    // 别的程序可能正开着剪贴板，打不开就重试几次
    unsafe fn open_clipboard() -> bool {
        for _ in 0..10 {
            if OpenClipboard(null_mut()) != 0 {
                return true;
            }
            sleep(Duration::from_millis(10));
        }
        false
    }

    fn key(vk: u16, up: bool) -> Input {
        let mut flags = if up { KEYEVENTF_KEYUP } else { 0 };
        if matches!(vk, VK_RMENU | VK_LWIN | VK_RWIN) {
            flags |= KEYEVENTF_EXTENDEDKEY;
        }
        Input {
            kind: INPUT_KEYBOARD,
            u: InputUnion {
                ki: KeybdInput { vk, scan: 0, flags, time: 0, extra: 0 },
            },
        }
    }

    // 热键回调触发时 Alt 通常还按着，直接发 Ctrl+C 对方收到的是 Ctrl+Alt+C。
    // 所以先按下 Ctrl，再把还按着的 Alt/Shift/Win 抬起来，然后 C。
    // 顺序不能反：单独"Alt 抬起"会让传统程序激活菜单栏，先按住 Ctrl 就不会
    unsafe fn send_ctrl_c() {
        let mut seq = vec![key(VK_CONTROL, false)];
        for vk in [VK_LMENU, VK_RMENU, VK_LSHIFT, VK_RSHIFT, VK_LWIN, VK_RWIN] {
            if GetAsyncKeyState(vk as i32) < 0 {
                seq.push(key(vk, true));
            }
        }
        seq.push(key(VK_C, false));
        seq.push(key(VK_C, true));
        seq.push(key(VK_CONTROL, true));
        SendInput(seq.len() as u32, seq.as_ptr(), std::mem::size_of::<Input>() as i32);
    }

    unsafe fn read_text() -> Option<String> {
        if !open_clipboard() {
            return None;
        }
        let mut out = None;
        let h = GetClipboardData(CF_UNICODETEXT);
        if !h.is_null() {
            let p = GlobalLock(h) as *const u16;
            if !p.is_null() {
                let max = GlobalSize(h) / 2;
                let mut len = 0;
                while len < max && *p.add(len) != 0 {
                    len += 1;
                }
                out = Some(String::from_utf16_lossy(std::slice::from_raw_parts(p, len)));
                GlobalUnlock(h);
            }
        }
        CloseClipboard();
        out
    }

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    // 只存常见的几种格式：文字、复制的文件、图片、网页富文本。
    // 不去全量枚举：Excel 这类程序用"延迟渲染"，每个格式都现算，
    // 全存一遍在大表格上可能卡好几秒，而且每次按热键都会付这个代价
    unsafe fn save_clipboard() -> Option<Vec<(u32, Vec<u8>)>> {
        let mut formats = vec![CF_UNICODETEXT, CF_HDROP, CF_DIB];
        for name in ["HTML Format", "Rich Text Format", "PNG"] {
            let f = RegisterClipboardFormatW(wide(name).as_ptr());
            if f != 0 {
                formats.push(f);
            }
        }

        if !open_clipboard() {
            return None; // 读不到原内容就别恢复，否则会把用户的剪贴板清空
        }
        let mut out = Vec::new();
        for f in formats {
            if IsClipboardFormatAvailable(f) == 0 {
                continue;
            }
            let h = GetClipboardData(f);
            if h.is_null() {
                continue;
            }
            let size = GlobalSize(h);
            let p = GlobalLock(h) as *const u8;
            if p.is_null() {
                continue;
            }
            if size > 0 {
                out.push((f, std::slice::from_raw_parts(p, size).to_vec()));
            }
            GlobalUnlock(h);
        }
        CloseClipboard();
        Some(out)
    }

    unsafe fn restore_clipboard(saved: &[(u32, Vec<u8>)]) {
        if !open_clipboard() {
            return;
        }
        EmptyClipboard();
        for (f, data) in saved {
            let h = GlobalAlloc(GMEM_MOVEABLE, data.len());
            if h.is_null() {
                continue;
            }
            let p = GlobalLock(h) as *mut u8;
            if p.is_null() {
                GlobalFree(h);
                continue;
            }
            std::ptr::copy_nonoverlapping(data.as_ptr(), p, data.len());
            GlobalUnlock(h);
            // 成功后内存归系统管；失败了得自己释放
            if SetClipboardData(*f, h).is_null() {
                GlobalFree(h);
            }
        }
        CloseClipboard();
    }
}
