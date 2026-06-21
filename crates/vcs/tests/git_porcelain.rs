use vantadeck_vcs::parse_git_porcelain_v2;

#[test]
fn parses_branch_and_changed_files() {
    let status = parse_git_porcelain_v2(
        "# branch.head feature/ui\n1 .M N... 100644 100644 100644 abc abc Assets/UI.png\n? Notes.txt\n",
    )
    .expect("valid porcelain output");

    assert_eq!(status.branch.as_deref(), Some("feature/ui"));
    assert_eq!(status.changed_files.len(), 2);
    assert_eq!(status.changed_files[0].path, "Assets/UI.png");
}
