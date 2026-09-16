//! Pi and OMP: an in-process extension rather than a command hook, registered with
//! `--extension`.

use super::{Adapter, Ctx, GlobalCtx, Plan};
use crate::error::AppResult;

/// OMP is a fork with its own binary and a home-directory allowance.
pub(crate) fn harness(id: &str) -> Option<Adapter> {
    let (id, executable, args): (&'static str, &'static str, &'static [&'static str]) = match id {
        "pi" => ("pi", "pi", &[]),
        "omp" => ("omp", "omp", &["--allow-home"]),
        _ => return None,
    };
    Some(Adapter { id, executable, args, resume: resume_args })
}

fn resume_args(session_id: &str) -> AppResult<Vec<String>> {
    Ok(vec!["--session".into(), super::checked_id(session_id)?.into()])
}

/// Pi is loaded by an extension, so that is all a session Cue never launched needs.
pub(crate) fn global(ctx: &GlobalCtx) {
    let _ = ctx.install_plugin("pi", "harness-pi.mjs", "pi-extension.mjs");
}

pub(super) async fn plan(ctx: &Ctx<'_>) -> AppResult<Plan> {
    let mut plan = Plan::default();
    plan.files.insert("pi-extension.mjs".into(), std::fs::read_to_string(ctx.bin_dir.join("harness-pi.mjs"))?);
    let extension = ctx.host.relative("pi-extension.mjs");
    plan.args.extend(["--extension".into(), extension]);
    Ok(plan)
}
