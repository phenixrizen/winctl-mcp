use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct MonitorInfo {
    pub name: String,
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
    pub dpi_scale: f32,
    pub primary: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct VirtualDesktopInfo {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
    pub monitors: Vec<MonitorInfo>,
}

pub fn monitors() -> VirtualDesktopInfo {
    VirtualDesktopInfo {
        x: 0,
        y: 0,
        width: 0,
        height: 0,
        monitors: vec![],
    }
}
