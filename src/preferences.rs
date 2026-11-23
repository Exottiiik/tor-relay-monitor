//! Persistent display preferences only; never store Tor events or metrics.
use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
pub struct Preferences {
    pub graph_height: u16,
    pub history_seconds: u32,
}
impl Default for Preferences {
    fn default() -> Self {
        Self {
            graph_height: 12,
            history_seconds: 120,
        }
    }
}
impl Preferences {
    pub fn validate(&self) -> Result<()> {
        if !(5..=30).contains(&self.graph_height)
            || !((30..=120).contains(&self.history_seconds)
                || matches!(self.history_seconds, 3600 | 86400))
        {
            bail!(
                "Graph height must be 5..30 rows; history must be 30..120, 3600 or 86400 seconds"
            );
        }
        Ok(())
    }
    pub fn step_window(&mut self, longer: bool) {
        let windows = [30, 60, 90, 120, 3600, 86400];
        self.history_seconds = if longer {
            windows
                .into_iter()
                .find(|w| *w > self.history_seconds)
                .unwrap_or(86400)
        } else {
            windows
                .into_iter()
                .rev()
                .find(|w| *w < self.history_seconds)
                .unwrap_or(30)
        };
    }
    /// Resolves an absolute per-user path. No implicit write occurs on startup.
    pub fn path() -> Option<PathBuf> {
        std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
            .or_else(|| std::env::var_os("HOME").map(|p| PathBuf::from(p).join(".config")))
            .map(|p| p.join("tor-relay-monitor/ui.toml"))
    }
    pub async fn load(path: &Path) -> Result<Self> {
        let file = match tokio::fs::File::open(path).await {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Self::default());
            }
            Err(error) => return Err(error).context("Cannot read display preferences"),
        };
        let mut contents = String::new();
        file.take(8193).read_to_string(&mut contents).await?;
        if contents.len() > 8192 {
            bail!("Display preferences exceed 8 KiB");
        }
        let preferences: Self = toml::from_str(&contents).context("Invalid display preferences")?;
        preferences.validate()?;
        Ok(preferences)
    }
    pub async fn save(&self, path: &Path) -> Result<()> {
        self.validate()?;
        let parent = path.parent().context("Missing preferences directory")?;
        tokio::fs::create_dir_all(parent)
            .await
            .context("Cannot create preferences directory")?;
        // Same-directory atomic replacement; incomplete writes never replace the prior file.
        let temporary = path.with_extension(format!("{}.tmp", std::process::id()));
        let mut options = tokio::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        options.mode(0o600);
        let mut file = options
            .open(&temporary)
            .await
            .context("Cannot create display preferences")?;
        let result = async {
            file.write_all(toml::to_string(self)?.as_bytes()).await?;
            file.sync_all().await?;
            drop(file);
            tokio::fs::rename(&temporary, path)
                .await
                .context("Cannot replace display preferences")?;
            Ok(())
        }
        .await;
        if result.is_err() {
            let _ = tokio::fs::remove_file(&temporary).await;
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn windows_cover_a_day_preserve_old_preferences_and_never_overflow() {
        let mut prefs = Preferences::default();
        for window in [3600, 86400, 86400] {
            prefs.step_window(true);
            assert_eq!(prefs.history_seconds, window);
            prefs.validate().unwrap();
        }
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("ui.toml");
        prefs.save(&path).await.unwrap();
        assert_eq!(
            Preferences::load(&path).await.unwrap().history_seconds,
            86400
        );
        for window in [3600, 120, 90, 60, 30, 30] {
            prefs.step_window(false);
            assert_eq!(prefs.history_seconds, window);
        }
        tokio::fs::write(&path, "graph_height = 14\nhistory_seconds = 75\n")
            .await
            .unwrap();
        assert_eq!(Preferences::load(&path).await.unwrap().history_seconds, 75);
        prefs.history_seconds = 86401;
        assert!(prefs.validate().is_err());
    }
    #[tokio::test]
    async fn preferences_survive_restart_and_reject_bad_input() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("display/ui.toml");
        assert_eq!(
            Preferences::load(&path).await.unwrap(),
            Preferences::default()
        );
        let preferences = Preferences {
            graph_height: 14,
            history_seconds: 60,
        };
        preferences.save(&path).await.unwrap();
        assert_eq!(Preferences::load(&path).await.unwrap(), preferences);
        preferences.save(&path).await.unwrap();
        tokio::fs::write(&path, "graph_height = 999\n")
            .await
            .unwrap();
        assert!(Preferences::load(&path).await.is_err());
        tokio::fs::write(&path, "x".repeat(8193)).await.unwrap();
        assert!(Preferences::load(&path).await.is_err());
    }
}
