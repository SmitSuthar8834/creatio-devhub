mod applications;
mod cache;
mod catalog;
mod clio;
mod diagnostics;
mod envstate;
mod git;
mod github;
mod jobs;
mod objectmove;
mod packages;
mod refdata;
mod sql;
mod tools;
mod workspaces;

use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .manage(jobs::JobState::default())
        .setup(|app| {
            tools::init(app.handle());
            app.manage(workspaces::WsState::load(app.handle()));
            app.manage(cache::CacheState::load(app.handle()));
            if let Ok(dir) = app.path().app_data_dir() {
                app.state::<jobs::JobState>().init_persistence(dir.join("jobs"));
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            applications::list_applications,
            applications::application_extras,
            applications::application_details,
            applications::deploy_application_between_environments,
            clio::list_environments,
            clio::set_default_environment,
            clio::clio_status,
            clio::install_or_update_clio,
            catalog::prefetch_env_catalog,
            diagnostics::diagnose_error,
            envstate::capture_env_state,
            envstate::list_snapshots,
            envstate::delete_snapshot,
            envstate::diff_environments,
            envstate::export_diff_report,
            github::github_status,
            github::set_git_identity,
            github::start_github_login,
            github::github_login_with_token,
            github::list_github_repos,
            github::list_repo_branches,
            jobs::run_clio_job,
            jobs::get_jobs,
            jobs::get_job_log,
            jobs::cancel_job,
            jobs::clear_job_history,
            packages::list_packages,
            packages::package_lock_states,
            packages::run_package_action,
            packages::deploy_package_between_environments,
            refdata::list_lookups,
            refdata::list_lookup_snapshots,
            refdata::delete_lookup_snapshot,
            refdata::capture_lookups,
            refdata::diff_lookups,
            refdata::build_lookup_migration,
            refdata::migrate_lookups,
            objectmove::list_objects,
            objectmove::object_columns,
            objectmove::object_dependencies,
            objectmove::object_row_count,
            objectmove::build_object_migration,
            objectmove::migrate_object,
            sql::run_sql,
            sql::export_sql,
            tools::tool_paths,
            tools::set_tool_path,
            workspaces::list_workspaces,
            workspaces::register_workspace,
            workspaces::remove_workspace,
            workspaces::create_workspace_flow,
            workspaces::pull_workspace,
            workspaces::add_package_to_workspace,
            workspaces::push_workspace_cloud,
            workspaces::ws_status,
            workspaces::ws_diff,
            workspaces::ws_log,
            workspaces::ws_commit,
            workspaces::ws_set_remote,
            workspaces::ws_remote_status,
            workspaces::ws_push_remote,
            workspaces::create_github_repo,
            workspaces::deploy_from_github,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
