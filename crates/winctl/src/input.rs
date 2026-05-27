use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::WindowInfo;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CoordinateSpace {
    ScreenPixels,
    WindowPixels,
    ClientPixels,
    NormalizedWindow,
    NormalizedClient,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ClickRequest {
    pub bound_id: String,
    pub x: f64,
    pub y: f64,
    pub coordinate_space: CoordinateSpace,
    pub button: Option<String>,
    pub fail_if_outside_bound: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct TypeTextRequest {
    pub bound_id: String,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ResolvedPoint {
    pub screen_x: i32,
    pub screen_y: i32,
    pub coordinate_space: CoordinateSpace,
}

#[derive(Debug, Clone, Error)]
pub enum CoordinateResolutionError {
    #[error("coordinate space requires a bound window")]
    WindowRequired,
}

pub fn resolve_screen_point(
    window: Option<&WindowInfo>,
    x: f64,
    y: f64,
    coordinate_space: &CoordinateSpace,
) -> Result<ResolvedPoint, CoordinateResolutionError> {
    let (screen_x, screen_y) = match coordinate_space {
        CoordinateSpace::ScreenPixels => (x.round() as i32, y.round() as i32),
        CoordinateSpace::WindowPixels | CoordinateSpace::ClientPixels => {
            let window = window.ok_or(CoordinateResolutionError::WindowRequired)?;
            (window.x + x.round() as i32, window.y + y.round() as i32)
        }
        CoordinateSpace::NormalizedWindow | CoordinateSpace::NormalizedClient => {
            let window = window.ok_or(CoordinateResolutionError::WindowRequired)?;
            (
                window.x + (x * window.width as f64).round() as i32,
                window.y + (y * window.height as f64).round() as i32,
            )
        }
    };

    Ok(ResolvedPoint {
        screen_x,
        screen_y,
        coordinate_space: coordinate_space.clone(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::WindowInfo;

    fn window() -> WindowInfo {
        WindowInfo {
            id: "hwnd:0x1".into(),
            hwnd: 1,
            hwnd_hex: "0x1".into(),
            pid: 1,
            tid: 1,
            process_name: Some("app.exe".into()),
            exe_path: Some("C:/app.exe".into()),
            title: "App".into(),
            class_name: "App".into(),
            x: 10,
            y: 20,
            width: 200,
            height: 400,
            visible: true,
            enabled: true,
            foreground: false,
            minimized: false,
            cloaked: false,
            top_level: true,
        }
    }

    #[test]
    fn normalized_window_coordinates_resolve_to_screen_pixels() {
        let point = resolve_screen_point(
            Some(&window()),
            0.5,
            0.25,
            &CoordinateSpace::NormalizedWindow,
        )
        .unwrap();

        assert_eq!(point.screen_x, 110);
        assert_eq!(point.screen_y, 120);
    }
}
