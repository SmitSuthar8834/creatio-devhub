use crate::applications::parse_applications_json;
use crate::cache::CacheState;
use crate::clio;
use crate::packages::parse_package_json;
use tauri::{AppHandle, Emitter, State};

/// Silently refresh an environment's packages and applications and store them in
/// the catalog cache, so screens are instant and state is captured without the
/// user clicking Refresh. These are read-only clio calls, so it is safe to run
/// in the background; failures are ignored (the cache simply stays as-is).
///
/// Emits `catalog-updated` with the environment name when anything was written,
/// so open screens can reload from the freshened cache.
#[tauri::command]
pub fn prefetch_env_catalog(
    app: AppHandle,
    cache: State<'_, CacheState>,
    env: String,
) -> Result<(), String> {
    let env = env.trim().to_string();
    if env.is_empty() {
        return Ok(());
    }
    let cache = cache.inner().clone();
    std::thread::spawn(move || {
        let mut wrote = false;

        if let Ok((0, out)) = clio::clio_capture(&["list-packages", "-e", &env, "-j"]) {
            if let Ok(packages) = parse_package_json(&out) {
                cache.put("packages", &env, &packages);
                wrote = true;
            }
        }

        if let Ok((0, out)) = clio::clio_capture(&["list-apps", "-e", &env, "--json"]) {
            if let Ok(applications) = parse_applications_json(&out) {
                cache.put("applications", &env, &applications);
                wrote = true;
            }
        }

        if wrote {
            let _ = app.emit("catalog-updated", env);
        }
    });
    Ok(())
}
