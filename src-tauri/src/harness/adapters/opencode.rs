//! OpenCode: an in-process plugin, named by an environment variable that carries the
//! whole config — including whatever the user already had.

use super::{Adapter, Ctx, GlobalCtx, Plan};
use crate::error::AppResult;
use crate::harness::inherited::inherited_config;

pub(crate) fn harness(id: &str) -> Option<Adapter> {
    if id != "opencode" { return None; }
    Some(Adapter { id: "opencode", executable: "opencode", args: &[], resume: resume_args })
}

fn resume_args(session_id: &str) -> AppResult<Vec<String>> {
    Ok(vec!["--session".into(), super::checked_id(session_id)?.into()])
}

/// OpenCode has no hook files: a session Cue never launched only needs the plugin itself.
pub(crate) fn global(ctx: &GlobalCtx) {
    let _ = ctx.install_plugin("opencode", "harness-opencode.mjs", "opencode-plugin.mjs");
}

pub(super) async fn plan(ctx: &Ctx<'_>) -> AppResult<Plan> {
    let mut plan = Plan::default();
    plan.files.insert("opencode-plugin.mjs".into(), std::fs::read_to_string(ctx.bin_dir.join("harness-opencode.mjs"))?);
    let mut config = inherited_config("opencode", ctx.workspace, &ctx.host.node).await?;
    let plugin = if ctx.workspace.kind == "ssh" {
        format!(
            "file://{}/opencode-plugin.mjs",
            ctx.host.root.to_string_lossy().split('/').map(encoded).collect::<Vec<_>>().join("/")
        )
    } else {
        url::Url::from_file_path(ctx.host.root.join("opencode-plugin.mjs")).map(|u| u.to_string()).unwrap_or_default()
    };
    let mut plugins = config.get("plugin").and_then(|v| v.as_array()).cloned().unwrap_or_default();
    plugins.push(serde_json::Value::String(plugin));
    config.insert("plugin".into(), serde_json::Value::Array(plugins));
    plan.env.insert("OPENCODE_CONFIG_CONTENT".into(), serde_json::Value::Object(config).to_string());
    Ok(plan)
}

fn encoded(value: &str) -> String {
    url::form_urlencoded::byte_serialize(value.as_bytes()).collect()
}
