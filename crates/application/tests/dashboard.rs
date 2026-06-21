use vantadeck_application::DashboardSnapshot;

#[test]
fn default_dashboard_is_local_and_groups_app_versions() {
    let dashboard = DashboardSnapshot::demo();

    assert!(!dashboard.network_enabled);
    assert!(dashboard.pinned_projects.len() >= 5);
    assert!(dashboard.apps.len() >= 4);
    assert!(
        dashboard
            .apps
            .iter()
            .any(|app| app.name == "Blender" && app.versions.len() > 1)
    );
}
