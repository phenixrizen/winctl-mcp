use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CaptureRegion {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ScreenshotResult {
    pub output_path: String,
    pub region_virtual_desktop: CaptureRegion,
    pub width: u32,
    pub height: u32,
    pub image_base64: Option<String>,
}
