//! The saved recorders. Addresses go in a JSON file, passwords in the system keyring.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

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

#[derive(Debug, Default, Serialize, Deserialize)]
struct Saved {
    selected: Option<u64>,
    recorders: Vec<Recorder>,
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
        let saved = match fs::read(&path) {
            Ok(bytes) => serde_json::from_slice(&bytes)?,
            Err(e) if e.kind() == io::ErrorKind::NotFound => Saved::default(),
            Err(e) => return Err(e.into()),
        };
        Ok(Self {
            path,
            secrets,
            saved,
        })
    }

    pub fn recorders(&self) -> &[Recorder] {
        &self.saved.recorders
    }

    /// The recorder to show, which falls back to the first one when none was chosen.
    pub fn selected(&self) -> Option<&Recorder> {
        let recorders = &self.saved.recorders;
        self.saved
            .selected
            .and_then(|id| recorders.iter().find(|r| r.id == id))
            .or(recorders.first())
    }

    pub fn select(&mut self, id: u64) -> Result<(), Error> {
        self.saved.selected = Some(id);
        self.write()
    }

    /// Saves a new recorder and selects it. The password goes first, so a keyring failure doesn't
    /// leave a recorder in the list that has no password.
    pub fn add(
        &mut self,
        name: &str,
        host: &str,
        user: &str,
        password: &str,
    ) -> Result<u64, Error> {
        let id = self.saved.recorders.iter().map(|r| r.id).max().unwrap_or(0) + 1;
        self.secrets.set(id, password)?;
        self.saved.recorders.push(Recorder {
            id,
            name: name.to_owned(),
            host: host.to_owned(),
            user: user.to_owned(),
        });
        self.saved.selected = Some(id);
        self.write()?;
        Ok(id)
    }

    pub fn remove(&mut self, id: u64) -> Result<(), Error> {
        self.secrets.delete(id)?;
        self.saved.recorders.retain(|r| r.id != id);
        if self.saved.selected == Some(id) {
            self.saved.selected = None;
        }
        self.write()
    }

    pub fn password(&self, id: u64) -> Result<String, Error> {
        self.secrets.get(id)
    }

    fn write(&self) -> Result<(), Error> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }
        write_atomically(&self.path, &serde_json::to_vec_pretty(&self.saved)?)
    }
}

// Through a temporary file, so a crash while saving can't lose the list.
fn write_atomically(path: &Path, bytes: &[u8]) -> Result<(), Error> {
    let temporary = path.with_extension("json.tmp");
    fs::write(&temporary, bytes)?;
    fs::rename(&temporary, path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
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
    fn list_and_selection_survive_reopening() {
        let path = temp_path("reopen");
        let secrets = MemorySecrets::default();
        let mut list = RecorderList::open(path.clone(), secrets.clone()).unwrap();
        let first = list.add("A", "192.0.2.1", "u", "p").unwrap();
        list.add("B", "192.0.2.2", "u", "p").unwrap();
        list.select(first).unwrap();
        let reopened = RecorderList::open(path, secrets).unwrap();
        assert_eq!(reopened.recorders().len(), 2);
        assert_eq!(reopened.selected().unwrap().name, "A");
    }

    #[test]
    fn removing_deletes_the_password_and_moves_the_selection() {
        let path = temp_path("remove");
        let mut list = RecorderList::open(path, MemorySecrets::default()).unwrap();
        let first = list.add("A", "192.0.2.1", "u", "p").unwrap();
        let second = list.add("B", "192.0.2.2", "u", "p").unwrap();
        list.remove(second).unwrap();
        assert!(list.password(second).is_err());
        assert_eq!(list.selected().unwrap().id, first);
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
