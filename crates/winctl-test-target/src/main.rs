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
    use std::ffi::c_void;
    use std::ffi::OsStr;
    use std::fs;
    use std::os::windows::ffi::OsStrExt;
    use std::path::PathBuf;

    use windows::core::{Error, Result, PCWSTR};
    use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::UI::Controls::{
        InitCommonControlsEx, BST_CHECKED, BST_UNCHECKED, ICC_BAR_CLASSES, INITCOMMONCONTROLSEX,
        TBM_SETPOS, TBM_SETRANGE, TBS_AUTOTICKS,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetDlgItem, GetMessageW,
        PostQuitMessage, RegisterClassW, SendMessageW, SetTimer, SetWindowTextW, ShowWindow,
        TranslateMessage, BM_GETCHECK, BM_SETCHECK, BN_CLICKED, BS_CHECKBOX, BS_PUSHBUTTON,
        CBS_DROPDOWNLIST, CB_ADDSTRING, ES_LEFT, HMENU, LBS_NOTIFY, LB_ADDSTRING, MSG, SW_SHOW,
        WINDOW_EX_STYLE, WINDOW_STYLE, WM_COMMAND, WM_DESTROY, WM_TIMER, WNDCLASSW, WS_BORDER,
        WS_CHILD, WS_OVERLAPPEDWINDOW, WS_TABSTOP, WS_VISIBLE,
    };

    const TIMER_ID: usize = 1;
    const ID_INVOKE_BUTTON: i32 = 101;
    const ID_EDIT_VALUE: i32 = 102;
    const ID_TOGGLE_CHECKBOX: i32 = 103;
    const ID_LISTBOX: i32 = 104;
    const ID_COMBOBOX: i32 = 105;
    const ID_TRACKBAR: i32 = 106;
    const ID_STATUS_TEXT: i32 = 107;

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
        automation_controls: bool,
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
        if config.automation_controls {
            create_automation_controls(hwnd, instance)?;
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

    fn create_automation_controls(
        parent: HWND,
        instance: windows::Win32::Foundation::HINSTANCE,
    ) -> Result<()> {
        let common_controls = INITCOMMONCONTROLSEX {
            dwSize: std::mem::size_of::<INITCOMMONCONTROLSEX>() as u32,
            dwICC: ICC_BAR_CLASSES,
        };
        unsafe {
            let _ = InitCommonControlsEx(&common_controls);
        }

        create_control(
            "BUTTON",
            "Invoke Action",
            button_style(BS_PUSHBUTTON),
            24,
            28,
            150,
            32,
            parent,
            instance,
            ID_INVOKE_BUTTON,
        )?;
        create_control(
            "EDIT",
            "initial edit value",
            window_style(ES_LEFT) | WS_BORDER,
            24,
            76,
            300,
            28,
            parent,
            instance,
            ID_EDIT_VALUE,
        )?;
        create_control(
            "BUTTON",
            "Toggle Choice",
            button_style(BS_CHECKBOX),
            24,
            122,
            180,
            30,
            parent,
            instance,
            ID_TOGGLE_CHECKBOX,
        )?;

        let list = create_control(
            "LISTBOX",
            "",
            window_style(LBS_NOTIFY) | WS_BORDER,
            24,
            172,
            250,
            190,
            parent,
            instance,
            ID_LISTBOX,
        )?;
        for index in 1..=40 {
            let label = format!("Scroll Item {index:02}");
            send_string_message(list, LB_ADDSTRING, &label);
        }
        send_string_message(list, LB_ADDSTRING, "Select Target Item");

        let combo = create_control(
            "COMBOBOX",
            "Expand Collapse Choice",
            window_style(CBS_DROPDOWNLIST),
            360,
            28,
            240,
            180,
            parent,
            instance,
            ID_COMBOBOX,
        )?;
        send_string_message(combo, CB_ADDSTRING, "Combo Option A");
        send_string_message(combo, CB_ADDSTRING, "Combo Option B");
        send_string_message(combo, CB_ADDSTRING, "Combo Option C");

        let trackbar = create_control(
            "msctls_trackbar32",
            "Range Value Slider",
            WINDOW_STYLE(TBS_AUTOTICKS),
            360,
            96,
            260,
            48,
            parent,
            instance,
            ID_TRACKBAR,
        )?;
        let range = ((100_i32 as u32) << 16) as isize;
        unsafe {
            let _ = SendMessageW(trackbar, TBM_SETRANGE, Some(WPARAM(1)), Some(LPARAM(range)));
            let _ = SendMessageW(trackbar, TBM_SETPOS, Some(WPARAM(1)), Some(LPARAM(25)));
        }

        create_control(
            "STATIC",
            "invoke=idle",
            WINDOW_STYLE::default(),
            360,
            172,
            260,
            24,
            parent,
            instance,
            ID_STATUS_TEXT,
        )?;
        create_control(
            "STATIC",
            "Known OCR text: WINCTL OCR FIXTURE 4829",
            WINDOW_STYLE::default(),
            360,
            210,
            360,
            28,
            parent,
            instance,
            0,
        )?;

        Ok(())
    }

    fn create_control(
        class_name: &str,
        title: &str,
        extra_style: WINDOW_STYLE,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
        parent: HWND,
        instance: windows::Win32::Foundation::HINSTANCE,
        id: i32,
    ) -> Result<HWND> {
        let class_name = wide(class_name);
        let title = wide(title);
        unsafe {
            CreateWindowExW(
                WINDOW_EX_STYLE::default(),
                PCWSTR(class_name.as_ptr()),
                PCWSTR(title.as_ptr()),
                WS_CHILD | WS_VISIBLE | WS_TABSTOP | extra_style,
                x,
                y,
                width,
                height,
                Some(parent),
                Some(child_menu(id)),
                Some(instance),
                None,
            )
        }
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
                Some(child_menu(0)),
                Some(instance),
                None,
            )?;
        }
        Ok(())
    }

    fn send_string_message(hwnd: HWND, message: u32, value: &str) {
        let value = wide(value);
        unsafe {
            let _ = SendMessageW(
                hwnd,
                message,
                Some(WPARAM(0)),
                Some(LPARAM(value.as_ptr() as isize)),
            );
        }
    }

    fn child_menu(id: i32) -> HMENU {
        HMENU(id as isize as *mut c_void)
    }

    fn button_style(style: i32) -> WINDOW_STYLE {
        WINDOW_STYLE(style as u32)
    }

    fn window_style(style: i32) -> WINDOW_STYLE {
        WINDOW_STYLE(style as u32)
    }

    unsafe extern "system" fn window_proc(
        hwnd: HWND,
        message: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        match message {
            WM_COMMAND => {
                let control_id = loword(wparam.0);
                let notification = hiword(wparam.0);
                if control_id == ID_INVOKE_BUTTON && notification == BN_CLICKED as u16 {
                    set_status(hwnd, "invoke=done");
                } else if control_id == ID_TOGGLE_CHECKBOX && notification == BN_CLICKED as u16 {
                    toggle_checkbox(hwnd);
                }
                LRESULT(0)
            }
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

    fn toggle_checkbox(parent: HWND) {
        let checkbox = unsafe { GetDlgItem(Some(parent), ID_TOGGLE_CHECKBOX) };
        if let Ok(checkbox) = checkbox {
            let checked = unsafe { SendMessageW(checkbox, BM_GETCHECK, None, None) };
            let next = if checked.0 as u32 == BST_CHECKED.0 {
                BST_UNCHECKED
            } else {
                BST_CHECKED
            };
            unsafe {
                let _ = SendMessageW(checkbox, BM_SETCHECK, Some(WPARAM(next.0 as usize)), None);
            }
            if next == BST_CHECKED {
                set_status(parent, "toggle=on");
            } else {
                set_status(parent, "toggle=off");
            }
        }
    }

    fn set_status(parent: HWND, text: &str) {
        let status = unsafe { GetDlgItem(Some(parent), ID_STATUS_TEXT) };
        if let Ok(status) = status {
            let text = wide(text);
            unsafe {
                let _ = SetWindowTextW(status, PCWSTR(text.as_ptr()));
            }
        }
    }

    fn loword(value: usize) -> i32 {
        (value & 0xffff) as i32
    }

    fn hiword(value: usize) -> u16 {
        ((value >> 16) & 0xffff) as u16
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
                automation_controls: false,
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
                    "--automation-controls" => config.automation_controls = true,
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
            "Usage: winctl-test-target [--title TITLE] [--class CLASS] [--x PX] [--y PX] [--width PX] [--height PX] [--duration-ms MS] [--ready-file PATH] [--create-child] [--automation-controls]"
        );
    }
}
