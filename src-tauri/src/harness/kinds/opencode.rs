//! OpenCode: an in-process plugin, named by an environment variable that carries the
//! whole config — including whatever the user already had.

use super::registry::{checked_id, Adapter, Ctx, GlobalCtx, Harness, Plan};
use super::inherited::inherited_config;
use crate::error::AppResult;
use std::future::Future;
use std::pin::Pin;

fn resume_args(session_id: &str) -> AppResult<Vec<String>> {
    Ok(vec!["--session".into(), checked_id(session_id)?.into()])
}

async fn plan(ctx: Ctx<'_>) -> AppResult<Plan> {
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

pub struct OpenCode;

pub static OPENCODE: OpenCode = OpenCode;

impl Harness for OpenCode {
    fn id(&self) -> &'static str {
        "opencode"
    }
    fn adapter(&self) -> Option<Adapter> {
        Some(Adapter::new("opencode", &[], resume_args))
    }

    fn plan<'a>(&'a self, ctx: Ctx<'a>) -> Pin<Box<dyn Future<Output = AppResult<Plan>> + Send + 'a>> {
        Box::pin(plan(ctx))
    }

    /// OpenCode has no hook files: a session Cue never launched only needs the plugin itself.
    fn global(&self, ctx: &GlobalCtx) {
        let _ = ctx.install_plugin("opencode", "harness-opencode.mjs", "opencode-plugin.mjs");
    }

    fn extra_search_dirs(&self) -> &'static [&'static str] {
        &[".opencode/bin"]
    }

    /// A session name arrives through the stable OSC title alone, so the session files
    /// have nothing to add.
    fn refresh_probe_label(&self) -> bool {
        false
    }

    fn external_ingress(&self) -> bool {
        true
    }
}
