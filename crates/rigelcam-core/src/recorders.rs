//! The saved recorders. Addresses go in a JSON file, passwords in the system keyring.

use std::fs::{self, File, TryLockError};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
use std::thread::JoinHandle;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::layout::Layout;
#[cfg(target_os = "linux")]
use crate::secret_portal;

const KEYRING_SERVICE: &str = "rigelcam";

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("could not read or write the recorder list ({0})")]
    File(#[from] io::Error),
    #[error("the recorder list is corrupted ({0})")]
    Format(#[from] serde_json::Error),
    #[error("the system keyring failed ({0})")]
    Keyring(String),
    #[error("the system keyring has no password for this recorder")]
    NoPassword,
    #[error("this system has no folder for configuration files")]
    NoConfigDir,
}

const DEFAULT_HTTP_PORT: u16 = 80;
const DEFAULT_RTSP_PORT: u16 = 554;

fn default_http_port() -> u16 {
    DEFAULT_HTTP_PORT
}

fn default_rtsp_port() -> u16 {
    DEFAULT_RTSP_PORT
}

fn is_default_http_port(port: &u16) -> bool {
    *port == DEFAULT_HTTP_PORT
}

fn is_default_rtsp_port(port: &u16) -> bool {
    *port == DEFAULT_RTSP_PORT
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Recorder {
    pub id: u64,
    pub name: String,
    pub host: String,
    pub user: String,
    #[serde(
        default = "default_http_port",
        skip_serializing_if = "is_default_http_port"
    )]
    pub http_port: u16,
    #[serde(
        default = "default_rtsp_port",
        skip_serializing_if = "is_default_rtsp_port"
    )]
    pub rtsp_port: u16,
}

impl Recorder {
    pub fn address(&self) -> Address {
        Address {
            host: self.host.clone(),
            http_port: self.http_port,
            rtsp_port: self.rtsp_port,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum AddressError {
    #[error("the address carries a user or password")]
    Credentials,
    #[error("the client doesn't speak HTTPS")]
    Https,
    #[error("the address isn't a valid host")]
    Invalid,
}

/// Where a recorder answers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Address {
    pub host: String,
    pub http_port: u16,
    pub rtsp_port: u16,
}

impl Address {
    /// Reads what someone typed, which may be a URL copied from a browser. A port in the text wins
    /// over `http_port`, and a missing port is the factory one. A user or password is refused,
    /// because this text goes to a plain file and to the logs.
    pub fn parse(
        text: &str,
        http_port: Option<u16>,
        rtsp_port: Option<u16>,
    ) -> Result<Self, AddressError> {
        let text = text.trim();
        // Dropping the scheme would send what was meant for TLS over plain HTTP.
        if text.starts_with("https://") {
            return Err(AddressError::Https);
        }
        let text = text.strip_prefix("http://").unwrap_or(text);
        let authority = text.split('/').next().unwrap_or(text);
        if authority.contains('@') {
            return Err(AddressError::Credentials);
        }
        let (host, port) = split_port(authority)?;
        let valid = |c: char| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | ':');
        let bare = host.trim_start_matches('[').trim_end_matches(']');
        if bare.is_empty() || !bare.chars().all(valid) {
            return Err(AddressError::Invalid);
        }
        Ok(Self {
            host: host.to_owned(),
            http_port: port.or(http_port).unwrap_or(DEFAULT_HTTP_PORT),
            rtsp_port: rtsp_port.unwrap_or(DEFAULT_RTSP_PORT),
        })
    }
}

// An IPv6 address has colons of its own, so it needs brackets to carry a port.
fn split_port(authority: &str) -> Result<(&str, Option<u16>), AddressError> {
    let (host, port) = match authority.rfind(':') {
        Some(colon) if !authority[colon..].contains(']') => {
            (&authority[..colon], Some(&authority[colon + 1..]))
        }
        _ => (authority, None),
    };
    if host.contains(':') && !host.starts_with('[') {
        return Err(AddressError::Invalid);
    }
    match port {
        None => Ok((host, None)),
        Some(port) => match port.parse::<u16>() {
            Ok(number) if number != 0 => Ok((host, Some(number))),
            _ => Err(AddressError::Invalid),
        },
    }
}

/// Where passwords are kept, so tests don't touch the keyring of the machine.
pub trait Secrets {
    fn set(&self, recorder_id: u64, password: &str) -> Result<(), Error>;
    fn get(&self, recorder_id: u64) -> Result<String, Error>;
    fn delete(&self, recorder_id: u64) -> Result<(), Error>;
}

pub struct SystemKeyring;

fn account(recorder_id: u64) -> String {
    format!("recorder-{recorder_id}")
}

/// Flatpak creates this file in every sandbox.
#[cfg(target_os = "linux")]
fn in_flatpak() -> bool {
    Path::new("/.flatpak-info").exists()
}

fn entry(recorder_id: u64) -> Result<keyring::Entry, Error> {
    // The library tries the keyring once per run, and after a failure it only answers that there
    // is no store, which hides why.
    if let Err(error) = keyring::Entry::store_status() {
        return Err(Error::Keyring(error.to_string()));
    }
    keyring::Entry::new(KEYRING_SERVICE, &account(recorder_id))
        .map_err(|e| Error::Keyring(e.to_string()))
}

impl Secrets for SystemKeyring {
    fn set(&self, recorder_id: u64, password: &str) -> Result<(), Error> {
        #[cfg(target_os = "linux")]
        if in_flatpak() {
            return secret_portal::set(KEYRING_SERVICE, &account(recorder_id), password);
        }
        entry(recorder_id)?
            .set_password(password)
            .map_err(|e| Error::Keyring(e.to_string()))
    }

    fn get(&self, recorder_id: u64) -> Result<String, Error> {
        #[cfg(target_os = "linux")]
        if in_flatpak() {
            return secret_portal::get(KEYRING_SERVICE, &account(recorder_id));
        }
        match entry(recorder_id)?.get_password() {
            Ok(password) => Ok(password),
            Err(keyring::Error::NoEntry) => Err(Error::NoPassword),
            Err(e) => Err(Error::Keyring(e.to_string())),
        }
    }

    fn delete(&self, recorder_id: u64) -> Result<(), Error> {
        #[cfg(target_os = "linux")]
        if in_flatpak() {
            return secret_portal::delete(KEYRING_SERVICE, &account(recorder_id));
        }
        match entry(recorder_id)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(Error::Keyring(e.to_string())),
        }
    }
}

/// What the keyring held for a recorder before a change, to undo it.
#[derive(Debug, PartialEq)]
pub enum Previous {
    Password(String),
    Nothing,
    /// The keyring failed to answer, so undoing could only guess.
    Unknown,
}

impl Previous {
    fn of(secrets: &impl Secrets, id: u64) -> Self {
        match secrets.get(id) {
            Ok(password) => Self::Password(password),
            Err(Error::NoPassword) => Self::Nothing,
            Err(_) => Self::Unknown,
        }
    }
}

/// Puts the password of a recorder about to be saved, and returns what it replaces. The keyring
/// goes first, so a failure there leaves the list as it was.
pub fn put_password(secrets: &impl Secrets, id: u64, password: &str) -> Result<Previous, Error> {
    let previous = Previous::of(secrets, id);
    secrets.set(id, password)?;
    Ok(previous)
}

/// Undoes `put_password` or `take_password` after the list couldn't be saved, putting back the
/// password there was, or deleting the one of a recorder that never made it to the list. When the
/// keyring didn't tell what there was, the new password stays, since deleting could lose the only
/// one that works.
pub fn restore_password(secrets: &impl Secrets, id: u64, previous: &Previous) {
    let _ = match previous {
        Previous::Password(password) => secrets.set(id, password),
        Previous::Nothing => secrets.delete(id),
        Previous::Unknown => Ok(()),
    };
}

/// Deletes the password of a recorder about to be removed. Returns it, for `restore_password`, and
/// whether it went, since a keyring that refuses doesn't keep the recorder in the list.
pub fn take_password(secrets: &impl Secrets, id: u64) -> (Previous, bool) {
    let previous = Previous::of(secrets, id);
    (previous, secrets.delete(id).is_ok())
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
    /// "pt_BR", "en" or "es", or nothing to follow the system.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    language: Option<String>,
    /// "light" or "dark", or nothing to follow the system.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    theme: Option<String>,
    /// `false` keeps the app from asking GitHub for new versions.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    check_updates: Option<bool>,
}

/// 2026-01-01 in seconds since 1970, where ids of a list started over count from.
const ID_EPOCH: u64 = 1_767_225_600;

/// The saved recorders without their passwords, which can wait for the keyring to be unlocked and
/// so go through `put_password`, `restore_password` and `take_password` on another thread.
pub struct RecorderList {
    path: PathBuf,
    saved: Saved,
    saver: Saver,
}

/// Saves what the interface changes on a thread of its own, since a save waits for the disk.
/// Only the newest copy waits, so a burst of changes costs one save.
#[derive(Default)]
struct Saver {
    queue: Arc<SaveQueue>,
    thread: Option<JoinHandle<()>>,
}

#[derive(Default)]
struct SaveQueue {
    state: Mutex<SaveState>,
    changed: Condvar,
}

#[derive(Default)]
struct SaveState {
    pending: Option<Vec<u8>>,
    saving: bool,
    closing: bool,
    on_error: Option<Arc<ErrorHandler>>,
}

type ErrorHandler = dyn Fn(Error) + Send + Sync;

impl SaveQueue {
    fn lock(&self) -> MutexGuard<'_, SaveState> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn wait<'a>(&self, state: MutexGuard<'a, SaveState>) -> MutexGuard<'a, SaveState> {
        self.changed
            .wait(state)
            .unwrap_or_else(PoisonError::into_inner)
    }

    fn run(&self, path: &Path) {
        let mut state = self.lock();
        loop {
            if let Some(bytes) = state.pending.take() {
                state.saving = true;
                drop(state);
                let result = save(path, &bytes);
                state = self.lock();
                state.saving = false;
                self.changed.notify_all();
                if let Err(error) = result {
                    let handler = state.on_error.clone();
                    drop(state);
                    if let Some(handler) = handler {
                        handler(error);
                    }
                    state = self.lock();
                }
            } else if state.closing {
                return;
            } else {
                state = self.wait(state);
            }
        }
    }
}

/// Keeps other runs of the app away from the list while it lives. Two of them would each save
/// their own copy over the other, and give the same id to different recorders.
pub struct InstanceLock {
    _file: File,
}

/// Takes the lock of this user, or returns None when another run of the app holds it.
pub fn lock_instance() -> Result<Option<InstanceLock>, Error> {
    let path = default_path()?.with_file_name("instance.lock");
    if let Some(folder) = path.parent() {
        fs::create_dir_all(folder)?;
    }
    let file = File::options()
        .create(true)
        .truncate(false)
        .write(true)
        .open(path)?;
    match file.try_lock() {
        Ok(()) => Ok(Some(InstanceLock { _file: file })),
        Err(TryLockError::WouldBlock) => Ok(None),
        Err(TryLockError::Error(error)) => Err(error.into()),
    }
}

/// The file of the saved list, to show when something is wrong with it.
pub fn list_path() -> Result<PathBuf, Error> {
    default_path()
}

fn default_path() -> Result<PathBuf, Error> {
    Ok(dirs::config_dir()
        .ok_or(Error::NoConfigDir)?
        .join("rigelcam")
        .join("recorders.json"))
}

impl RecorderList {
    /// Opens the list in the configuration folder of the user.
    pub fn open_default() -> Result<Self, Error> {
        Self::open(default_path()?)
    }

    /// Starts an empty list after setting the file of the user aside.
    pub fn start_over_default() -> Result<(Self, PathBuf), Error> {
        Self::start_over(default_path()?)
    }

    /// Keeps a damaged file next to the list under a new name, so it can still be fixed by hand,
    /// and opens an empty list, also returning where the copy went. The name has the time so that
    /// an older copy isn't overwritten. New ids start from the time too, since the passwords of
    /// the damaged list stay in the keyring and it may come back.
    pub fn start_over(path: PathBuf) -> Result<(Self, PathBuf), Error> {
        let seconds = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |elapsed| elapsed.as_secs());
        let copy = path.with_extension(format!("json.corrupted-{seconds}"));
        fs::rename(&path, &copy)?;
        let mut list = Self::open(path)?;
        // Counted from 2026, since the interface takes ids as 32 bit numbers, which seconds since
        // 1970 outgrow in 2038.
        list.saved.next_id = seconds.saturating_sub(ID_EPOCH).max(1);
        list.write()?;
        Ok((list, copy))
    }

    pub fn open(path: PathBuf) -> Result<Self, Error> {
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
            saved,
            saver: Saver::default(),
        })
    }

    pub fn recorders(&self) -> &[Recorder] {
        &self.saved.recorders
    }

    /// An id for a recorder that isn't in the list yet, which a keyring can take before `store`.
    pub fn reserve_id(&mut self) -> u64 {
        let highest = self.saved.recorders.iter().map(|r| r.id).max();
        let id = self.saved.next_id.max(highest.unwrap_or(0) + 1);
        self.saved.next_id = id + 1;
        id
    }

    /// Saves a recorder by id, without touching its password, which can wait for the user to unlock
    /// the keyring and so goes apart from this.
    pub fn store(
        &mut self,
        id: u64,
        name: &str,
        address: &Address,
        user: &str,
    ) -> Result<(), Error> {
        let before = self.saved.clone();
        if let Some(recorder) = self.saved.recorders.iter_mut().find(|r| r.id == id) {
            recorder.name = name.to_owned();
            recorder.host = address.host.clone();
            recorder.user = user.to_owned();
            recorder.http_port = address.http_port;
            recorder.rtsp_port = address.rtsp_port;
        } else {
            self.saved.recorders.push(Recorder {
                id,
                name: name.to_owned(),
                host: address.host.clone(),
                user: user.to_owned(),
                http_port: address.http_port,
                rtsp_port: address.rtsp_port,
            });
        }
        self.write().inspect_err(|_| self.saved = before)
    }

    /// Takes a recorder out of the list and the layout, leaving its password to the caller.
    pub fn forget(&mut self, id: u64) -> Result<(), Error> {
        let before = self.saved.clone();
        // A list saved before next_id existed has to remember the id going away too.
        self.saved.next_id = self.saved.next_id.max(id + 1);
        self.saved.recorders.retain(|r| r.id != id);
        if let Some(layout) = &mut self.saved.layout {
            layout.remove_recorder(id);
        }
        self.write().inspect_err(|_| self.saved = before)
    }

    pub fn layout(&self) -> Option<&Layout> {
        self.saved.layout.as_ref()
    }

    pub fn set_layout(&mut self, layout: Layout) {
        self.saved.layout = Some(layout);
        self.save_later();
    }

    pub fn language(&self) -> Option<&str> {
        self.saved.language.as_deref()
    }

    pub fn set_language(&mut self, language: &str) {
        self.saved.language = Some(language.to_owned());
        self.save_later();
    }

    pub fn checks_for_updates(&self) -> bool {
        self.saved.check_updates != Some(false)
    }

    pub fn theme(&self) -> Option<&str> {
        self.saved.theme.as_deref()
    }

    pub fn set_theme(&mut self, theme: &str) {
        self.saved.theme = Some(theme.to_owned());
        self.save_later();
    }

    pub fn sidebar_collapsed(&self) -> bool {
        self.saved.sidebar_collapsed
    }

    pub fn set_sidebar_collapsed(&mut self, collapsed: bool) {
        self.saved.sidebar_collapsed = collapsed;
        self.save_later();
    }

    /// Where a save done by `set_layout` and the other settings reports its failure, on the
    /// thread that saves. Without it the failure is lost.
    pub fn on_save_error(&self, handler: impl Fn(Error) + Send + Sync + 'static) {
        self.saver.queue.lock().on_error = Some(Arc::new(handler));
    }

    fn save_later(&mut self) {
        let bytes = match serde_json::to_vec_pretty(&self.saved) {
            Ok(bytes) => bytes,
            Err(error) => {
                let handler = self.saver.queue.lock().on_error.clone();
                if let Some(handler) = handler {
                    handler(error.into());
                }
                return;
            }
        };
        self.saver.queue.lock().pending = Some(bytes);
        self.saver.queue.changed.notify_all();
        if self.saver.thread.is_none() {
            let queue = Arc::clone(&self.saver.queue);
            let path = self.path.clone();
            self.saver.thread = Some(std::thread::spawn(move || queue.run(&path)));
        }
    }

    /// Saves now, for the changes that have to be undone when saving fails. A save still waiting
    /// is dropped, since this one has everything it had, and one already running ends first, so
    /// an older copy can't land after this one.
    fn write(&self) -> Result<(), Error> {
        let queue = &self.saver.queue;
        let mut state = queue.lock();
        state.pending = None;
        while state.saving {
            state = queue.wait(state);
        }
        // Holding the lock keeps the thread away from the temporary file.
        let result = serde_json::to_vec_pretty(&self.saved)
            .map_err(Error::from)
            .and_then(|bytes| save(&self.path, &bytes));
        drop(state);
        result
    }
}

impl Drop for RecorderList {
    /// Waits for the last change to reach the disk, so closing the app doesn't lose it.
    fn drop(&mut self) {
        let Some(thread) = self.saver.thread.take() else {
            return;
        };
        self.saver.queue.lock().closing = true;
        self.saver.queue.changed.notify_all();
        let _ = thread.join();
    }
}

fn save(path: &Path, bytes: &[u8]) -> Result<(), Error> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
        restrict_folder(parent)?;
    }
    write_atomically(path, bytes)
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
            self.0.borrow().get(&id).cloned().ok_or(Error::NoPassword)
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

    struct UndeletableSecrets;

    impl Secrets for UndeletableSecrets {
        fn set(&self, _: u64, _: &str) -> Result<(), Error> {
            Ok(())
        }
        fn get(&self, _: u64) -> Result<String, Error> {
            Err(Error::Keyring("locked".to_owned()))
        }
        fn delete(&self, _: u64) -> Result<(), Error> {
            Err(Error::Keyring("locked".to_owned()))
        }
    }

    fn add(list: &mut RecorderList, name: &str, address: &Address) -> u64 {
        let id = list.reserve_id();
        list.store(id, name, address, "u").unwrap();
        id
    }

    fn addr(host: &str) -> Address {
        Address::parse(host, None, None).unwrap()
    }

    fn temp_path(test: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("rigelcam-{test}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        dir.join("recorders.json")
    }

    #[test]
    fn password_never_reaches_the_file() {
        let path = temp_path("password");
        let secrets = MemorySecrets::default();
        let mut list = RecorderList::open(path.clone()).unwrap();
        let id = list.reserve_id();
        put_password(&secrets, id, "s3cret-value").unwrap();
        list.store(id, "Depot", &addr("192.0.2.1"), "viewer")
            .unwrap();
        assert_eq!(secrets.get(id).unwrap(), "s3cret-value");
        let file = fs::read_to_string(&path).unwrap();
        assert!(file.contains("192.0.2.1"));
        assert!(!file.contains("s3cret-value"));
    }

    #[test]
    fn recorders_layout_and_sidebar_survive_reopening() {
        let path = temp_path("reopen");
        let mut list = RecorderList::open(path.clone()).unwrap();
        let first = add(&mut list, "A", &addr("192.0.2.1"));
        add(&mut list, "B", &addr("192.0.2.2"));
        let layout = Layout::fitting(&[CameraId {
            recorder: first,
            channel: 1,
        }]);
        list.set_layout(layout.clone());
        list.set_sidebar_collapsed(true);
        list.set_theme("light");
        list.set_language("en");
        drop(list);
        let reopened = RecorderList::open(path).unwrap();
        assert_eq!(reopened.theme(), Some("light"));
        assert_eq!(reopened.language(), Some("en"));
        assert_eq!(reopened.recorders().len(), 2);
        assert_eq!(reopened.layout(), Some(&layout));
        assert!(reopened.sidebar_collapsed());
    }

    #[test]
    fn saving_a_recorder_keeps_a_setting_still_waiting_to_be_saved() {
        let path = temp_path("ordered");
        let mut list = RecorderList::open(path.clone()).unwrap();
        for _ in 0..50 {
            list.set_theme("light");
            add(&mut list, "A", &addr("192.0.2.1"));
        }
        list.set_language("en");
        drop(list);
        let reopened = RecorderList::open(path).unwrap();
        assert_eq!(reopened.recorders().len(), 50);
        assert_eq!(reopened.theme(), Some("light"));
        assert_eq!(reopened.language(), Some("en"));
    }

    #[test]
    fn a_setting_that_fails_to_save_reports_it() {
        let path = temp_path("setting-unsaved");
        let mut list = RecorderList::open(path.clone()).unwrap();
        fs::create_dir_all(path.with_extension("json.tmp")).unwrap();
        let (sender, receiver) = std::sync::mpsc::channel();
        list.on_save_error(move |error| {
            let _ = sender.send(error.to_string());
        });
        list.set_theme("light");
        let message = receiver
            .recv_timeout(std::time::Duration::from_secs(10))
            .unwrap();
        assert!(!message.is_empty());
        drop(list);
        assert!(!path.exists());
    }

    #[test]
    fn removing_deletes_the_password_and_empties_its_squares() {
        let path = temp_path("remove");
        let secrets = MemorySecrets::default();
        let mut list = RecorderList::open(path).unwrap();
        let first = add(&mut list, "A", &addr("192.0.2.1"));
        let second = add(&mut list, "B", &addr("192.0.2.2"));
        put_password(&secrets, second, "p").unwrap();
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
        list.set_layout(layout);
        assert_eq!(
            take_password(&secrets, second),
            (Previous::Password("p".to_owned()), true)
        );
        list.forget(second).unwrap();
        assert!(secrets.get(second).is_err());
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
        let list = RecorderList::open(path).unwrap();
        assert_eq!(list.recorders().len(), 1);
        assert_eq!(list.recorders()[0].http_port, 80);
        assert_eq!(list.recorders()[0].rtsp_port, 554);
        assert_eq!(list.layout(), None);
        assert!(!list.sidebar_collapsed());
    }

    #[test]
    fn updating_keeps_the_id_and_hands_back_the_old_password() {
        let path = temp_path("update");
        let secrets = MemorySecrets::default();
        let mut list = RecorderList::open(path).unwrap();
        let id = list.reserve_id();
        assert_eq!(put_password(&secrets, id, "p").unwrap(), Previous::Nothing);
        list.store(id, "A", &addr("192.0.2.1"), "u").unwrap();
        list.store(id, "B", &addr("192.0.2.2"), "v").unwrap();
        assert_eq!(secrets.get(id).unwrap(), "p");
        assert_eq!(
            put_password(&secrets, id, "q").unwrap(),
            Previous::Password("p".to_owned())
        );
        let recorder = &list.recorders()[0];
        assert_eq!(list.recorders().len(), 1);
        assert_eq!(recorder.id, id);
        assert_eq!(recorder.name, "B");
        assert_eq!(recorder.host, "192.0.2.2");
        assert_eq!(recorder.user, "v");
    }

    #[test]
    fn failing_to_save_undoes_the_update() {
        let path = temp_path("update-unsaved");
        let secrets = MemorySecrets::default();
        let mut list = RecorderList::open(path.clone()).unwrap();
        let id = add(&mut list, "A", &addr("192.0.2.1"));
        put_password(&secrets, id, "p").unwrap();
        fs::create_dir(path.with_extension("json.tmp")).unwrap();
        let previous = put_password(&secrets, id, "q").unwrap();
        assert!(list.store(id, "B", &addr("192.0.2.2"), "v").is_err());
        restore_password(&secrets, id, &previous);
        assert_eq!(list.recorders()[0].name, "A");
        assert_eq!(secrets.get(id).unwrap(), "p");
    }

    #[test]
    fn removing_works_without_the_keyring_and_tells_the_password_stayed() {
        let path = temp_path("undeletable");
        let mut list = RecorderList::open(path).unwrap();
        let id = add(&mut list, "A", &addr("192.0.2.1"));
        assert!(!take_password(&UndeletableSecrets, id).1);
        list.forget(id).unwrap();
        assert!(list.recorders().is_empty());
    }

    #[test]
    fn address_accepts_what_people_paste() {
        let parsed = |text| Address::parse(text, None, None).unwrap();
        assert_eq!(parsed("  192.0.2.1 ").host, "192.0.2.1");
        assert_eq!(parsed("nvr-1.local").host, "nvr-1.local");
        assert_eq!(parsed("http://192.0.2.1/").host, "192.0.2.1");
        assert_eq!(parsed("http://192.0.2.1/login.html").host, "192.0.2.1");
        assert_eq!(parsed("[2001:db8::1]").host, "[2001:db8::1]");
        assert_eq!(parsed("[2001:db8::1]:8080").http_port, 8080);
        assert_eq!(parsed("192.0.2.1:8080").http_port, 8080);
        assert_eq!(parsed("http://192.0.2.1:8080/").host, "192.0.2.1");
    }

    #[test]
    fn port_in_the_address_wins_and_missing_ports_are_the_factory_ones() {
        let address = Address::parse("192.0.2.1:8080", Some(81), Some(8554)).unwrap();
        assert_eq!((address.http_port, address.rtsp_port), (8080, 8554));
        let address = Address::parse("192.0.2.1", Some(81), None).unwrap();
        assert_eq!((address.http_port, address.rtsp_port), (81, 554));
        let address = Address::parse("192.0.2.1", None, None).unwrap();
        assert_eq!((address.http_port, address.rtsp_port), (80, 554));
    }

    #[test]
    fn address_refuses_credentials_and_junk() {
        let refused = |text| Address::parse(text, None, None).unwrap_err();
        assert_eq!(refused("admin:secret@192.0.2.1"), AddressError::Credentials);
        assert_eq!(
            refused("http://admin@192.0.2.1/"),
            AddressError::Credentials
        );
        assert_eq!(refused("https://192.0.2.1/"), AddressError::Https);
        for text in [
            "",
            "http://",
            "192.0.2.1 nvr",
            "192.0.2.1:0",
            "192.0.2.1:99999",
        ] {
            assert_eq!(refused(text), AddressError::Invalid, "{text}");
        }
        assert_eq!(refused("2001:db8::1"), AddressError::Invalid);
    }

    #[test]
    fn ports_survive_reopening_and_stay_out_of_the_file_when_they_are_the_default() {
        let path = temp_path("ports");
        let mut list = RecorderList::open(path.clone()).unwrap();
        add(&mut list, "A", &addr("192.0.2.1"));
        assert!(!fs::read_to_string(&path).unwrap().contains("port"));
        let custom = Address::parse("192.0.2.2:8080", None, Some(8554)).unwrap();
        add(&mut list, "B", &custom);
        let reopened = RecorderList::open(path).unwrap();
        assert_eq!(reopened.recorders()[1].http_port, 8080);
        assert_eq!(reopened.recorders()[1].rtsp_port, 8554);
    }

    #[test]
    fn starting_over_keeps_the_damaged_file_and_opens_an_empty_list() {
        let path = temp_path("start-over");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, "not json").unwrap();
        let (list, copy) = RecorderList::start_over(path.clone()).unwrap();
        assert!(copy.exists());
        assert!(list.saved.next_id > 1);
        assert!(list.recorders().is_empty());
        assert!(copy.to_string_lossy().contains("recorders.json.corrupted-"));
        assert_eq!(fs::read_to_string(&copy).unwrap(), "not json");
        // Without saving anything else, so the passwords of the old ids stay safe from new ones.
        let reopened = RecorderList::open(path).unwrap();
        assert_eq!(reopened.saved.next_id, list.saved.next_id);
    }

    #[test]
    fn stores_a_reserved_id_without_touching_passwords() {
        let path = temp_path("store");
        let secrets = MemorySecrets::default();
        let mut list = RecorderList::open(path.clone()).unwrap();
        let id = list.reserve_id();
        assert_ne!(list.reserve_id(), id);
        list.store(id, "Altermed", &addr("10.0.0.1"), "admin")
            .unwrap();
        list.store(id, "Conferencia", &addr("10.0.0.1"), "admin")
            .unwrap();
        assert!(secrets.0.borrow().is_empty());
        let reopened = RecorderList::open(path).unwrap();
        assert_eq!(reopened.recorders().len(), 1);
        assert_eq!(reopened.recorders()[0].name, "Conferencia");
    }

    #[test]
    fn forgets_a_recorder_without_touching_passwords() {
        let path = temp_path("forget");
        let secrets = MemorySecrets::default();
        let mut list = RecorderList::open(path).unwrap();
        let id = add(&mut list, "Altermed", &addr("10.0.0.1"));
        put_password(&secrets, id, "pw").unwrap();
        list.forget(id).unwrap();
        assert!(list.recorders().is_empty());
        assert_eq!(secrets.get(id).unwrap(), "pw");
    }

    #[test]
    fn ids_never_come_back() {
        let path = temp_path("ids");
        let mut list = RecorderList::open(path).unwrap();
        let first = add(&mut list, "A", &addr("192.0.2.1"));
        list.forget(first).unwrap();
        assert_ne!(add(&mut list, "B", &addr("192.0.2.2")), first);
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
        let mut list = RecorderList::open(path).unwrap();
        list.forget(5).unwrap();
        assert_eq!(list.reserve_id(), 6);
    }

    #[test]
    fn failing_to_save_undoes_adding_and_removing() {
        let path = temp_path("unsaved");
        let secrets = MemorySecrets::default();
        let mut list = RecorderList::open(path.clone()).unwrap();
        let id = add(&mut list, "A", &addr("192.0.2.1"));
        put_password(&secrets, id, "p").unwrap();
        // A folder in the way of the temporary file makes every save fail.
        fs::create_dir(path.with_extension("json.tmp")).unwrap();
        let new = list.reserve_id();
        let previous = put_password(&secrets, new, "p2").unwrap();
        assert!(list.store(new, "B", &addr("192.0.2.2"), "u").is_err());
        restore_password(&secrets, new, &previous);
        assert_eq!(list.recorders().len(), 1);
        assert!(secrets.get(new).is_err());
        let (password, _) = take_password(&secrets, id);
        assert!(list.forget(id).is_err());
        restore_password(&secrets, id, &password);
        assert_eq!(list.recorders().len(), 1);
        assert_eq!(secrets.get(id).unwrap(), "p");
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
        let list = RecorderList::open(path).unwrap();
        let layout = list.layout().unwrap();
        assert_eq!((layout.columns, layout.cameras.len()), (6, 36));
    }

    #[cfg(unix)]
    #[test]
    fn saved_list_is_private() {
        use std::os::unix::fs::PermissionsExt;
        let path = temp_path("private");
        let mut list = RecorderList::open(path.clone()).unwrap();
        add(&mut list, "A", &addr("192.0.2.1"));
        let mode = |p: &Path| fs::metadata(p).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode(&path), 0o600);
        assert_eq!(mode(path.parent().unwrap()), 0o700);
    }

    /// Reads fail as if the keyring were locked, while writes work.
    #[derive(Clone, Default)]
    struct UnreadableSecrets(MemorySecrets);

    impl Secrets for UnreadableSecrets {
        fn set(&self, id: u64, password: &str) -> Result<(), Error> {
            self.0.set(id, password)
        }
        fn get(&self, _: u64) -> Result<String, Error> {
            Err(Error::Keyring("locked".to_owned()))
        }
        fn delete(&self, id: u64) -> Result<(), Error> {
            self.0.delete(id)
        }
    }

    #[test]
    fn undoing_after_an_unreadable_keyring_keeps_the_new_password() {
        let secrets = UnreadableSecrets::default();
        let previous = put_password(&secrets, 1, "new").unwrap();
        assert_eq!(previous, Previous::Unknown);
        restore_password(&secrets, 1, &previous);
        assert_eq!(secrets.0.get(1).unwrap(), "new");
    }

    #[test]
    fn keyring_failure_adds_nothing() {
        let path = temp_path("keyring");
        assert!(put_password(&FailingSecrets, 1, "p").is_err());
        assert!(!path.exists());
    }

    #[test]
    fn corrupted_file_is_an_error_and_is_not_overwritten() {
        let path = temp_path("corrupted");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, "not json").unwrap();
        assert!(matches!(
            RecorderList::open(path.clone()),
            Err(Error::Format(_))
        ));
        assert_eq!(fs::read_to_string(&path).unwrap(), "not json");
    }
}
