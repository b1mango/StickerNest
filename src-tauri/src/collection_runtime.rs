use std::{path::PathBuf, process::Command};

/// All collection tools and interpreters belong to one application resource.
/// Never resolve executable names through the user's PATH.
pub struct CollectionRuntime {
    root: PathBuf,
}
impl CollectionRuntime {
    pub fn at(root: PathBuf) -> Result<Self, String> {
        for file in [
            "node",
            "python/bin/python3",
            "scripts/collect_douyin.mjs",
            "scripts/download_douyin.py",
            "scripts/download_wechat.py",
        ] {
            if !root.join(file).is_file() {
                return Err(format!(
                    "应用采集组件不完整（{file}），请重新安装完整应用。"
                ));
            }
        }
        Ok(Self { root })
    }
    pub fn script(&self, name: &str) -> PathBuf {
        self.root.join("scripts").join(name)
    }
    pub fn node(&self) -> Command {
        let mut command = Command::new(self.root.join("node"));
        command.env_remove("NODE_OPTIONS").env_remove("NODE_PATH");
        command
    }
    pub fn python(&self) -> Command {
        let mut command = Command::new(self.root.join("python/bin/python3"));
        // Isolated mode ignores PYTHONHOME/PYTHONPATH/user site packages; don't
        // write bytecode into the signed application bundle.
        command.args(["-I", "-B"]);
        command
    }
    #[cfg(test)]
    pub fn fixture(root: &std::path::Path) -> Self {
        Self {
            root: root.to_path_buf(),
        }
    }
}
