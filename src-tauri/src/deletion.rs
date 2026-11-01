use super::*;
use crate::management::ManagementSnapshot;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteResult {
    pub snapshot: LibrarySnapshot,
    pub management: ManagementSnapshot,
    pub deleted: usize,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct DeleteJournal {
    manifest: Manifest,
    management: Option<ManagementSnapshot>,
    files: Vec<String>,
}
fn atomic_json(path: &Path, value: &impl Serialize) -> Result<(), String> {
    if fs::symlink_metadata(path).is_ok() {
        regular_file(path)?;
    }
    let mut temp = tempfile::NamedTempFile::new_in(path.parent().unwrap()).map_err(err)?;
    serde_json::to_writer_pretty(&mut temp, value).map_err(err)?;
    temp.as_file().sync_all().map_err(err)?;
    temp.persist(path).map_err(err)?;
    Ok(())
}
impl Library {
    // The journal survives a crash until the entire staging directory reaches
    // system Trash. Opening an interrupted library restores its original state.
    pub(crate) fn recover_deletion(root: &Path) -> Result<(), String> {
        let stage = root.join("StickerNest Deleted Assets");
        match fs::symlink_metadata(&stage) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Ok(meta) if meta.file_type().is_dir() => (),
            _ => return Err("删除事务目录异常，请恢复备份".into()),
        }
        if fs::symlink_metadata(stage.join("journal.json")).is_err() {
            fs::remove_dir(&stage).map_err(|e| format!("无法清理未开始的删除事务：{e}"))?;
            return Ok(());
        }
        regular_file(&stage.join("journal.json"))?;
        let journal: DeleteJournal = serde_json::from_reader(
            File::open(stage.join("journal.json"))
                .map_err(err)?
                .take(MAX_BYTES * 2 + 1),
        )
        .map_err(|e| format!("删除事务记录损坏：{e}"))?;
        Self::check_assets(root)?;
        for name in &journal.files {
            let relative = Path::new(name);
            let parts: Vec<_> = relative.components().collect();
            let valid = (parts.len() == 2 && parts[0].as_os_str() == "assets")
                || (parts.len() == 3
                    && parts[0].as_os_str() == "cache"
                    && parts[1].as_os_str() == "wechat");
            let filename = relative.file_name().unwrap_or_default().to_string_lossy();
            if !valid
                || !parts
                    .iter()
                    .all(|p| matches!(p, std::path::Component::Normal(_)))
                || !journal.manifest.items.iter().any(|i| {
                    filename == i.file_name
                        || filename == format!("{}.png", i.id)
                        || filename == format!("{}.gif", i.id)
                })
            {
                return Err("删除事务含不安全的文件路径".into());
            }
            let mut staged_parent = stage.clone();
            for part in &parts[..parts.len() - 1] {
                staged_parent.push(part.as_os_str());
                match fs::symlink_metadata(&staged_parent) {
                    Ok(meta) if meta.file_type().is_dir() => (),
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => break,
                    _ => return Err("删除事务含不安全的暂存目录".into()),
                }
            }
            let staged = stage.join(relative);
            if staged.exists() {
                regular_file(&staged)?;
                let destination = root.join(relative);
                if let Some(parent) = destination.parent() {
                    if parts.len() == 3 {
                        crate::sharing::real_directory(&root.join("cache"))?;
                        crate::sharing::real_directory(parent)?;
                    }
                }
                fs::rename(staged, destination)
                    .map_err(|e| format!("无法恢复删除中的素材：{e}"))?;
            }
        }
        Self::validate(root, &journal.manifest)?;
        match journal.management {
            Some(m) => atomic_json(&root.join("management.json"), &m)?,
            None => match fs::remove_file(root.join("management.json")) {
                Ok(()) => (),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
                Err(e) => return Err(err(e)),
            },
        }
        atomic_json(&root.join("library.json"), &journal.manifest)?;
        fs::remove_dir_all(stage).map_err(|e| format!("无法清理已恢复的删除事务：{e}"))?;
        Ok(())
    }

    pub fn delete_permanently(
        &mut self,
        expected_root: &str,
        ids: Vec<String>,
    ) -> Result<DeleteResult, String> {
        self.delete_with(expected_root, ids, crate::native::trash)
    }
    pub fn empty_trash(&mut self, expected_root: &str) -> Result<DeleteResult, String> {
        self.check_expected_root(expected_root)?;
        let ids = self.get_management()?.trash.keys().cloned().collect();
        self.delete_permanently(expected_root, ids)
    }
    fn delete_with(
        &mut self,
        expected_root: &str,
        ids: Vec<String>,
        trash: impl FnOnce(&Path) -> Result<(), String>,
    ) -> Result<DeleteResult, String> {
        self.check_expected_root(expected_root)?;
        let mut m = self.get_management()?;
        if ids.is_empty() {
            return Ok(DeleteResult {
                snapshot: self.snapshot(),
                management: m,
                deleted: 0,
            });
        }
        self.validate_selection(&ids)?;
        if ids.iter().any(|id| !m.trash.contains_key(id)) {
            return Err("只能彻底删除已在回收站中的素材".into());
        }
        let selected: HashSet<_> = ids.into_iter().collect();
        let mut next = self.manifest.clone();
        next.items.retain(|item| !selected.contains(&item.id));
        m.metadata.retain(|id, _| !selected.contains(id));
        m.trash.retain(|id, _| !selected.contains(id));
        m.favorites.retain(|id, _| !selected.contains(id));
        m.recent.retain(|id, _| !selected.contains(id));
        m.references.retain(|r| !selected.contains(&r.asset_id));
        m.ignored_pairs
            .retain(|(a, b)| !selected.contains(a) && !selected.contains(b));
        for group in &mut m.groups {
            group.member_ids.retain(|id| !selected.contains(id));
            if !group.member_ids.contains(&group.main_asset_id) && !group.member_ids.is_empty() {
                group.main_asset_id = group.member_ids[0].clone();
            }
        }
        m.groups.retain(|g| g.member_ids.len() >= 2);
        self.validate_management(&m)?;
        let mut files: Vec<_> = self
            .manifest
            .items
            .iter()
            .filter(|i| selected.contains(&i.id))
            .map(|i| format!("assets/{}", i.file_name))
            .collect();
        let cache = self.root.join("cache");
        if cache.exists() {
            if !fs::symlink_metadata(&cache)
                .map_err(err)?
                .file_type()
                .is_dir()
            {
                return Err("缓存目录不能是符号链接".into());
            }
            let wechat = cache.join("wechat");
            if wechat.exists() {
                if !fs::symlink_metadata(&wechat)
                    .map_err(err)?
                    .file_type()
                    .is_dir()
                {
                    return Err("微信缓存目录不能是符号链接".into());
                }
                for id in &selected {
                    for ext in ["png", "gif"] {
                        let name = format!("cache/wechat/{id}.{ext}");
                        if fs::symlink_metadata(self.root.join(&name)).is_ok() {
                            regular_file(&self.root.join(&name))?;
                            files.push(name);
                        }
                    }
                }
            }
        }
        let original_management = if self.root.join("management.json").exists() {
            Some(self.get_management()?)
        } else {
            None
        };
        let stage = self.root.join("StickerNest Deleted Assets");
        fs::create_dir(&stage).map_err(|e| format!("无法准备删除事务：{e}"))?;
        let journal = DeleteJournal {
            manifest: self.manifest.clone(),
            management: original_management,
            files,
        };
        let result = (|| {
            atomic_json(&stage.join("journal.json"), &journal)?;
            for name in &journal.files {
                let destination = stage.join(name);
                fs::create_dir_all(destination.parent().unwrap()).map_err(err)?;
                fs::rename(self.root.join(name), destination)
                    .map_err(|e| format!("无法暂存待删除文件：{e}"))?;
            }
            atomic_json(&self.root.join("management.json"), &m)?;
            self.save(&next)?;
            trash(&stage)?;
            Ok::<_, String>(())
        })();
        if let Err(error) = result {
            return match Self::recover_deletion(&self.root) {
                Ok(()) => Err(format!("删除失败，已恢复原状态：{error}")),
                Err(recovery) => Err(format!(
                    "删除失败：{error}；恢复未完成，请重新打开资料库：{recovery}"
                )),
            };
        }
        self.manifest = next;
        Ok(DeleteResult {
            snapshot: self.snapshot(),
            management: m,
            deleted: selected.len(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> (tempfile::TempDir, Library, Vec<String>) {
        let dir = tempfile::tempdir().unwrap();
        let mut lib = Library::create(dir.path()).unwrap();
        let mut paths = vec![];
        for i in 0..4 {
            let path = dir.path().join(format!("{i}.png"));
            image::RgbaImage::from_pixel(2, 2, image::Rgba([i, 0, 0, 255]))
                .save(&path)
                .unwrap();
            paths.push(path);
        }
        lib.import_files(paths, "本地".into()).unwrap();
        let ids = lib.snapshot().items.iter().map(|i| i.id.clone()).collect();
        (dir, lib, ids)
    }
    #[test]
    fn deletion_removes_index_annotations_references_groups_and_cache() {
        let (dir, mut lib, ids) = fixture();
        let root = lib.snapshot().root;
        lib.save_metadata(
            &root,
            ids[0].clone(),
            "名字".into(),
            vec![],
            vec!["合集".into()],
        )
        .unwrap();
        lib.set_favorite(&root, ids.clone(), true).unwrap();
        lib.mark_used(&root, ids.clone()).unwrap();
        lib.ignore_pair(&root, &ids[0], &ids[2]).unwrap();
        lib.create_group(&root, ids[..3].to_vec(), ids[0].clone(), vec![], vec![])
            .unwrap();
        let mut m = lib.get_management().unwrap();
        m.accounts.push(crate::management::Account {
            id: "a".repeat(64),
            platform: "微信".into(),
            alias: "测试".into(),
        });
        m.references.push(crate::management::Reference {
            account_id: "a".repeat(64),
            sticker_id: "123".into(),
            asset_id: ids[0].clone(),
            resource_identity: "b".repeat(64),
        });
        // Historical files can contain a trashed group member; deletion must repair it.
        m.trash.insert(ids[0].clone(), 1);
        lib.write_management(&m).unwrap();
        let cache = Path::new(&root).join("cache/wechat");
        fs::create_dir_all(&cache).unwrap();
        fs::write(cache.join(format!("{}.png", ids[0])), b"cache").unwrap();
        let trash_path = dir.path().join("fake-system-trash");
        let deleted = lib
            .delete_with(&root, vec![ids[0].clone(), ids[0].clone()], |stage| {
                fs::rename(stage, &trash_path).map_err(err)
            })
            .unwrap();
        assert_eq!(deleted.deleted, 1);
        assert_eq!(deleted.snapshot.items.len(), 3);
        assert!(!deleted.management.metadata.contains_key(&ids[0]));
        assert!(!deleted.management.trash.contains_key(&ids[0]));
        assert!(!deleted.management.favorites.contains_key(&ids[0]));
        assert!(!deleted.management.recent.contains_key(&ids[0]));
        assert!(deleted.management.references.is_empty());
        assert!(deleted.management.ignored_pairs.is_empty());
        assert_eq!(deleted.management.groups[0].member_ids, ids[1..3]);
        assert_eq!(deleted.management.groups[0].main_asset_id, ids[1]);
        assert!(!cache.join(format!("{}.png", ids[0])).exists());
        assert!(trash_path.join(format!("assets/{}.png", ids[0])).is_file());
        let mut m = lib.get_management().unwrap();
        m.trash.insert(ids[1].clone(), 1);
        lib.write_management(&m).unwrap();
        let deleted = lib
            .delete_with(&root, vec![ids[1].clone()], |stage| {
                fs::rename(stage, dir.path().join("fake-trash-2")).map_err(err)
            })
            .unwrap();
        assert!(deleted.management.groups.is_empty());
        drop(lib);
        let mut lib = Library::open(Path::new(&root)).unwrap();
        assert_eq!(lib.snapshot().items.len(), 2);
        assert_eq!(lib.empty_trash(&root).unwrap().deleted, 0);
    }
    #[test]
    fn deletion_rejects_live_assets_and_rolls_back_failed_trash_and_index_write() {
        let (dir, mut lib, ids) = fixture();
        let root = lib.snapshot().root;
        assert!(lib
            .delete_with(&root, vec![ids[0].clone()], |_| panic!("must not trash"))
            .err()
            .unwrap()
            .contains("回收站"));
        assert!(lib
            .delete_with("wrong-root", vec![ids[0].clone()], |_| panic!(
                "must not trash"
            ))
            .is_err());
        assert!(lib
            .delete_with(&root, vec!["f".repeat(64)], |_| panic!("must not trash"))
            .is_err());
        lib.set_trash(&root, ids.clone(), true).unwrap();
        let manifest = fs::read(Path::new(&root).join("library.json")).unwrap();
        let management: serde_json::Value =
            serde_json::from_slice(&fs::read(Path::new(&root).join("management.json")).unwrap())
                .unwrap();
        let error = lib
            .delete_with(&root, ids.clone(), |_| Err("模拟废纸篓失败".into()))
            .err()
            .unwrap();
        assert!(error.contains("已恢复原状态"), "{error}");
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(
                &fs::read(Path::new(&root).join("management.json")).unwrap()
            )
            .unwrap(),
            management
        );
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(
                &fs::read(Path::new(&root).join("library.json")).unwrap()
            )
            .unwrap(),
            serde_json::from_slice::<serde_json::Value>(&manifest).unwrap()
        );
        assert_eq!(lib.snapshot().items.len(), 4);
        for id in &ids {
            assert!(lib.asset_path(id).unwrap().exists());
        }
        #[cfg(unix)]
        {
            let index = Path::new(&root).join("library.json");
            let saved = dir.path().join("saved-index");
            fs::rename(&index, &saved).unwrap();
            std::os::unix::fs::symlink(&saved, &index).unwrap();
            let error = lib
                .delete_with(&root, ids.clone(), |_| panic!("must not trash"))
                .err()
                .unwrap();
            assert!(error.contains("删除失败"));
            fs::remove_file(&index).unwrap();
            fs::rename(saved, index).unwrap();
            // A failed rollback is retained for retry after the filesystem obstruction is removed.
            Library::recover_deletion(Path::new(&root)).unwrap();
            for id in &ids {
                assert!(lib.asset_path(id).unwrap().exists());
            }
        }
    }
    #[test]
    fn interrupted_deletion_restores_before_library_open() {
        let (_dir, lib, ids) = fixture();
        let root = PathBuf::from(lib.snapshot().root);
        lib.set_trash(root.to_str().unwrap(), vec![ids[0].clone()], true)
            .unwrap();
        let stage = root.join("StickerNest Deleted Assets");
        fs::create_dir_all(stage.join("assets")).unwrap();
        let name = format!("assets/{}.png", ids[0]);
        atomic_json(
            &stage.join("journal.json"),
            &DeleteJournal {
                manifest: lib.manifest.clone(),
                management: Some(lib.get_management().unwrap()),
                files: vec![name.clone()],
            },
        )
        .unwrap();
        fs::rename(root.join(&name), stage.join(&name)).unwrap();
        let mut next = lib.manifest.clone();
        next.items.retain(|i| i.id != ids[0]);
        lib.save(&next).unwrap();
        drop(lib);
        let lib = Library::open(&root).unwrap();
        assert_eq!(lib.snapshot().items.len(), 4);
        assert!(lib.asset_path(&ids[0]).unwrap().is_file());
        assert!(lib.get_management().unwrap().trash.contains_key(&ids[0]));
        assert!(!stage.exists());
    }
}
