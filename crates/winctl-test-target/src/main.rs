#[cfg(not(windows))]
fn main() {
    eprintln!("winctl-test-target is a Windows integration fixture");
}

#[cfg(windows)]
fn main() {
    if let Err(error) = windows_app::run() {
        eprintln!("winctl-test-target failed: {error}");
        std::process::exit(1);
    }
}

#[cfg(windows)]
mod windows_app {
    use std::env;
    use std::ffi::OsStr;
    use std::fs;
    use std::os::windows::ffi::OsStrExt;
    use std::path::PathBuf;
    use std::ptr::null_mut;

    use windows::core::{Error, Result, PCWSTR};
    use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetMessageW,
        PostQuitMessage, RegisterClassW, SetTimer, ShowWindow, TranslateMessage, HMENU, MSG,
        SW_SHOW, WINDOW_EX_STYLE, WM_DESTROY, WM_TIMER, WNDCLASSW, WS_CHILD, WS_OVERLAPPEDWINDOW,
        WS_VISIBLE,
    };

    const TIMER_ID: usize = 1;

    struct Config {
        title: String,
        class_name: String,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
        duration_ms: Option<u32>,
        ready_file: Option<PathBuf>,
        create_child: bool,
        child_title: String,
    }

    pub fn run() -> Result<()> {
        let config = Config::from_args();
        unsafe { run_window(config) }
    }

    unsafe fn run_window(config: Config) -> Result<()> {
        let module = unsafe { GetModuleHandleW(None)? };
        let instance = windows::Win32::Foundation::HINSTANCE(module.0);
        let class_name = wide(&config.class_name);
        let title = wide(&config.title);

        let wnd_class = WNDCLASSW {
            lpfnWndProc: Some(window_proc),
            hInstance: instance,
            lpszClassName: PCWSTR(class_name.as_ptr()),
            ..Default::default()
        };
        unsafe {
            RegisterClassW(&wnd_class);
        }

        let hwnd = unsafe {
            CreateWindowExW(
                WINDOW_EX_STYLE::default(),
                PCWSTR(class_name.as_ptr()),
                PCWSTR(title.as_ptr()),
                WS_OVERLAPPEDWINDOW | WS_VISIBLE,
                config.x,
                config.y,
                config.width,
                config.height,
                None,
                None,
                Some(instance),
                None,
            )?
        };

        if config.create_child {
            create_child_window(hwnd, instance, &config.child_title)?;
        }

        unsafe {
            let _ = ShowWindow(hwnd, SW_SHOW);
        }

        if let Some(duration_ms) = config.duration_ms {
            unsafe {
                SetTimer(Some(hwnd), TIMER_ID, duration_ms, None);
            }
        }

        if let Some(path) = &config.ready_file {
            fs::write(path, format!("hwnd={:#x}\n", hwnd.0 as usize)).map_err(Error::from)?;
        }

        let mut msg = MSG::default();
        while unsafe { GetMessageW(&mut msg, None, 0, 0) }.as_bool() {
            unsafe {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }

        Ok(())
    }

    fn create_child_window(
        parent: HWND,
        instance: windows::Win32::Foundation::HINSTANCE,
        child_title: &str,
    ) -> Result<()> {
        let child_class = wide("STATIC");
        let child_title = wide(child_title);
        unsafe {
            CreateWindowExW(
                WINDOW_EX_STYLE::default(),
                PCWSTR(child_class.as_ptr()),
                PCWSTR(child_title.as_ptr()),
                WS_CHILD | WS_VISIBLE,
                20,
                40,
                220,
                80,
                Some(parent),
                Some(HMENU(null_mut())),
                Some(instance),
                None,
            )?;
        }
        Ok(())
    }

    unsafe extern "system" fn window_proc(
        hwnd: HWND,
        message: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        match message {
            WM_TIMER => {
                unsafe {
                    let _ = DestroyWindow(hwnd);
                }
                LRESULT(0)
            }
            WM_DESTROY => {
                unsafe {
                    PostQuitMessage(0);
                }
                LRESULT(0)
            }
            _ => unsafe { DefWindowProcW(hwnd, message, wparam, lparam) },
        }
    }

    impl Config {
        fn from_args() -> Self {
            let mut config = Self {
                title: "winctl integration target".into(),
                class_name: "WinctlIntegrationTarget".into(),
                x: 80,
                y: 80,
                width: 640,
                height: 420,
                duration_ms: None,
                ready_file: None,
                create_child: false,
                child_title: "winctl integration child".into(),
            };

            let mut args = env::args().skip(1);
            while let Some(arg) = args.next() {
                match arg.as_str() {
                    "--title" => config.title = next_value(&mut args, "--title"),
                    "--class" => config.class_name = next_value(&mut args, "--class"),
                    "--x" => config.x = parse_i32(&mut args, "--x"),
                    "--y" => config.y = parse_i32(&mut args, "--y"),
                    "--width" => config.width = parse_i32(&mut args, "--width"),
                    "--height" => config.height = parse_i32(&mut args, "--height"),
                    "--duration-ms" => {
                        config.duration_ms = Some(parse_u32(&mut args, "--duration-ms"))
                    }
                    "--ready-file" => {
                        config.ready_file =
                            Some(PathBuf::from(next_value(&mut args, "--ready-file")))
                    }
                    "--create-child" => config.create_child = true,
                    "--child-title" => config.child_title = next_value(&mut args, "--child-title"),
                    "--help" | "-h" => {
                        print_help();
                        std::process::exit(0);
                    }
                    other => {
                        eprintln!("unknown argument: {other}");
                        print_help();
                        std::process::exit(2);
                    }
                }
            }

            config
        }
    }

    fn next_value(args: &mut impl Iterator<Item = String>, name: &str) -> String {
        args.next().unwrap_or_else(|| {
            eprintln!("missing value for {name}");
            std::process::exit(2);
        })
    }

    fn parse_i32(args: &mut impl Iterator<Item = String>, name: &str) -> i32 {
        next_value(args, name).parse().unwrap_or_else(|error| {
            eprintln!("invalid {name}: {error}");
            std::process::exit(2);
        })
    }

    fn parse_u32(args: &mut impl Iterator<Item = String>, name: &str) -> u32 {
        next_value(args, name).parse().unwrap_or_else(|error| {
            eprintln!("invalid {name}: {error}");
            std::process::exit(2);
        })
    }

    fn wide(value: &str) -> Vec<u16> {
        OsStr::new(value).encode_wide().chain(Some(0)).collect()
    }

    fn print_help() {
        eprintln!(
            "Usage: winctl-test-target [--title TITLE] [--class CLASS] [--x PX] [--y PX] [--width PX] [--height PX] [--duration-ms MS] [--ready-file PATH] [--create-child]"
        );
    }
}
