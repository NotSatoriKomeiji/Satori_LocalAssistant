use habitos_core::{assistant_state::Saved, quick_words::Word};
use std::fs;

fn pin(saved: &mut Saved) -> Result<bool, String> {
    saved.quick_words.words.push(Word {
        text: "明天再联系。".into(),
        uses: 2,
        last: 100,
        pinned: true,
    });
    Ok(true)
}

#[test]
fn commits_replace_existing_json_and_survive_restart_without_temp_files() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("assist.json");
    let mut saved = Saved::load(&path).unwrap();
    saved.update(&path, pin).unwrap();
    saved
        .update(&path, |s| {
            s.quick_words.enabled = true;
            s.quick_words.limit = 20;
            Ok(true)
        })
        .unwrap();
    let restarted = Saved::load(&path).unwrap();
    assert!(restarted.quick_words.enabled);
    assert_eq!(restarted.quick_words.limit, 20);
    assert!(restarted
        .quick_words
        .words
        .iter()
        .any(|w| w.text == "明天再联系。"));
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
}

#[test]
fn write_failure_keeps_toggle_limit_addition_deletion_and_learning_unchanged() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("assist.json");
    let mut saved = Saved::default();
    saved.update(&path, pin).unwrap();
    let original = fs::read(&path).unwrap();
    // A file blocks creation of the parent directory. This fails on both OSes,
    // including privileged test environments where read-only mode isn't enough.
    let blocked = path.join("blocked.json");
    for operation in 0..5 {
        assert!(saved
            .update(&blocked, |s| {
                match operation {
                    0 => s.quick_words.enabled = !s.quick_words.enabled,
                    1 => s.quick_words.limit = 1,
                    2 => {
                        pin(s)?;
                    }
                    3 => s.quick_words.dismiss("明天再联系。"),
                    _ => {
                        s.quick_words.learn("明天再见", 200);
                    }
                }
                Ok(true)
            })
            .is_err());
        assert_eq!(
            serde_json::to_value(&saved).unwrap(),
            serde_json::from_slice::<serde_json::Value>(&original).unwrap()
        );
        assert_eq!(fs::read(&path).unwrap(), original);
    }
}

#[test]
fn failed_rename_cleans_staged_file_and_keeps_existing_directory_contents() {
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("blocked.json");
    fs::create_dir(&target).unwrap();
    fs::write(target.join("keep"), "old data").unwrap();
    let mut saved = Saved::default();
    let original = serde_json::to_value(&saved).unwrap();
    assert!(saved.update(&target, pin).is_err());
    assert_eq!(serde_json::to_value(&saved).unwrap(), original);
    assert_eq!(fs::read_to_string(target.join("keep")).unwrap(), "old data");
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
}

#[test]
fn no_op_and_rejected_edit_never_write_or_commit_a_partial_change() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("assist.json");
    let mut saved = Saved::default();
    assert!(!saved.update(&path, |_| Ok(false)).unwrap());
    assert!(saved
        .update(&path, |s| {
            s.quick_words.enabled = true;
            Err("rejected".into())
        })
        .is_err());
    assert!(!saved.quick_words.enabled);
    assert!(!path.exists());
}

#[test]
fn damaged_file_is_preserved_and_legacy_phrases_still_migrate() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("assist.json");
    fs::write(&path, b"{broken").unwrap();
    assert!(Saved::load(&path).is_err());
    assert_eq!(fs::read(&path).unwrap(), b"{broken");
    fs::write(&path, r#"{"phrases":[{"text":"老词库"}]}"#).unwrap();
    let saved = Saved::load(&path).unwrap();
    assert_eq!(saved.quick_words.words[0].text, "老词库");
    assert!(saved.quick_words.words[0].pinned);
}
