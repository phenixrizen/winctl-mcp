use crate::{
    AppState, MacroExportResultRequest, MacroRunRequest, TestManifestRequest, TestRunRequest,
};

pub fn test_validate(request: TestManifestRequest) -> serde_json::Value {
    tracing::info!(title = %request.manifest.title, "test.validate requested");
    let report = winctl_macro::validate_test_manifest(&request.manifest);
    serde_json::json!({
        "ok": true,
        "valid": report.valid,
        "report": report,
        "version": winctl_macro::TEST_MANIFEST_VERSION,
    })
}

pub fn test_dry_run(request: TestManifestRequest) -> serde_json::Value {
    tracing::info!(title = %request.manifest.title, "test.dry_run requested");
    match winctl_macro::dry_run_test_plan(&request.manifest) {
        Ok(plan) => serde_json::json!({"ok": true, "valid": true, "plan": plan}),
        Err(report) => serde_json::json!({"ok": false, "valid": false, "report": report}),
    }
}

pub fn test_run(state: &AppState, request: TestRunRequest) -> serde_json::Value {
    tracing::info!(title = %request.manifest.title, max_steps = ?request.max_steps, "test.run requested");
    let report = winctl_macro::validate_test_manifest(&request.manifest);
    if !report.valid {
        return serde_json::json!({
            "ok": false,
            "error": {
                "code": "test_manifest_validation_failed",
                "message": "test manifest failed validation"
            },
            "report": report,
        });
    }
    crate::tools::macros::macro_run(
        state,
        MacroRunRequest {
            manifest: Some(winctl_macro::test_manifest_to_macro(&request.manifest)),
            memory_id: None,
            max_steps: request.max_steps,
            video: request.video,
        },
    )
}

pub fn test_export_result(
    state: &AppState,
    request: MacroExportResultRequest,
) -> serde_json::Value {
    tracing::info!(run_id = %request.run_id, "test.export_result requested");
    crate::tools::macros::macro_export_result(state, request)
}
