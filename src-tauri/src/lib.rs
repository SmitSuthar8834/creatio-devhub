mod applications;
mod cache;
mod clio;
mod git;
mod github;
mod jobs;
mod packages;
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
            app.manage(workspaces::WsState::load(app.handle()));
            app.manage(cache::CacheState::load(app.handle()));
            if let Ok(dir) = app.path().app_data_dir() {
                app.state::<jobs::JobState>().init_persistence(dir.join("jobs"));
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            applications::list_applications,
            applications::deploy_application_between_environments,
            clio::list_environments,
            clio::set_default_environment,
            github::github_status,
            github::set_git_identity,
            github::start_github_login,
            jobs::run_clio_job,
            jobs::get_jobs,
            jobs::get_job_log,
            jobs::cancel_job,
            jobs::clear_job_history,
            packages::list_packages,
            packages::run_package_action,
            packages::deploy_package_between_environments,
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
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
