use std::io::Read;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use image::{GenericImageView, ImageBuffer, Rgba};
use winctl::{find_ui_elements, flatten_ui_elements, ui_automation_snapshot};

use crate::{
    AppState, AssertClipboardRequest, AssertElementRequest, AssertPixelColorRequest,
    AssertTextVisibleRequest, AssertWindowCountRequest, CaptureCompareBaselineRequest,
    CaptureOcrRegionRequest, CaptureReadTextRequest,
};

struct OcrOutput {
    provider: &'static str,
    text: String,
    words: Vec<serde_json::Value>,
}

pub fn assert_element(state: &AppState, request: AssertElementRequest) -> serde_json::Value {
    tracing::info!(
        bound_id = %request.bound_id,
        element_ref = ?request.element_ref,
        selector = ?request.selector,
        "assert.element requested"
    );
    let snapshot = match fresh_snapshot(
        state,
        &request.bound_id,
        request.max_depth,
        request.max_elements,
    ) {
        Ok(snapshot) => snapshot,
        Err(error) => return error,
    };
    let matches = if let Some(element_ref) = &request.element_ref {
        flatten_ui_elements(&snapshot.root)
            .into_iter()
            .filter(|element| element.element_ref == *element_ref)
            .collect::<Vec<_>>()
    } else if let Some(selector) = &request.selector {
        find_ui_elements(&snapshot, selector)
    } else {
        return fail(
            "uia_selector_required",
            "element_ref or selector is required",
            None,
        );
    };
    let exists = !matches.is_empty();
    let mut failures = Vec::new();
    if request.exists.unwrap_or(true) != exists {
        failures.push(format!("exists was {exists}"));
    }
    let first = matches.first().copied();
    if let (Some(expected), Some(element)) = (request.enabled, first) {
        if element.enabled != Some(expected) {
            failures.push(format!("enabled was {:?}", element.enabled));
        }
    }
    if let (Some(expected), Some(element)) = (&request.name, first) {
        if element.name.as_deref() != Some(expected.as_str()) {
            failures.push(format!("name was {:?}", element.name));
        }
    }
    if let (Some(needle), Some(element)) = (&request.name_contains, first) {
        if !contains_ci(element.name.as_deref(), needle) {
            failures.push(format!("name did not contain {needle:?}"));
        }
    }
    let passed = failures.is_empty();
    serde_json::json!({
        "ok": true,
        "passed": passed,
        "match_count": matches.len(),
        "matches": matches,
        "failures": failures,
        "snapshot_summary": snapshot_summary(&snapshot),
    })
}

pub fn assert_text_visible(
    state: &AppState,
    request: AssertTextVisibleRequest,
) -> serde_json::Value {
    tracing::info!(
        bound_id = %request.bound_id,
        text = %request.text,
        "assert.text_visible requested"
    );
    let window = match state.revalidate_bound_window(&request.bound_id) {
        Ok(window) => window,
        Err(error) => return serde_json::json!({"ok": false, "error": error}),
    };
    let mut matches = Vec::new();
    if contains_ci(Some(&window.title), &request.text) {
        matches.push(serde_json::json!({"source": "window_title", "text": window.title}));
    }
    if contains_ci(Some(&window.class_name), &request.text) {
        matches.push(serde_json::json!({"source": "window_class", "text": window.class_name}));
    }
    match ui_automation_snapshot(
        &window,
        request.max_depth.unwrap_or(8),
        request.max_elements.unwrap_or(2_000),
    ) {
        Ok(snapshot) => {
            for element in flatten_ui_elements(&snapshot.root) {
                if contains_ci(element.name.as_deref(), &request.text)
                    || contains_ci(element.automation_id.as_deref(), &request.text)
                    || contains_ci(element.class_name.as_deref(), &request.text)
                {
                    matches.push(serde_json::json!({
                        "source": "uia",
                        "element_ref": element.element_ref,
                        "name": element.name,
                        "automation_id": element.automation_id,
                        "class_name": element.class_name,
                    }));
                }
            }
            serde_json::json!({
                "ok": true,
                "passed": !matches.is_empty(),
                "matches": matches,
                "snapshot_summary": snapshot_summary(&snapshot),
            })
        }
        Err(error) => serde_json::json!({
            "ok": true,
            "passed": !matches.is_empty(),
            "matches": matches,
            "warnings": [format!("UI Automation snapshot unavailable: {}", error.message)],
        }),
    }
}

pub fn assert_pixel_color(state: &AppState, request: AssertPixelColorRequest) -> serde_json::Value {
    tracing::info!(
        image_path = ?request.image_path,
        bound_id = ?request.bound_id,
        x = request.x,
        y = request.y,
        expected_rgb = ?request.expected_rgb,
        "assert.pixel_color requested"
    );
    let image_path = match resolve_image_path(state, request.image_path, request.bound_id) {
        Ok(path) => path,
        Err(error) => return error,
    };
    let image = match image::open(&image_path) {
        Ok(image) => image,
        Err(error) => return fail("image_open_failed", &format!("{error}"), Some(image_path)),
    };
    if request.x >= image.width() || request.y >= image.height() {
        return fail(
            "pixel_out_of_bounds",
            "sample coordinate is outside the image",
            Some(image_path),
        );
    }
    let pixel = image.get_pixel(request.x, request.y).0;
    let actual = [pixel[0], pixel[1], pixel[2]];
    let tolerance = request.tolerance.unwrap_or(0);
    let passed = request
        .expected_rgb
        .map(|expected| rgb_within_tolerance(actual, expected, tolerance))
        .unwrap_or(true);
    serde_json::json!({
        "ok": true,
        "passed": passed,
        "image_path": image_path,
        "x": request.x,
        "y": request.y,
        "actual_rgb": actual,
        "expected_rgb": request.expected_rgb,
        "tolerance": tolerance,
    })
}

pub fn assert_window_count(request: AssertWindowCountRequest) -> serde_json::Value {
    tracing::info!(selector = ?request.selector, "assert.window_count requested");
    let windows = winctl::list_windows();
    let matches = winctl::find_windows(&request.selector, &windows);
    let count = matches.len();
    let mut failures = Vec::new();
    if let Some(expected) = request.expected {
        if count != expected {
            failures.push(format!("count {count} did not equal {expected}"));
        }
    }
    if let Some(min) = request.min {
        if count < min {
            failures.push(format!("count {count} was less than {min}"));
        }
    }
    if let Some(max) = request.max {
        if count > max {
            failures.push(format!("count {count} was greater than {max}"));
        }
    }
    serde_json::json!({
        "ok": true,
        "passed": failures.is_empty(),
        "count": count,
        "matches": matches,
        "failures": failures,
    })
}

pub fn assert_clipboard(request: AssertClipboardRequest) -> serde_json::Value {
    tracing::info!("assert.clipboard requested");
    let mut clipboard = match winctl::clipboard_read_text() {
        Ok(clipboard) => clipboard,
        Err(error) => return serde_json::json!({"ok": false, "error": error}),
    };
    if let (Some(text), Some(max_chars)) = (clipboard.text.as_mut(), request.max_chars) {
        if text.chars().count() > max_chars {
            *text = text.chars().take(max_chars).collect();
        }
    }
    let text = clipboard.text.clone().unwrap_or_default();
    let mut failures = Vec::new();
    if let Some(expected) = &request.expected {
        if &text != expected {
            failures.push("clipboard text did not equal expected text".to_owned());
        }
    }
    if let Some(contains) = &request.contains {
        if !text.contains(contains) {
            failures.push("clipboard text did not contain requested text".to_owned());
        }
    }
    serde_json::json!({
        "ok": true,
        "passed": failures.is_empty(),
        "clipboard": clipboard,
        "failures": failures,
    })
}

pub fn capture_ocr_region(state: &AppState, request: CaptureOcrRegionRequest) -> serde_json::Value {
    tracing::info!(
        image_path = ?request.image_path,
        bound_id = ?request.bound_id,
        "capture.ocr_region requested"
    );
    let image_path = match resolve_image_path(state, request.image_path, request.bound_id) {
        Ok(path) => path,
        Err(error) => return error,
    };
    let image = match image::open(&image_path) {
        Ok(image) => image.to_rgba8(),
        Err(error) => return fail("image_open_failed", &format!("{error}"), Some(image_path)),
    };
    let x = request.x.unwrap_or(0);
    let y = request.y.unwrap_or(0);
    if x >= image.width() || y >= image.height() {
        return fail(
            "ocr_region_out_of_bounds",
            "OCR region origin is outside the image",
            Some(image_path),
        );
    }
    let width = request
        .width
        .unwrap_or_else(|| image.width().saturating_sub(x));
    let height = request
        .height
        .unwrap_or_else(|| image.height().saturating_sub(y));
    if width == 0
        || height == 0
        || x.saturating_add(width) > image.width()
        || y.saturating_add(height) > image.height()
    {
        return fail(
            "ocr_region_invalid",
            "OCR region must fit inside the image and have non-zero size",
            Some(image_path),
        );
    }
    let crop = image::imageops::crop_imm(&image, x, y, width, height).to_image();
    let crop_path = state
        .capture_dir
        .join(format!("ocr-region-{}.png", now_unix_ms()));
    if let Err(error) = crop.save(&crop_path) {
        return fail(
            "ocr_region_save_failed",
            &format!("{error}"),
            Some(crop_path),
        );
    }
    let region = serde_json::json!({
        "x": x,
        "y": y,
        "width": width,
        "height": height,
    });

    #[cfg(windows)]
    let mut provider_warnings = match run_windows_media_ocr(&crop_path) {
        Ok(output) => {
            return ocr_success(image_path, crop_path, region, output, Vec::new());
        }
        Err(error) => {
            tracing::warn!(error = %error, "Windows.Media.Ocr provider failed; falling back to tesseract");
            vec![serde_json::json!({
                "provider": "windows_media_ocr",
                "error": error,
            })]
        }
    };
    #[cfg(not(windows))]
    let mut provider_warnings = Vec::new();

    match run_tesseract_tsv(&crop_path) {
        Ok(tsv) => {
            let words = parse_tesseract_tsv(&tsv);
            let text = words
                .iter()
                .filter_map(|word| word.get("text").and_then(|value| value.as_str()))
                .collect::<Vec<_>>()
                .join(" ");
            ocr_success(
                image_path,
                crop_path,
                region,
                OcrOutput {
                    provider: "tesseract",
                    text,
                    words,
                },
                provider_warnings,
            )
        }
        Err(error) => {
            provider_warnings.push(serde_json::json!({
                "provider": "tesseract",
                "error": error,
            }));
            serde_json::json!({
                "ok": false,
                "provider_enabled": false,
                "provider": "none",
                "image_path": image_path,
                "crop_path": crop_path,
                "region": region,
                "error": {
                    "code": "ocr_provider_unavailable",
                    "message": "no OCR provider succeeded",
                    "providers": provider_warnings,
                },
            })
        }
    }
}

pub fn capture_read_text(state: &AppState, request: CaptureReadTextRequest) -> serde_json::Value {
    tracing::info!(
        bound_id = %request.bound_id,
        "capture.read_text requested"
    );
    let snapshot = match fresh_snapshot(
        state,
        &request.bound_id,
        request.max_depth,
        request.max_elements,
    ) {
        Ok(snapshot) => snapshot,
        Err(error) => return error,
    };
    let mut fragments = Vec::new();
    for element in flatten_ui_elements(&snapshot.root) {
        for (field, value) in [
            ("name", element.name.as_deref()),
            ("automation_id", element.automation_id.as_deref()),
            ("class_name", element.class_name.as_deref()),
        ] {
            if let Some(value) = value.filter(|value| !value.trim().is_empty()) {
                fragments.push(serde_json::json!({
                    "source": field,
                    "text": value,
                    "element_ref": element.element_ref,
                }));
            }
        }
    }
    let text = fragments
        .iter()
        .filter_map(|fragment| fragment.get("text").and_then(|value| value.as_str()))
        .collect::<Vec<_>>()
        .join("\n");
    serde_json::json!({
        "ok": true,
        "text": text,
        "fragments": fragments,
        "snapshot_summary": snapshot_summary(&snapshot),
    })
}

pub fn capture_compare_baseline(
    state: &AppState,
    request: CaptureCompareBaselineRequest,
) -> serde_json::Value {
    tracing::info!(
        actual_path = %request.actual_path,
        baseline_path = %request.baseline_path,
        tolerance = ?request.tolerance,
        "capture.compare_baseline requested"
    );
    let actual_path = PathBuf::from(&request.actual_path);
    let baseline_path = PathBuf::from(&request.baseline_path);
    let actual = match image::open(&actual_path) {
        Ok(image) => image.to_rgba8(),
        Err(error) => {
            return fail(
                "actual_image_open_failed",
                &format!("{error}"),
                Some(actual_path),
            )
        }
    };
    let baseline = match image::open(&baseline_path) {
        Ok(image) => image.to_rgba8(),
        Err(error) => {
            return fail(
                "baseline_image_open_failed",
                &format!("{error}"),
                Some(baseline_path),
            )
        }
    };
    if actual.dimensions() != baseline.dimensions() {
        return serde_json::json!({
            "ok": true,
            "passed": false,
            "error": {
                "code": "image_dimensions_differ",
                "message": "actual and baseline dimensions differ"
            },
            "actual_dimensions": {"width": actual.width(), "height": actual.height()},
            "baseline_dimensions": {"width": baseline.width(), "height": baseline.height()},
        });
    }
    let tolerance = request.tolerance.unwrap_or(0);
    let mut diff = ImageBuffer::<Rgba<u8>, Vec<u8>>::new(actual.width(), actual.height());
    let mut different_pixels = 0u64;
    for (x, y, actual_pixel) in actual.enumerate_pixels() {
        let baseline_pixel = baseline.get_pixel(x, y);
        let actual_rgb = [actual_pixel[0], actual_pixel[1], actual_pixel[2]];
        let baseline_rgb = [baseline_pixel[0], baseline_pixel[1], baseline_pixel[2]];
        if rgb_within_tolerance(actual_rgb, baseline_rgb, tolerance) {
            diff.put_pixel(x, y, Rgba([0, 0, 0, 0]));
        } else {
            different_pixels += 1;
            diff.put_pixel(x, y, Rgba([255, 0, 80, 255]));
        }
    }
    let max_different_pixels = request.max_different_pixels.unwrap_or(0);
    let passed = different_pixels <= max_different_pixels;
    let diff_path = request.diff_path.map(PathBuf::from).unwrap_or_else(|| {
        state
            .capture_dir
            .join(format!("baseline-diff-{}.png", now_unix_ms()))
    });
    let diff_save = diff.save(&diff_path).map(|_| diff_path.clone());
    serde_json::json!({
        "ok": true,
        "passed": passed,
        "actual_path": actual_path,
        "baseline_path": baseline_path,
        "diff_path": diff_save.ok(),
        "different_pixels": different_pixels,
        "max_different_pixels": max_different_pixels,
        "tolerance": tolerance,
        "dimensions": {"width": actual.width(), "height": actual.height()},
    })
}

fn fresh_snapshot(
    state: &AppState,
    bound_id: &str,
    max_depth: Option<usize>,
    max_elements: Option<usize>,
) -> Result<winctl::UiAutomationSnapshot, serde_json::Value> {
    let window = match state.revalidate_bound_window(bound_id) {
        Ok(window) => window,
        Err(error) => return Err(serde_json::json!({"ok": false, "error": error})),
    };
    ui_automation_snapshot(
        &window,
        max_depth.unwrap_or(8),
        max_elements.unwrap_or(2_000),
    )
    .map_err(|error| serde_json::json!({"ok": false, "error": error}))
}

fn resolve_image_path(
    state: &AppState,
    image_path: Option<String>,
    bound_id: Option<String>,
) -> Result<PathBuf, serde_json::Value> {
    if let Some(path) = image_path {
        return Ok(PathBuf::from(path));
    }
    let Some(bound_id) = bound_id else {
        return Err(fail(
            "image_or_bound_id_required",
            "image_path or bound_id is required",
            None,
        ));
    };
    let capture = crate::tools::capture::screenshot_window(state, bound_id);
    if !capture
        .get("ok")
        .and_then(|value| value.as_bool())
        .unwrap_or(false)
    {
        return Err(capture);
    }
    capture
        .get("screenshot")
        .and_then(|screenshot| screenshot.get("output_path"))
        .and_then(|value| value.as_str())
        .map(PathBuf::from)
        .ok_or_else(|| {
            fail(
                "screenshot_path_missing",
                "screenshot did not include output_path",
                None,
            )
        })
}

fn snapshot_summary(snapshot: &winctl::UiAutomationSnapshot) -> serde_json::Value {
    serde_json::json!({
        "owner_window": snapshot.owner_window,
        "flattened_count": snapshot.flattened_count,
        "truncated": snapshot.truncated,
        "warnings": snapshot.warnings,
    })
}

fn rgb_within_tolerance(actual: [u8; 3], expected: [u8; 3], tolerance: u8) -> bool {
    actual
        .iter()
        .zip(expected.iter())
        .all(|(actual, expected)| actual.abs_diff(*expected) <= tolerance)
}

fn contains_ci(actual: Option<&str>, expected: &str) -> bool {
    actual
        .map(|actual| {
            actual
                .to_ascii_lowercase()
                .contains(&expected.to_ascii_lowercase())
        })
        .unwrap_or(false)
}

fn ocr_success(
    image_path: PathBuf,
    crop_path: PathBuf,
    region: serde_json::Value,
    output: OcrOutput,
    provider_warnings: Vec<serde_json::Value>,
) -> serde_json::Value {
    let mut response = serde_json::json!({
        "ok": true,
        "provider_enabled": true,
        "provider": output.provider,
        "image_path": image_path,
        "crop_path": crop_path,
        "region": region,
        "text": output.text,
        "words": output.words,
    });
    if !provider_warnings.is_empty() {
        response["provider_warnings"] = serde_json::Value::Array(provider_warnings);
    }
    response
}

#[cfg(windows)]
fn run_windows_media_ocr(image_path: &PathBuf) -> Result<OcrOutput, serde_json::Value> {
    run_windows_media_ocr_inner(image_path).map_err(|error| {
        serde_json::json!({
            "code": "windows_media_ocr_failed",
            "message": error,
        })
    })
}

#[cfg(windows)]
fn run_windows_media_ocr_inner(image_path: &PathBuf) -> Result<OcrOutput, String> {
    use windows::core::HSTRING;
    use windows::Graphics::Imaging::BitmapDecoder;
    use windows::Media::Ocr::OcrEngine;
    use windows::Storage::{FileAccessMode, Streams::FileRandomAccessStream};
    use windows::Win32::System::WinRT::{RoInitialize, RoUninitialize, RO_INIT_MULTITHREADED};

    let ro_initialized = match unsafe { RoInitialize(RO_INIT_MULTITHREADED) } {
        Ok(()) => true,
        Err(error) => {
            tracing::warn!(error = %error, "RoInitialize failed before Windows OCR; trying WinRT activation anyway");
            false
        }
    };
    let result = (|| {
        let max_dimension = OcrEngine::MaxImageDimension().map_err(|error| error.to_string())?;
        let image = image::open(image_path)
            .map_err(|error| format!("failed to inspect OCR image dimensions: {error}"))?;
        if image.width() > max_dimension || image.height() > max_dimension {
            return Err(format!(
                "OCR image {}x{} exceeds Windows.Media.Ocr max dimension {max_dimension}",
                image.width(),
                image.height()
            ));
        }

        let path = HSTRING::from(image_path.as_os_str().to_string_lossy().as_ref());
        let stream = FileRandomAccessStream::OpenAsync(&path, FileAccessMode::Read)
            .map_err(|error| error.to_string())?
            .join()
            .map_err(|error| error.to_string())?;
        let decoder = BitmapDecoder::CreateAsync(&stream)
            .map_err(|error| error.to_string())?
            .join()
            .map_err(|error| error.to_string())?;
        let bitmap = decoder
            .GetSoftwareBitmapAsync()
            .map_err(|error| error.to_string())?
            .join()
            .map_err(|error| error.to_string())?;
        let engine =
            OcrEngine::TryCreateFromUserProfileLanguages().map_err(|error| error.to_string())?;
        let result = engine
            .RecognizeAsync(&bitmap)
            .map_err(|error| error.to_string())?
            .join()
            .map_err(|error| error.to_string())?;
        let text = result
            .Text()
            .map_err(|error| error.to_string())?
            .to_string();
        let lines = result.Lines().map_err(|error| error.to_string())?;
        let mut words = Vec::new();
        for line_index in 0..lines.Size().map_err(|error| error.to_string())? {
            let line = lines.GetAt(line_index).map_err(|error| error.to_string())?;
            let line_words = line.Words().map_err(|error| error.to_string())?;
            for word_index in 0..line_words.Size().map_err(|error| error.to_string())? {
                let word = line_words
                    .GetAt(word_index)
                    .map_err(|error| error.to_string())?;
                let bounds = word.BoundingRect().map_err(|error| error.to_string())?;
                words.push(serde_json::json!({
                    "text": word.Text().map_err(|error| error.to_string())?.to_string(),
                    "confidence": serde_json::Value::Null,
                    "bounds": {
                        "x": bounds.X.max(0.0).round() as u32,
                        "y": bounds.Y.max(0.0).round() as u32,
                        "width": bounds.Width.max(0.0).round() as u32,
                        "height": bounds.Height.max(0.0).round() as u32,
                    },
                    "line": line_index + 1,
                    "word": word_index + 1,
                }));
            }
        }
        Ok(OcrOutput {
            provider: "windows_media_ocr",
            text,
            words,
        })
    })();
    if ro_initialized {
        unsafe { RoUninitialize() };
    }
    result
}

fn run_tesseract_tsv(image_path: &PathBuf) -> Result<String, serde_json::Value> {
    let mut command = Command::new("tesseract");
    command
        .arg(image_path)
        .arg("stdout")
        .arg("--psm")
        .arg("6")
        .arg("tsv")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command.spawn().map_err(|error| {
        serde_json::json!({
            "code": "ocr_provider_unavailable",
            "message": format!("failed to start tesseract OCR provider: {error}"),
            "hint": "install Tesseract OCR or configure a future Windows.Media.Ocr provider",
        })
    })?;
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let stdout_reader = thread::spawn(move || read_pipe(stdout));
    let stderr_reader = thread::spawn(move || read_pipe(stderr));
    let started = Instant::now();
    let timeout = Duration::from_secs(10);
    let mut timed_out = false;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if started.elapsed() >= timeout => {
                timed_out = true;
                let _ = child.kill();
                match child.wait() {
                    Ok(status) => break status,
                    Err(error) => {
                        return Err(serde_json::json!({
                            "code": "ocr_provider_wait_failed",
                            "message": format!("failed waiting for tesseract after timeout: {error}"),
                        }));
                    }
                }
            }
            Ok(None) => thread::sleep(Duration::from_millis(50)),
            Err(error) => {
                return Err(serde_json::json!({
                    "code": "ocr_provider_wait_failed",
                    "message": format!("failed polling tesseract: {error}"),
                }));
            }
        }
    };
    let stdout = stdout_reader.join().unwrap_or_default();
    let stderr = stderr_reader.join().unwrap_or_default();
    let stdout_text = String::from_utf8_lossy(&stdout).to_string();
    let stderr_text = String::from_utf8_lossy(&stderr).to_string();
    if timed_out {
        return Err(serde_json::json!({
            "code": "ocr_provider_timeout",
            "message": "tesseract OCR timed out after 10 seconds",
            "stderr": stderr_text,
        }));
    }
    if !status.success() {
        return Err(serde_json::json!({
            "code": "ocr_provider_failed",
            "message": format!("tesseract exited with status {:?}", status.code()),
            "stderr": stderr_text,
        }));
    }
    Ok(stdout_text)
}

fn read_pipe(pipe: Option<impl Read>) -> Vec<u8> {
    let Some(mut pipe) = pipe else {
        return Vec::new();
    };
    let mut buffer = Vec::new();
    let _ = pipe.read_to_end(&mut buffer);
    buffer
}

fn parse_tesseract_tsv(tsv: &str) -> Vec<serde_json::Value> {
    tsv.lines()
        .skip(1)
        .filter_map(|line| {
            let fields = line.split('\t').collect::<Vec<_>>();
            if fields.len() < 12 {
                return None;
            }
            let text = fields[11..].join("\t").trim().to_owned();
            if text.is_empty() {
                return None;
            }
            let confidence = fields[10].parse::<f64>().ok();
            if confidence.map(|value| value < 0.0).unwrap_or(false) {
                return None;
            }
            Some(serde_json::json!({
                "text": text,
                "confidence": confidence,
                "bounds": {
                    "x": fields[6].parse::<u32>().unwrap_or_default(),
                    "y": fields[7].parse::<u32>().unwrap_or_default(),
                    "width": fields[8].parse::<u32>().unwrap_or_default(),
                    "height": fields[9].parse::<u32>().unwrap_or_default(),
                },
                "line": fields[4].parse::<u32>().ok(),
                "word": fields[5].parse::<u32>().ok(),
            }))
        })
        .collect()
}

fn fail(code: &str, message: &str, path: Option<PathBuf>) -> serde_json::Value {
    serde_json::json!({
        "ok": false,
        "error": {
            "code": code,
            "message": message,
        },
        "path": path,
    })
}

fn now_unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_tesseract_tsv_words() {
        let tsv = "level\tpage_num\tblock_num\tpar_num\tline_num\tword_num\tleft\ttop\twidth\theight\tconf\ttext\n5\t1\t1\t1\t1\t1\t10\t20\t30\t40\t96.5\tHello\n5\t1\t1\t1\t1\t2\t44\t20\t20\t40\t90\tworld\n";

        let words = parse_tesseract_tsv(tsv);

        assert_eq!(words.len(), 2);
        assert_eq!(words[0]["text"], "Hello");
        assert_eq!(words[0]["bounds"]["x"], 10);
        assert_eq!(words[1]["text"], "world");
    }
}
