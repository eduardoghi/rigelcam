//! The saved recorders. Addresses go in a JSON file, passwords in the system keyring.

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::layout::Layout;

const KEYRING_SERVICE: &str = "rigelcam";

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("could not read or write the recorder list ({0})")]
    File(#[from] io::Error),
    #[error("the recorder list is corrupted ({0})")]
    Format(#[from] serde_json::Error),
    #[error("the system keyring failed ({0})")]
    Keyring(String),
    #[error("this system has no folder for configuration files")]
    NoConfigDir,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Recorder {
    pub id: u64,
    pub name: String,
    pub host: String,
    pub user: String,
}

/// Where passwords are kept, so tests don't touch the keyring of the machine.
pub trait Secrets {
    fn set(&self, recorder_id: u64, password: &str) -> Result<(), Error>;
    fn get(&self, recorder_id: u64) -> Result<String, Error>;
    fn delete(&self, recorder_id: u64) -> Result<(), Error>;
}

pub struct SystemKeyring;

fn entry(recorder_id: u64) -> Result<keyring::Entry, Error> {
    keyring::Entry::new(KEYRING_SERVICE, &format!("recorder-{recorder_id}"))
        .map_err(|e| Error::Keyring(e.to_string()))
}

impl Secrets for SystemKeyring {
    fn set(&self, recorder_id: u64, password: &str) -> Result<(), Error> {
        entry(recorder_id)?
            .set_password(password)
            .map_err(|e| Error::Keyring(e.to_string()))
    }

    fn get(&self, recorder_id: u64) -> Result<String, Error> {
        entry(recorder_id)?
            .get_password()
            .map_err(|e| Error::Keyring(e.to_string()))
    }

    fn delete(&self, recorder_id: u64) -> Result<(), Error> {
        match entry(recorder_id)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(Error::Keyring(e.to_string())),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct Saved {
    recorders: Vec<Recorder>,
    /// Never goes back, so an answer that was still on its way from a removed recorder can't be
    /// taken for a new one.
    #[serde(default)]
    next_id: u64,
    /// None until the first recorder answers with its channels.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    layout: Option<Layout>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    sidebar_collapsed: bool,
}

pub struct RecorderList<S: Secrets> {
    path: PathBuf,
    secrets: S,
    saved: Saved,
}

impl RecorderList<SystemKeyring> {
    /// Opens the list in the configuration folder of the user.
    pub fn open_default() -> Result<Self, Error> {
        let path = dirs::config_dir()
            .ok_or(Error::NoConfigDir)?
            .join("rigelcam")
            .join("recorders.json");
        Self::open(path, SystemKeyring)
    }
}

impl<S: Secrets> RecorderList<S> {
    pub fn open(path: PathBuf, secrets: S) -> Result<Self, Error> {
        let mut saved: Saved = match fs::read(&path) {
            Ok(bytes) => serde_json::from_slice(&bytes)?,
            Err(e) if e.kind() == io::ErrorKind::NotFound => Saved::default(),
            Err(e) => return Err(e.into()),
        };
        if let Some(layout) = &mut saved.layout {
            layout.normalize();
        }
        Ok(Self {
            path,
            secrets,
            saved,
        })
    }

    pub fn recorders(&self) -> &[Recorder] {
        &self.saved.recorders
    }

    /// Saves a new recorder. The password goes first, so a keyring failure doesn't
    /// leave a recorder in the list that has no password.
    pub fn add(
        &mut self,
        name: &str,
        host: &str,
        user: &str,
        password: &str,
    ) -> Result<u64, Error> {
        let highest = self.saved.recorders.iter().map(|r| r.id).max();
        let id = self.saved.next_id.max(highest.unwrap_or(0) + 1);
        self.secrets.set(id, password)?;
        let before = self.saved.clone();
        self.saved.next_id = id + 1;
        self.saved.recorders.push(Recorder {
            id,
            name: name.to_owned(),
            host: host.to_owned(),
            user: user.to_owned(),
        });
        if let Err(error) = self.write() {
            self.saved = before;
            let _ = self.secrets.delete(id);
            return Err(error);
        }
        Ok(id)
    }

    pub fn remove(&mut self, id: u64) -> Result<(), Error> {
        let password = self.secrets.get(id).ok();
        self.secrets.delete(id)?;
        let before = self.saved.clone();
        // A list saved before next_id existed has to remember the id going away too.
        self.saved.next_id = self.saved.next_id.max(id + 1);
        self.saved.recorders.retain(|r| r.id != id);
        if let Some(layout) = &mut self.saved.layout {
            layout.remove_recorder(id);
        }
        if let Err(error) = self.write() {
            self.saved = before;
            if let Some(password) = password {
                let _ = self.secrets.set(id, &password);
            }
            return Err(error);
        }
        Ok(())
    }

    pub fn layout(&self) -> Option<&Layout> {
        self.saved.layout.as_ref()
    }

    pub fn set_layout(&mut self, layout: Layout) -> Result<(), Error> {
        self.saved.layout = Some(layout);
        self.write()
    }

    pub fn sidebar_collapsed(&self) -> bool {
        self.saved.sidebar_collapsed
    }

    pub fn set_sidebar_collapsed(&mut self, collapsed: bool) -> Result<(), Error> {
        self.saved.sidebar_collapsed = collapsed;
        self.write()
    }

    pub fn password(&self, id: u64) -> Result<String, Error> {
        self.secrets.get(id)
    }

    fn write(&self) -> Result<(), Error> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
            restrict_folder(parent)?;
        }
        write_atomically(&self.path, &serde_json::to_vec_pretty(&self.saved)?)
    }
}

// The list tells which recorders exist and which user opens them, so it stays private.
#[cfg(unix)]
fn restrict_folder(path: &Path) -> Result<(), Error> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
    Ok(())
}

#[cfg(not(unix))]
fn restrict_folder(_: &Path) -> Result<(), Error> {
    Ok(())
}

fn create_private(path: &Path) -> io::Result<fs::File> {
    let mut options = fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(path)
}

// Through a temporary file, so a crash or a power cut while saving can't lose the list.
fn write_atomically(path: &Path, bytes: &[u8]) -> Result<(), Error> {
    let temporary = path.with_extension("json.tmp");
    let _ = fs::remove_file(&temporary);
    let mut file = create_private(&temporary)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    fs::rename(&temporary, path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::CameraId;
    use std::cell::RefCell;
    use std::collections::HashMap;
    use std::rc::Rc;

    #[derive(Clone, Default)]
    struct MemorySecrets(Rc<RefCell<HashMap<u64, String>>>);

    impl Secrets for MemorySecrets {
        fn set(&self, id: u64, password: &str) -> Result<(), Error> {
            self.0.borrow_mut().insert(id, password.to_owned());
            Ok(())
        }
        fn get(&self, id: u64) -> Result<String, Error> {
            self.0
                .borrow()
                .get(&id)
                .cloned()
                .ok_or_else(|| Error::Keyring("missing".to_owned()))
        }
        fn delete(&self, id: u64) -> Result<(), Error> {
            self.0.borrow_mut().remove(&id);
            Ok(())
        }
    }

    struct FailingSecrets;

    impl Secrets for FailingSecrets {
        fn set(&self, _: u64, _: &str) -> Result<(), Error> {
            Err(Error::Keyring("locked".to_owned()))
        }
        fn get(&self, _: u64) -> Result<String, Error> {
            Err(Error::Keyring("locked".to_owned()))
        }
        fn delete(&self, _: u64) -> Result<(), Error> {
            Err(Error::Keyring("locked".to_owned()))
        }
    }

    fn temp_path(test: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("rigelcam-{test}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        dir.join("recorders.json")
    }

    #[test]
    fn password_never_reaches_the_file() {
        let path = temp_path("password");
        let mut list = RecorderList::open(path.clone(), MemorySecrets::default()).unwrap();
        let id = list
            .add("Depot", "192.0.2.1", "viewer", "s3cret-value")
            .unwrap();
        assert_eq!(list.password(id).unwrap(), "s3cret-value");
        let file = fs::read_to_string(&path).unwrap();
        assert!(file.contains("192.0.2.1"));
        assert!(!file.contains("s3cret-value"));
    }

    #[test]
    fn recorders_layout_and_sidebar_survive_reopening() {
        let path = temp_path("reopen");
        let secrets = MemorySecrets::default();
        let mut list = RecorderList::open(path.clone(), secrets.clone()).unwrap();
        let first = list.add("A", "192.0.2.1", "u", "p").unwrap();
        list.add("B", "192.0.2.2", "u", "p").unwrap();
        let layout = Layout::fitting(&[CameraId {
            recorder: first,
            channel: 1,
        }]);
        list.set_layout(layout.clone()).unwrap();
        list.set_sidebar_collapsed(true).unwrap();
        let reopened = RecorderList::open(path, secrets).unwrap();
        assert_eq!(reopened.recorders().len(), 2);
        assert_eq!(reopened.layout(), Some(&layout));
        assert!(reopened.sidebar_collapsed());
    }

    #[test]
    fn removing_deletes_the_password_and_empties_its_squares() {
        let path = temp_path("remove");
        let mut list = RecorderList::open(path, MemorySecrets::default()).unwrap();
        let first = list.add("A", "192.0.2.1", "u", "p").unwrap();
        let second = list.add("B", "192.0.2.2", "u", "p").unwrap();
        let camera = |recorder| {
            Some(CameraId {
                recorder,
                channel: 1,
            })
        };
        let layout = Layout {
            columns: 2,
            cameras: vec![camera(first), camera(second), None, None],
        };
        list.set_layout(layout).unwrap();
        list.remove(second).unwrap();
        assert!(list.password(second).is_err());
        assert_eq!(list.recorders().len(), 1);
        assert_eq!(
            list.layout().unwrap().cameras,
            vec![camera(first), None, None, None]
        );
    }

    #[test]
    fn file_saved_before_layouts_still_opens() {
        let path = temp_path("older");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(
            &path,
            r#"{"selected":1,"recorders":[{"id":1,"name":"A","host":"192.0.2.1","user":"u"}]}"#,
        )
        .unwrap();
        let list = RecorderList::open(path, MemorySecrets::default()).unwrap();
        assert_eq!(list.recorders().len(), 1);
        assert_eq!(list.layout(), None);
        assert!(!list.sidebar_collapsed());
    }

    #[test]
    fn ids_never_come_back() {
        let path = temp_path("ids");
        let mut list = RecorderList::open(path, MemorySecrets::default()).unwrap();
        let first = list.add("A", "192.0.2.1", "u", "p").unwrap();
        list.remove(first).unwrap();
        assert_ne!(list.add("B", "192.0.2.2", "u", "p").unwrap(), first);
    }

    #[test]
    fn removing_from_a_list_older_than_next_id_keeps_the_id_used() {
        let path = temp_path("old-ids");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(
            &path,
            r#"{"recorders":[{"id":5,"name":"A","host":"192.0.2.1","user":"u"}]}"#,
        )
        .unwrap();
        let mut list = RecorderList::open(path, MemorySecrets::default()).unwrap();
        list.remove(5).unwrap();
        assert_eq!(list.add("B", "192.0.2.2", "u", "p").unwrap(), 6);
    }

    #[test]
    fn failing_to_save_undoes_adding_and_removing() {
        let path = temp_path("unsaved");
        let secrets = MemorySecrets::default();
        let mut list = RecorderList::open(path.clone(), secrets.clone()).unwrap();
        let id = list.add("A", "192.0.2.1", "u", "p").unwrap();
        // A folder in the way of the temporary file makes every save fail.
        fs::create_dir(path.with_extension("json.tmp")).unwrap();
        assert!(list.add("B", "192.0.2.2", "u", "p").is_err());
        assert_eq!(list.recorders().len(), 1);
        assert!(secrets.get(id + 1).is_err());
        assert!(list.remove(id).is_err());
        assert_eq!(list.recorders().len(), 1);
        assert_eq!(list.password(id).unwrap(), "p");
    }

    #[test]
    fn layout_read_from_the_file_stays_within_the_limits() {
        let path = temp_path("huge");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(
            &path,
            r#"{"recorders":[],"layout":{"columns":1000,"cameras":[]}}"#,
        )
        .unwrap();
        let list = RecorderList::open(path, MemorySecrets::default()).unwrap();
        let layout = list.layout().unwrap();
        assert_eq!((layout.columns, layout.cameras.len()), (6, 36));
    }

    #[cfg(unix)]
    #[test]
    fn saved_list_is_private() {
        use std::os::unix::fs::PermissionsExt;
        let path = temp_path("private");
        let mut list = RecorderList::open(path.clone(), MemorySecrets::default()).unwrap();
        list.add("A", "192.0.2.1", "u", "p").unwrap();
        let mode = |p: &Path| fs::metadata(p).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode(&path), 0o600);
        assert_eq!(mode(path.parent().unwrap()), 0o700);
    }

    #[test]
    fn keyring_failure_adds_nothing() {
        let path = temp_path("keyring");
        let mut list = RecorderList::open(path.clone(), FailingSecrets).unwrap();
        assert!(list.add("A", "192.0.2.1", "u", "p").is_err());
        assert!(list.recorders().is_empty());
        assert!(!path.exists());
    }

    #[test]
    fn corrupted_file_is_an_error_and_is_not_overwritten() {
        let path = temp_path("corrupted");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, "not json").unwrap();
        assert!(matches!(
            RecorderList::open(path.clone(), MemorySecrets::default()),
            Err(Error::Format(_))
        ));
        assert_eq!(fs::read_to_string(&path).unwrap(), "not json");
    }
}
