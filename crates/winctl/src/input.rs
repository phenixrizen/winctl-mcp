use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

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
