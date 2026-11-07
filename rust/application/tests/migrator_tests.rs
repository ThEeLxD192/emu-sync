use emusync_application::{CloudFolderMigrator, MigrationError};
use emusync_domain::sync::{CloudFileListing, CloudFolderStore, RemoteFile};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

type RenameRecord = (String, String, Option<String>, Option<String>);

#[derive(Clone, Default)]
struct MockDrive {
    folders: Arc<Mutex<HashMap<String, String>>>, // path -> folder_id
    files: Arc<Mutex<HashMap<String, Vec<RemoteFile>>>>, // folder_id -> files
    move_log: Arc<Mutex<Vec<(String, String, String)>>>, // (file_id, from_parent, to_parent)
    fail_on_move: Arc<Mutex<Option<String>>>,     // file_id that should fail
    rename_log: Arc<Mutex<Vec<RenameRecord>>>,
}

impl MockDrive {
    fn add_folder(&self, path: &[String], id: &str) {
        let key = path.join("/");
        self.folders.lock().unwrap().insert(key, id.to_string());
    }

    fn add_file(&self, folder_id: &str, file: RemoteFile) {
        self.files
            .lock()
            .unwrap()
            .entry(folder_id.to_string())
            .or_default()
            .push(file);
    }
}

impl CloudFileListing for MockDrive {
    type Error = String;

    async fn list_files(&self, _token: &str, folder: &str) -> Result<Vec<RemoteFile>, Self::Error> {
        let lock = self.files.lock().unwrap();
        Ok(lock.get(folder).cloned().unwrap_or_default())
    }

    async fn list_children(
        &self,
        _token: &str,
        folder: &str,
    ) -> Result<Vec<RemoteFile>, Self::Error> {
        let lock = self.files.lock().unwrap();
        Ok(lock.get(folder).cloned().unwrap_or_default())
    }
}

impl CloudFolderStore for MockDrive {
    type Error = String;

    async fn ensure_path(&self, _token: &str, segments: &[String]) -> Result<String, Self::Error> {
        let key = segments.join("/");
        let mut lock = self.folders.lock().unwrap();
        if let Some(id) = lock.get(&key) {
            Ok(id.clone())
        } else {
            let id = format!("id_{key}");
            lock.insert(key, id.clone());
            Ok(id)
        }
    }

    async fn find_path(
        &self,
        _token: &str,
        segments: &[String],
    ) -> Result<Option<String>, Self::Error> {
        let key = segments.join("/");
        let lock = self.folders.lock().unwrap();
        Ok(lock.get(&key).cloned())
    }

    async fn rename_folder(
        &self,
        _token: &str,
        folder: &str,
        name: &str,
        add_parent: Option<&str>,
        remove_parent: Option<&str>,
    ) -> Result<(), Self::Error> {
        self.rename_log.lock().unwrap().push((
            folder.to_string(),
            name.to_string(),
            add_parent.map(String::from),
            remove_parent.map(String::from),
        ));
        let mut lock = self.folders.lock().unwrap();
        // find entry by id and rename key
        let found = lock
            .iter()
            .find(|(_, id)| *id == folder)
            .map(|(k, _)| k.clone());
        if let Some(old_key) = found {
            lock.remove(&old_key);
            lock.insert(name.to_string(), folder.to_string());
        }
        Ok(())
    }

    async fn move_file(
        &self,
        _token: &str,
        file: &str,
        add_parent: &str,
        remove_parent: &str,
    ) -> Result<(), Self::Error> {
        if let Some(fail_id) = self.fail_on_move.lock().unwrap().as_deref()
            && file == fail_id
        {
            return Err(format!("Simulated failure moving {file}"));
        }

        self.move_log.lock().unwrap().push((
            file.to_string(),
            remove_parent.to_string(),
            add_parent.to_string(),
        ));

        let mut lock = self.files.lock().unwrap();
        let mut moved_file = None;
        if let Some(source_list) = lock.get_mut(remove_parent)
            && let Some(idx) = source_list.iter().position(|f| f.id == file)
        {
            moved_file = Some(source_list.remove(idx));
        }
        if let Some(f) = moved_file {
            lock.entry(add_parent.to_string()).or_default().push(f);
        }
        Ok(())
    }
}

fn sample_remote_file(id: &str, name: &str, modified: i64) -> RemoteFile {
    RemoteFile {
        id: id.to_string(),
        name: name.to_string(),
        modified: modified as i128,
        is_folder: false,
        sha256: None,
        version: None,
    }
}

#[tokio::test]
async fn duplicate_remote_files_abort_migration_without_moving() {
    let drive = MockDrive::default();
    let old_path = vec!["OldFolder".to_string()];
    let new_path = vec!["NewFolder".to_string()];

    drive.add_folder(&old_path, "old_id");
    drive.add_folder(&new_path, "new_id");

    // Two files with identical names and different checksums/ids -> duplicate!
    drive.add_file("old_id", sample_remote_file("f1", "save.dat", 1000));
    drive.add_file("old_id", sample_remote_file("f2", "save.dat", 2000));

    let migrator = CloudFolderMigrator::new(drive.clone(), drive.clone());
    let res = migrator.migrate("token", &old_path, &new_path).await;

    assert!(matches!(res, Err(MigrationError::DuplicateNames(_))));
    // No moves performed
    assert!(drive.move_log.lock().unwrap().is_empty());
}

#[tokio::test]
async fn move_failure_rolls_back_in_reverse_order() {
    let drive = MockDrive::default();
    let old_path = vec!["OldFolder".to_string()];
    let new_path = vec!["NewFolder".to_string()];

    drive.add_folder(&old_path, "old_id");
    drive.add_folder(&new_path, "new_id");

    drive.add_file("old_id", sample_remote_file("f1", "save1.dat", 1000));
    drive.add_file("old_id", sample_remote_file("f2", "save2.dat", 2000));
    drive.add_file("old_id", sample_remote_file("f3", "save3.dat", 3000));

    // Make moving f3 fail
    *drive.fail_on_move.lock().unwrap() = Some("f3".to_string());

    let migrator = CloudFolderMigrator::new(drive.clone(), drive.clone());
    let res = migrator.migrate("token", &old_path, &new_path).await;

    assert!(matches!(res, Err(MigrationError::MoveFailed { .. })));

    let log = drive.move_log.lock().unwrap().clone();
    // f1 moved old->new, f2 moved old->new, f3 failed
    // Then rollback in reverse order: f2 moved new->old, f1 moved new->old
    assert_eq!(
        log,
        vec![
            ("f1".into(), "old_id".into(), "new_id".into()),
            ("f2".into(), "old_id".into(), "new_id".into()),
            ("f2".into(), "new_id".into(), "old_id".into()),
            ("f1".into(), "new_id".into(), "old_id".into()),
        ]
    );

    // Files are back in old_id!
    let old_files = drive.files.lock().unwrap().get("old_id").cloned().unwrap();
    let old_names: Vec<_> = old_files.iter().map(|f| f.name.as_str()).collect();
    assert!(old_names.contains(&"save1.dat"));
    assert!(old_names.contains(&"save2.dat"));
}

#[tokio::test]
async fn cooperative_cancellation_triggers_reverse_rollback() {
    let drive = MockDrive::default();
    let old_path = vec!["OldFolder".to_string()];
    let new_path = vec!["NewFolder".to_string()];

    drive.add_folder(&old_path, "old_id");
    drive.add_folder(&new_path, "new_id");

    drive.add_file("old_id", sample_remote_file("f1", "save1.dat", 1000));
    drive.add_file("old_id", sample_remote_file("f2", "save2.dat", 2000));

    let cancel_flag = Arc::new(AtomicBool::new(false));
    let cancel_clone = Arc::clone(&cancel_flag);

    let migrator = CloudFolderMigrator::new(drive.clone(), drive.clone());

    // Cancel after 1 move
    let move_log_clone = Arc::clone(&drive.move_log);
    let cancel_checker = move || {
        if !move_log_clone.lock().unwrap().is_empty() {
            cancel_clone.store(true, Ordering::SeqCst);
        }
        cancel_clone.load(Ordering::SeqCst)
    };

    let res = migrator
        .migrate_with_cancel("token", &old_path, &new_path, cancel_checker)
        .await;

    assert!(matches!(res, Err(MigrationError::Cancelled { .. })));

    let log = drive.move_log.lock().unwrap().clone();
    // f1 moved old->new, then cancelled, f1 rolled back new->old
    assert_eq!(
        log,
        vec![
            ("f1".into(), "old_id".into(), "new_id".into()),
            ("f1".into(), "new_id".into(), "old_id".into()),
        ]
    );
}

#[tokio::test]
async fn relocate_in_same_parent_calls_rename_folder() {
    let drive = MockDrive::default();
    let old_path = vec!["Saves".to_string(), "OldGame".to_string()];
    let new_path = vec!["Saves".to_string(), "NewGame".to_string()];

    drive.add_folder(&old_path, "old_id");

    let migrator = CloudFolderMigrator::new(drive.clone(), drive.clone());
    let res = migrator.migrate("token", &old_path, &new_path).await;
    assert!(res.is_ok());

    let rename_log = drive.rename_log.lock().unwrap().clone();
    assert_eq!(
        rename_log,
        vec![("old_id".to_string(), "NewGame".to_string(), None, None)]
    );
}

#[tokio::test]
async fn relocate_across_different_parents_calls_rename_folder_with_add_and_remove_parents() {
    let drive = MockDrive::default();
    let old_parent = vec!["OldParent".to_string()];
    let old_path = vec!["OldParent".to_string(), "Game".to_string()];
    let new_path = vec!["NewParent".to_string(), "Game".to_string()];

    drive.add_folder(&old_parent, "old_parent_id");
    drive.add_folder(&old_path, "source_id");

    let migrator = CloudFolderMigrator::new(drive.clone(), drive.clone());
    let res = migrator.migrate("token", &old_path, &new_path).await;
    assert!(res.is_ok());

    let rename_log = drive.rename_log.lock().unwrap().clone();
    assert_eq!(
        rename_log,
        vec![(
            "source_id".to_string(),
            "Game".to_string(),
            Some("id_NewParent".to_string()),
            Some("old_parent_id".to_string())
        )]
    );
}

#[tokio::test]
async fn migrate_same_path_or_same_id_returns_ok_immediately() {
    let drive = MockDrive::default();
    let old_path = vec!["Saves".to_string(), "Game".to_string()];

    let migrator = CloudFolderMigrator::new(drive.clone(), drive.clone());

    // 1. Identical path returns Ok without checking drive
    let res = migrator.migrate("token", &old_path, &old_path).await;
    assert!(res.is_ok());
    assert!(drive.folders.lock().unwrap().is_empty());

    // 2. Different path but resolves to same id
    let new_path = vec!["Aliases".to_string(), "Game".to_string()];
    drive.add_folder(&old_path, "shared_id");
    drive.add_folder(&new_path, "shared_id");

    let res2 = migrator.migrate("token", &old_path, &new_path).await;
    assert!(res2.is_ok());
    assert!(drive.rename_log.lock().unwrap().is_empty());
    assert!(drive.move_log.lock().unwrap().is_empty());
}

#[tokio::test]
async fn migrate_non_existent_source_returns_ok_immediately() {
    let drive = MockDrive::default();
    let old_path = vec!["NonExistent".to_string()];
    let new_path = vec!["Target".to_string()];

    let migrator = CloudFolderMigrator::new(drive.clone(), drive.clone());
    let res = migrator.migrate("token", &old_path, &new_path).await;
    assert!(res.is_ok());
    assert!(drive.rename_log.lock().unwrap().is_empty());
}
