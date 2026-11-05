// 시험 — `openguild plugin test examples/plugins/backup-archive`

fn backup(automatic) { #{ path: "/guild/backup.db", automatic: automatic } }

fn test_no_folder_means_nothing_runs() {
    assert_eq(fire("backup.created", backup(false)).commands.len(), 0);
    set_config("ARCHIVE_DIR", "");
    assert_eq(fire("backup.created", backup(false)).commands.len(), 0);
}

fn test_a_backup_is_copied() {
    set_config("ARCHIVE_DIR", "/keep");
    let r = fire("backup.created", backup(true));
    assert_eq(r.commands.len(), 1);
    assert_eq(r.commands[0].action, "archive");
}

fn test_only_manual_skips_automatic_ones() {
    set_config("ARCHIVE_DIR", "/keep");
    set_config("ONLY_MANUAL", true);
    assert_eq(fire("backup.created", backup(true)).commands.len(), 0);
    assert_eq(fire("backup.created", backup(false)).commands.len(), 1);
}
