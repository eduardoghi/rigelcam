//! Client for the Dahua HTTP API, which answers plain `key=value` lines behind Digest auth.

use std::cell::Cell;
use std::collections::HashMap;
use std::io::Read;
use std::net::{IpAddr, Ipv4Addr, ToSocketAddrs};
use std::time::{Duration, Instant};

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use digest_auth::{AuthContext, WwwAuthenticateHeader};
use md5::{Digest, Md5};
use serde_json::{json, Value};
use ureq::http::{Response, StatusCode};
use ureq::Body;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("request failed ({0})")]
    Http(#[from] ureq::Error),
    #[error("the recorder rejected the user or password")]
    Unauthorized,
    #[error("the recorder answered HTTP {0}")]
    Status(u16),
    #[error("the recorder sent an invalid Digest challenge ({0})")]
    Challenge(String),
    #[error("unexpected answer from the recorder ({0})")]
    Unexpected(String),
}

const DEFAULT_RTSP_PORT: u16 = 554;

/// For requests with no limit on their total time, which still have to start answering.
const CONNECTION_TIMEOUT: Duration = Duration::from_secs(15);

/// More than any recorder holds, so that one that keeps answering full pages can't hold a search.
const MAX_SEARCH_PAGES: usize = 1_000;
/// What a session answers to the search of a day with nothing recorded.
const SEARCH_FOUND_NOTHING: i64 = 285_409_284;
const MAX_CHANNELS: usize = 512;
const MAX_NAME_CHARS: usize = 120;

/// Files asked for at a time, which is about two days of a channel recording all the time.
const SEARCH_PAGE: usize = 100;

/// A recorder channel. `number` starts at 1, as in the RTSP URLs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Channel {
    pub number: u32,
    pub name: String,
    /// Where the camera of this channel answers, when the recorder tells.
    pub camera: Option<Camera>,
    /// Width and height of the main stream, when the recorder tells. The substream can't give the
    /// shape of the picture, since recorders send it at sizes like 704x480 for a 16:9 camera and
    /// don't mark its pixels as wider than tall.
    pub main_size: Option<(u32, u32)>,
}

/// Network address of the camera behind a channel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Camera {
    pub host: String,
    pub rtsp_port: u16,
}

impl Camera {
    /// RTSP address of its live stream, in which a camera is always channel 1.
    pub fn live_url(&self, stream: Stream) -> String {
        live_url(&self.host, self.rtsp_port, 1, stream)
    }
}

fn live_url(host: &str, port: u16, channel: u32, stream: Stream) -> String {
    let subtype = match stream {
        Stream::Main => 0,
        Stream::Sub => 1,
    };
    format!("rtsp://{host}:{port}/cam/realmonitor?channel={channel}&subtype={subtype}")
}

/// How long a confirmed login spares the next opening of a stream from checking again. Long enough
/// for a whole grid to open, and short enough for a password changed since.
const LOGIN_CHECK_INTERVAL: Duration = Duration::from_secs(5);
/// A recorder quiet for this long in the middle of a stretch stopped sending it.
const STALLED_READ_TIMEOUT: Duration = Duration::from_secs(60);

#[derive(Default)]
struct LoginState {
    checked: Option<Instant>,
}

/// Decides whether the streams of a recorder may try to log in. `rtspsrc` repeats a wrong password
/// by itself and every camera has its own, so a changed password would reach the recorder hundreds
/// of times at once and can lock the user. One HTTP login answers for all of them.
pub struct LoginGate {
    client: Client,
    state: Mutex<LoginState>,
    // Apart from the lock, which a check holds while the recorder answers, so the interface can
    // read it without waiting.
    refused: AtomicBool,
}

impl LoginGate {
    pub fn new(client: Client) -> Self {
        Self {
            client,
            state: Mutex::default(),
            refused: AtomicBool::new(false),
        }
    }

    /// Records a login that just worked on this recorder.
    pub fn confirm(&self) {
        self.state.lock().unwrap().checked = Some(Instant::now());
    }

    /// Stops all further logins, after a stream was refused.
    pub fn refuse(&self) {
        self.refused.store(true, Ordering::Relaxed);
    }

    pub fn is_refused(&self) -> bool {
        self.refused.load(Ordering::Relaxed)
    }

    /// Whether a stream may log in now. The lock stays held during the check, so cameras opening
    /// together wait for one answer. A recorder that doesn't answer lets them through, since it
    /// can't count a login that doesn't reach it.
    pub fn allows(&self) -> bool {
        let mut state = self.state.lock().unwrap();
        // Also after the lock, since another check may have been refused while this one waited.
        if self.is_refused() {
            return false;
        }
        if state
            .checked
            .is_some_and(|checked| checked.elapsed() < LOGIN_CHECK_INTERVAL)
        {
            return true;
        }
        if matches!(self.client.clock(), Err(Error::Unauthorized)) {
            self.refuse();
            return false;
        }
        state.checked = Some(Instant::now());
        true
    }
}

/// A continuous recording of a day, in seconds since its midnight on the clock of the recorder.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    pub start: u32,
    pub end: u32,
    /// Saved because of an event, such as motion, and not just the regular schedule.
    pub event: bool,
}

/// Which of the two streams of a channel to open.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stream {
    Main,
    Sub,
}

pub struct Client {
    host: String,
    http_port: u16,
    rtsp_port: u16,
    user: String,
    password: String,
    agent: ureq::Agent,
    /// The last Digest challenge, answered again without asking for a new one. Otherwise every
    /// request costs two round trips, which adds up in a search of many days.
    challenge: Mutex<Option<WwwAuthenticateHeader>>,
    /// The recorder refused the password, so this client stops talking to it.
    refused: AtomicBool,
    /// The session login failed on this recorder, or the recorder doesn't offer it, for this and
    /// for later clients. Each try would count as a wrong password, even where the CGI login works,
    /// or cost a request that can't work.
    /// Its lock stays held during a login, so streams asking together spend one wrong password.
    sessions_refused: Arc<Mutex<bool>>,
}

impl Client {
    pub fn new(host: &str, user: &str, password: &str) -> Self {
        // Status codes are handled here because a 401 is the normal first step of Digest.
        let agent = ureq::Agent::config_builder()
            .http_status_as_error(false)
            // A redirect drops the Authorization header, so following one would make the right
            // password look refused.
            .max_redirects(0)
            // A recorder sits on the local network, and a proxy of the system would only see its
            // address and user, or block it. RTSP doesn't go through one either.
            .proxy(None)
            .timeout_global(Some(Duration::from_secs(15)))
            .build()
            .into();
        Self {
            host: host.to_owned(),
            http_port: 80,
            rtsp_port: DEFAULT_RTSP_PORT,
            user: user.to_owned(),
            password: password.to_owned(),
            challenge: Mutex::new(None),
            refused: AtomicBool::new(false),
            sessions_refused: Arc::default(),
            agent,
        }
    }

    pub fn device_type(&self) -> Result<String, Error> {
        let body = self.get("/cgi-bin/magicBox.cgi?action=getDeviceType")?;
        parse_value(&body, "type")
            .map(str::to_owned)
            .ok_or_else(|| Error::Unexpected(body.clone()))
    }

    /// Points at ports other than the HTTP 80 and RTSP 554 of a factory recorder.
    pub fn with_ports(mut self, http_port: u16, rtsp_port: u16) -> Self {
        self.http_port = http_port;
        self.rtsp_port = rtsp_port;
        self
    }

    /// Shares with the other clients of a recorder whether its session login failed.
    pub fn with_session_memory(mut self, memory: Arc<Mutex<bool>>) -> Self {
        self.sessions_refused = memory;
        self
    }

    /// Lists every channel the recorder has, including the ones with no camera connected.
    pub fn channels(&self) -> Result<Vec<Channel>, Error> {
        let body = self.get("/cgi-bin/configManager.cgi?action=getConfig&name=ChannelTitle")?;
        let mut channels = parse_channel_titles(&body);
        channels.truncate(MAX_CHANNELS);
        // Some models answer an error with status 200, and that can't look like an empty recorder.
        if channels.is_empty() {
            return Err(Error::Unexpected("the channel list is empty".to_owned()));
        }
        // Not every model or user can read this table, and then all channels stay listed.
        if let Ok(devices) =
            self.get("/cgi-bin/configManager.cgi?action=getConfig&name=RemoteDevice")
        {
            let disabled = parse_disabled_cameras(&devices);
            channels.retain(|channel| !disabled.contains(&channel.number));
            let cameras = parse_cameras(&devices, &self.addresses());
            for channel in &mut channels {
                channel.camera = cameras.get(&channel.number).cloned();
            }
        }
        if let Ok(encode) = self.get("/cgi-bin/configManager.cgi?action=getConfig&name=Encode") {
            let sizes = parse_main_sizes(&encode);
            for channel in &mut channels {
                channel.main_size = sizes.get(&channel.number).copied();
            }
        }
        Ok(channels)
    }

    /// The IPv4 addresses the host of the recorder stands for, as the system resolves it.
    fn addresses(&self) -> Vec<Ipv4Addr> {
        (self.host.as_str(), self.http_port)
            .to_socket_addrs()
            .map(|addresses| {
                addresses
                    .filter_map(|address| match address.ip() {
                        IpAddr::V4(ip) => Some(ip),
                        IpAddr::V6(_) => None,
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Current time of the recorder as `YYYY-MM-DD HH:MM:SS`, which can differ from this computer
    /// and is the one its recordings use.
    pub fn clock(&self) -> Result<String, Error> {
        let body = self.get("/cgi-bin/global.cgi?action=getCurrentTime")?;
        parse_value(&body, "result")
            .map(str::to_owned)
            .ok_or_else(|| Error::Unexpected(body.clone()))
    }

    /// Whether the recorder is connected to the camera of a channel, or `None` if it can't tell.
    pub fn camera_connected(&self, channel: u32) -> Option<bool> {
        let session = Session::open(self)?;
        let params = json!({ "uniqueChannels": [-1] });
        let answer = session
            .call("LogicDeviceManager.getCameraState", params, &Value::Null)
            .ok()?;
        connection_state(&answer, channel)
    }

    /// Lists what a channel recorded on a day given as `YYYY-MM-DD`. An old day comes back empty.
    pub fn recordings(&self, channel: u32, date: &str) -> Result<Vec<Span>, Error> {
        let finder = Finder::open(self)?;
        if !finder.find(channel, date)? {
            return Ok(Vec::new());
        }
        let mut spans = Vec::new();
        for _ in 0..MAX_SEARCH_PAGES {
            let files = finder.next(SEARCH_PAGE)?;
            let found = files.len();
            spans.extend(files.iter().filter_map(|file| file.span_on(date)));
            // Only an empty page ends the list, since a firmware may answer pages smaller than
            // asked for with more still to come. It costs one more request at the end.
            if found == 0 {
                return Ok(merge_spans(spans));
            }
        }
        Err(Error::Unexpected("too many recordings".to_owned()))
    }

    /// Tells which of the given days, as `YYYY-MM-DD`, have recordings of a channel, one at a time
    /// through `found`, which returns false to stop. They share one search on the recorder,
    /// because opening one per day takes more than twice as long.
    pub fn recorded_days<'a>(
        &self,
        channel: u32,
        dates: impl IntoIterator<Item = &'a str>,
        mut found: impl FnMut(&str, bool) -> bool,
    ) -> Result<(), Error> {
        let finder = Finder::open(self)?;
        for date in dates {
            let recorded = finder.find(channel, date)? && !finder.next(1)?.is_empty();
            if !found(date, recorded) {
                break;
            }
        }
        Ok(())
    }

    /// RTSP address of a recording, from `seconds` after the midnight of `date` until that day
    /// ends. The time goes in the address because the positions of the stream count only
    /// recorded time, so seeking with them would land later after every gap. Recordings exist
    /// only in the main stream, and asking for the substream is an error.
    pub fn playback_url(&self, channel: u32, date: &str, seconds: u32) -> String {
        let day = date.replace('-', "_");
        let (hours, minutes, seconds) = (seconds / 3600, seconds / 60 % 60, seconds % 60);
        format!(
            "rtsp://{}:{}/cam/playback?channel={channel}\
             &starttime={day}_{hours:02}_{minutes:02}_{seconds:02}&endtime={day}_23_59_59",
            self.host, self.rtsp_port
        )
    }

    /// Opens a stretch of a recording as a `.dav` stream, between two times in seconds after the
    /// midnight of `date`. The recorder sends it much faster than real time. Nothing limits how
    /// long the reading takes, so whoever reads has to notice when it stops.
    pub fn download(
        &self,
        channel: u32,
        date: &str,
        start: u32,
        end: u32,
    ) -> Result<Box<dyn Read + Send>, Error> {
        let clock = |seconds: u32| {
            let (hours, minutes, seconds) = (seconds / 3600, seconds / 60 % 60, seconds % 60);
            format!("{hours:02}:{minutes:02}:{seconds:02}")
        };
        let path = format!(
            "/cgi-bin/loadfile.cgi?action=startLoad&channel={channel}\
             &startTime={date}%20{}&endTime={date}%20{}&subtype=0",
            clock(start),
            clock(end)
        );
        let response = self.send(&path, true)?;
        Ok(Box::new(response.into_body().into_reader()))
    }

    /// RTSP address of the live stream. It has no credentials, so it is safe to log.
    pub fn live_url(&self, channel: u32, stream: Stream) -> String {
        live_url(&self.host, self.rtsp_port, channel, stream)
    }

    fn get(&self, path: &str) -> Result<String, Error> {
        Ok(self.send(path, false)?.body_mut().read_to_string()?)
    }

    fn post_rpc(&self, path: &str, request: &Value) -> Result<Value, Error> {
        let url = format!("http://{}:{}{path}", self.host, self.http_port);
        let mut response = self
            .agent
            .post(&url)
            .header("Content-Type", "application/json")
            .send(request.to_string())?;
        if response.status() != StatusCode::OK {
            return Err(Error::Status(response.status().as_u16()));
        }
        let body = response.body_mut().read_to_string()?;
        serde_json::from_str(&body).map_err(|_| {
            let start: String = body.chars().take(200).collect();
            Error::Unexpected(start)
        })
    }

    fn send(&self, path: &str, long_body: bool) -> Result<Response<Body>, Error> {
        // Recorders lock a user after a few wrong passwords, and a search makes many requests.
        if self.refused.load(Ordering::Relaxed) {
            return Err(Error::Unauthorized);
        }
        let url = format!("http://{}:{}{path}", self.host, self.http_port);
        let saved = self.challenge.lock().unwrap().take();
        let mut prompt = saved;
        let answer = prompt
            .as_mut()
            .and_then(|prompt| self.answer(prompt, path).ok());
        let request = || {
            let request = self.agent.get(&url);
            if !long_body {
                return request;
            }
            request
                .config()
                .timeout_global(None)
                .timeout_connect(Some(CONNECTION_TIMEOUT))
                .timeout_recv_response(Some(CONNECTION_TIMEOUT))
                // Each wait, so a slow reader or a pause doesn't count, only a silent recorder.
                .timeout_per_read(Some(STALLED_READ_TIMEOUT))
                .build()
        };
        let mut pending = request();
        if let Some(answer) = answer {
            pending = pending.header("Authorization", answer);
        }
        let mut response = pending.call()?;
        if response.status() == StatusCode::UNAUTHORIZED {
            // A recorder can offer other schemes first, and only Digest is answered.
            let challenge = response
                .headers()
                .get_all("www-authenticate")
                .iter()
                .filter_map(|value| value.to_str().ok())
                .find(|value| {
                    value
                        .trim_start()
                        .to_ascii_lowercase()
                        .starts_with("digest")
                })
                .ok_or_else(|| Error::Challenge("no Digest challenge".to_owned()))?
                .to_owned();
            let mut fresh =
                digest_auth::parse(&challenge).map_err(|e| Error::Challenge(e.to_string()))?;
            let answer = self.answer(&mut fresh, path)?;
            prompt = Some(fresh);
            response = request().header("Authorization", answer).call()?;
        }
        match response.status() {
            StatusCode::OK | StatusCode::BAD_REQUEST => {
                // Only a challenge that worked is worth keeping.
                *self.challenge.lock().unwrap() = prompt;
            }
            StatusCode::UNAUTHORIZED => {
                self.refused.store(true, Ordering::Relaxed);
                return Err(Error::Unauthorized);
            }
            _ => {}
        }
        match response.status() {
            StatusCode::OK => Ok(response),
            other => Err(Error::Status(other.as_u16())),
        }
    }

    fn answer(&self, prompt: &mut WwwAuthenticateHeader, path: &str) -> Result<String, Error> {
        let context = AuthContext::new(self.user.as_str(), self.password.as_str(), path);
        prompt
            .respond(&context)
            .map(|answer| answer.to_header_string())
            .map_err(|e| Error::Challenge(e.to_string()))
    }
}

/// A login session of the API the web interface uses, which is undocumented. Recorders log every
/// CGI request as a login and hold few log entries, so a month of a calendar would push out days of
/// history, while the calls of a session leave nothing in the log.
struct Session<'a> {
    client: &'a Client,
    id: Value,
    calls: Cell<u32>,
}

impl<'a> Session<'a> {
    /// Logs in, or returns None when the recorder doesn't offer this or refuses it, which the CGI
    /// path then answers for.
    fn open(client: &'a Client) -> Option<Self> {
        let mut sessions_refused = client.sessions_refused.lock().unwrap();
        if client.refused.load(Ordering::Relaxed) || *sessions_refused {
            return None;
        }
        let login = |password: &str, session: &Value| {
            let params = json!({
                "userName": client.user,
                "password": password,
                "clientType": "Web3.0",
                "authorityType": "Default",
                "passwordType": "Default",
            });
            client.post_rpc(
                "/RPC2_Login",
                &json!({"method": "global.login", "params": params, "id": 1, "session": session}),
            )
        };
        let challenge = match login("", &Value::Null) {
            Ok(challenge) => challenge,
            // The network can come back, and a busy recorder can answer later.
            Err(Error::Http(_) | Error::Status(500..)) => return None,
            // Not found, not JSON or any other refusal of the first step, which asks for nothing
            // yet, so the recorder doesn't offer this.
            Err(_) => {
                *sessions_refused = true;
                return None;
            }
        };
        let Some(params) = challenge
            .get("params")
            .filter(|params| params.get("encryption").and_then(Value::as_str) == Some("Default"))
        else {
            *sessions_refused = true;
            return None;
        };
        let realm = params.get("realm")?.as_str()?;
        let random = params.get("random")?.as_str()?;
        let hash = login_hash(&client.user, &client.password, realm, random);
        let session = challenge.get("session")?;
        let answer = login(&hash, session).ok()?;
        if answer.get("result")?.as_bool() != Some(true) {
            *sessions_refused = true;
            return None;
        }
        Some(Self {
            client,
            id: answer.get("session").unwrap_or(session).clone(),
            calls: Cell::new(1),
        })
    }

    fn call(&self, method: &str, params: Value, object: &Value) -> Result<Value, Error> {
        self.calls.set(self.calls.get() + 1);
        self.client.post_rpc(
            "/RPC2",
            &rpc_request(method, params, self.calls.get(), &self.id, object),
        )
    }
}

/// The body of a call. Recorders answer `Invalid Request` to some methods that get a null object,
/// so a call without one doesn't carry the field.
fn rpc_request(method: &str, params: Value, id: u32, session: &Value, object: &Value) -> Value {
    let mut request = json!({"method": method, "params": params, "id": id, "session": session});
    if !object.is_null() {
        request["object"] = object.clone();
    }
    request
}

impl Drop for Session<'_> {
    fn drop(&mut self) {
        let _ = self.call("global.logout", Value::Null, &Value::Null);
    }
}

/// `MD5(user:random:MD5(user:realm:password))`, in uppercase hexadecimal.
fn login_hash(user: &str, password: &str, realm: &str, random: &str) -> String {
    let md5 = |text: String| {
        Md5::digest(text.as_bytes())
            .iter()
            .fold(String::new(), |hex, byte| hex + &format!("{byte:02X}"))
    };
    md5(format!(
        "{user}:{random}:{}",
        md5(format!("{user}:{realm}:{password}"))
    ))
}

/// Reads a channel from the states of all cameras, where channels start at 0.
fn connection_state(answer: &Value, channel: u32) -> Option<bool> {
    let index = channel.checked_sub(1)?;
    let state = answer
        .pointer("/params/states")?
        .as_array()?
        .iter()
        .find(|state| state.get("channel").and_then(Value::as_u64) == Some(u64::from(index)))?;
    Some(state.get("connectionState")?.as_str()? == "Connected")
}

fn session_error(answer: &Value) -> Error {
    Error::Unexpected(answer.get("error").unwrap_or(&Value::Null).to_string())
}

/// Whether `date` is a day as `YYYY-MM-DD`. Days come from the clock of the recorder and end up
/// in file names and URLs, so nothing else gets through.
pub fn is_date(date: &str) -> bool {
    let bytes = date.as_bytes();
    bytes.len() == 10
        && bytes.iter().enumerate().all(|(i, &b)| {
            if i == 4 || i == 7 {
                b == b'-'
            } else {
                b.is_ascii_digit()
            }
        })
}

/// Whether `now` is the clock of a recorder as `YYYY-MM-DD HH:MM:SS`.
pub fn is_clock(now: &str) -> bool {
    let Some((date, time)) = now.split_once(' ') else {
        return false;
    };
    let time = time.as_bytes();
    is_date(date)
        && time.len() == 8
        && time.iter().enumerate().all(|(i, &b)| {
            if i == 2 || i == 5 {
                b == b':'
            } else {
                b.is_ascii_digit()
            }
        })
}

/// How many days a month of the form `YYYY-MM` has.
pub fn days_in_month(month: &str) -> u32 {
    let (year, month) = month.split_once('-').unwrap_or_default();
    let year: u32 = year.parse().unwrap_or(0);
    match month.parse().unwrap_or(0) {
        2 if year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400)) => {
            29
        }
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

fn answered_true(answer: &Value) -> bool {
    answer.get("result").and_then(Value::as_bool) == Some(true)
}

/// Tells a day with nothing recorded from a search that failed, which would be remembered as an
/// empty day.
fn search_found_files(answer: &Value) -> Result<bool, Error> {
    if answered_true(answer) {
        return Ok(true);
    }
    match answer.pointer("/error/code").and_then(Value::as_i64) {
        Some(SEARCH_FOUND_NOTHING) => Ok(false),
        _ => Err(session_error(answer)),
    }
}

/// The end of a list answers true with no files, so false can't be taken for it.
fn files_of(answer: &Value) -> Result<Vec<RecordedFile>, Error> {
    if !answered_true(answer) {
        return Err(session_error(answer));
    }
    let time = |file: &Value, field: &str| {
        file.get(field)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned()
    };
    Ok(answer
        .pointer("/params/infos")
        .and_then(Value::as_array)
        .map(|files| {
            files
                .iter()
                .map(|file| RecordedFile {
                    start: time(file, "StartTime"),
                    end: time(file, "EndTime"),
                    event: file
                        .get("Flags")
                        .and_then(Value::as_array)
                        .is_some_and(|flags| flags.iter().any(|flag| flag != "Timing")),
                })
                .collect()
        })
        .unwrap_or_default())
}

/// A search on the recorder, closed when dropped because recorders allow few at a time.
enum Finder<'a> {
    Session { session: Session<'a>, object: Value },
    Cgi { client: &'a Client, object: String },
}

impl<'a> Finder<'a> {
    fn open(client: &'a Client) -> Result<Self, Error> {
        if let Some(session) = Session::open(client) {
            let created = session.call("mediaFileFind.factory.create", Value::Null, &Value::Null);
            if let Some(object) = created
                .ok()
                .and_then(|answer| answer.get("result").cloned())
            {
                if object.as_u64().is_some_and(|object| object != 0) {
                    return Ok(Self::Session { session, object });
                }
            }
        }
        let created = client.get("/cgi-bin/mediaFileFind.cgi?action=factory.create")?;
        let object = parse_value(&created, "result")
            .ok_or_else(|| Error::Unexpected(created.clone()))?
            .to_owned();
        Ok(Self::Cgi { client, object })
    }

    /// Starts a search of a day and tells whether it found anything.
    fn find(&self, channel: u32, date: &str) -> Result<bool, Error> {
        match self {
            Self::Session { session, object } => {
                let condition = json!({
                    // Starts at 0 here, unlike everywhere else.
                    "Channel": channel.saturating_sub(1),
                    "StartTime": format!("{date} 00:00:00"),
                    "EndTime": format!("{date} 23:59:59"),
                    "Types": ["dav"],
                    "VideoStream": "Main",
                });
                let params = json!({ "condition": condition });
                let answer = session.call("mediaFileFind.findFile", params, object)?;
                search_found_files(&answer)
            }
            Self::Cgi { client, object } => {
                let path = format!(
                    "/cgi-bin/mediaFileFind.cgi?action=findFile&object={object}\
                     &condition.Channel={channel}&condition.StartTime={date}%2000:00:00\
                     &condition.EndTime={date}%2023:59:59"
                );
                // The recorder answers an error to a search that finds nothing.
                match client.get(&path) {
                    Ok(_) => Ok(true),
                    Err(Error::Status(400)) => Ok(false),
                    Err(error) => Err(error),
                }
            }
        }
    }

    fn next(&self, count: usize) -> Result<Vec<RecordedFile>, Error> {
        match self {
            Self::Session { session, object } => {
                let params = json!({ "count": count });
                let answer = session.call("mediaFileFind.findNextFile", params, object)?;
                files_of(&answer)
            }
            Self::Cgi { client, object } => {
                let body = client.get(&format!(
                    "/cgi-bin/mediaFileFind.cgi?action=findNextFile&object={object}&count={count}"
                ))?;
                Ok(parse_files(&body))
            }
        }
    }
}

impl Drop for Finder<'_> {
    fn drop(&mut self) {
        match self {
            Self::Session { session, object } => {
                let _ = session.call("mediaFileFind.close", Value::Null, object);
                let _ = session.call("mediaFileFind.destroy", Value::Null, object);
            }
            Self::Cgi { client, object } => {
                let base = "/cgi-bin/mediaFileFind.cgi?action";
                let _ = client.get(&format!("{base}=close&object={object}"));
                let _ = client.get(&format!("{base}=destroy&object={object}"));
            }
        }
    }
}

fn parse_value<'a>(body: &'a str, key: &str) -> Option<&'a str> {
    body.lines()
        .filter_map(|line| line.trim_end().split_once('='))
        .find_map(|(k, v)| (k == key).then_some(v))
}

/// One entry of a search, with times as `YYYY-MM-DD HH:MM:SS`.
#[derive(Debug, Default, PartialEq, Eq)]
struct RecordedFile {
    start: String,
    end: String,
    event: bool,
}

/// Seconds after midnight of a time such as `2026-10-30 13:05:00`. 24:00:00 is the end of the day,
/// and anything past it isn't a time.
fn seconds_of(time: &str) -> Option<u32> {
    let (_, clock) = time.split_once(' ')?;
    let mut parts = clock.split(':').map(|part| part.parse::<u8>().ok());
    let (hours, minutes, seconds) = (parts.next()??, parts.next()??, parts.next()??);
    let total = u32::from(hours) * 3600 + u32::from(minutes) * 60 + u32::from(seconds);
    (minutes < 60 && seconds < 60 && total <= 86_400).then_some(total)
}

/// The channel number of an index the recorder counts from 0, within the channels a recorder has.
fn channel_of(index: &str) -> Option<u32> {
    let index = index.parse::<u32>().ok()?;
    // A closure, since then_some would add 1 before the check and overflow on u32::MAX.
    ((index as usize) < MAX_CHANNELS).then(|| index + 1)
}

impl RecordedFile {
    /// The part of this file on a day, since a file can cross midnight.
    fn span_on(&self, date: &str) -> Option<Span> {
        let day_of = |time: &str| time.split(' ').next().unwrap_or_default().to_owned();
        // The format sorts like the dates it holds.
        if day_of(&self.end).as_str() < date || day_of(&self.start).as_str() > date {
            return None;
        }
        let on_day = |time: &str| time.starts_with(date);
        let start = if on_day(&self.start) {
            seconds_of(&self.start)?
        } else {
            0
        };
        let end = if on_day(&self.end) {
            seconds_of(&self.end)?
        } else {
            86_400
        };
        (start < end).then_some(Span {
            start,
            end,
            event: self.event,
        })
    }
}

fn parse_files(body: &str) -> Vec<RecordedFile> {
    let mut files: Vec<(u32, RecordedFile)> = Vec::new();
    for line in body.lines() {
        let Some((key, value)) = line.trim_end().split_once('=') else {
            continue;
        };
        let Some(rest) = key.strip_prefix("items[") else {
            continue;
        };
        let Some((index, field)) = rest.split_once("].") else {
            continue;
        };
        let Ok(index) = index.parse::<u32>() else {
            continue;
        };
        if files.last().is_none_or(|(last, _)| *last != index) {
            files.push((index, RecordedFile::default()));
        }
        let file = &mut files.last_mut().unwrap().1;
        match field {
            "StartTime" => file.start = value.to_owned(),
            "EndTime" => file.end = value.to_owned(),
            _ if field.starts_with("Flags[") && value != "Timing" => file.event = true,
            _ => {}
        }
    }
    files.into_iter().map(|(_, file)| file).collect()
}

/// Joins recordings that touch, since recorders split a continuous day in files of minutes.
fn merge_spans(mut spans: Vec<Span>) -> Vec<Span> {
    spans.sort_by_key(|span| span.start);
    let mut merged: Vec<Span> = Vec::new();
    for span in spans {
        match merged.last_mut() {
            Some(last) if last.event == span.event && span.start <= last.end + 1 => {
                last.end = last.end.max(span.end);
            }
            _ => merged.push(span),
        }
    }
    merged
}

fn parse_channel_titles(body: &str) -> Vec<Channel> {
    let mut channels: Vec<Channel> = body
        .lines()
        .filter_map(|line| {
            let (key, name) = line.trim_end().split_once('=')?;
            let index = key
                .strip_prefix("table.ChannelTitle[")?
                .strip_suffix("].Name")?;
            let name: String = name.chars().take(MAX_NAME_CHARS).collect();
            Some(Channel {
                camera: None,
                main_size: None,
                number: channel_of(index)?,
                name,
            })
        })
        .collect();
    channels.sort_by_key(|channel| channel.number);
    channels
}

/// The size of the main stream of each channel, from lines like
/// `table.Encode[1].MainFormat[0].Video.resolution=1920x1080`, where channels start at 0.
fn parse_main_sizes(body: &str) -> HashMap<u32, (u32, u32)> {
    let mut sizes = HashMap::new();
    for line in body.lines() {
        let Some((index, size)) = line
            .trim_end()
            .strip_prefix("table.Encode[")
            .and_then(|field| field.split_once("].MainFormat[0].Video.resolution="))
        else {
            continue;
        };
        let parsed = channel_of(index).zip(size.split_once('x').and_then(|(width, height)| {
            Some((width.parse::<u32>().ok()?, height.parse::<u32>().ok()?))
        }));
        if let Some((channel, (width, height))) = parsed.filter(|(_, (_, height))| *height > 0) {
            sizes.insert(channel, (width, height));
        }
    }
    sizes
}

/// The address of each channel that has one, from the fields of its entry. It ends up in a URL that
/// gets the password of the recorder, so only a private address in the /24 of the recorder counts,
/// and a recorder that names another host can't send the password away from its network.
fn parse_cameras(body: &str, recorder: &[Ipv4Addr]) -> HashMap<u32, Camera> {
    let near = |ip: Ipv4Addr| {
        ip.is_private()
            && recorder
                .iter()
                .any(|recorder| recorder.octets()[..3] == ip.octets()[..3])
    };
    let mut hosts = HashMap::new();
    let mut ports = HashMap::new();
    for line in body.lines() {
        let Some(field) = line
            .trim_end()
            .strip_prefix("table.RemoteDevice.uuid:System_CONFIG_NETCAMERA_INFO_")
        else {
            continue;
        };
        let Some((index, field)) = field.split_once('.') else {
            continue;
        };
        let Some(channel) = channel_of(index) else {
            continue;
        };
        if let Some(host) = field.strip_prefix("Address=") {
            if host.parse::<Ipv4Addr>().is_ok_and(near) {
                hosts.insert(channel, host.to_owned());
            }
        } else if let Some(port) = field.strip_prefix("RtspPort=") {
            ports.insert(channel, port.parse().unwrap_or(DEFAULT_RTSP_PORT));
        }
    }
    hosts
        .into_iter()
        .map(|(channel, host)| {
            let rtsp_port = ports.get(&channel).copied().unwrap_or(DEFAULT_RTSP_PORT);
            (channel, Camera { host, rtsp_port })
        })
        .collect()
}

/// Channels with no camera assigned. Their titles are still listed, as "CAM 32" for example.
fn parse_disabled_cameras(body: &str) -> Vec<u32> {
    body.lines()
        .filter_map(|line| {
            let index = line
                .trim_end()
                .strip_prefix("table.RemoteDevice.uuid:System_CONFIG_NETCAMERA_INFO_")?
                .strip_suffix(".Enable=false")?;
            channel_of(index)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const FILES: &str = "found=3\r\n\
        items[0].Channel=2\r\n\
        items[0].StartTime=2026-10-29 23:55:00\r\n\
        items[0].EndTime=2026-10-30 00:08:22\r\n\
        items[0].Flags[0]=Timing\r\n\
        items[1].StartTime=2026-10-30 00:08:22\r\n\
        items[1].EndTime=2026-10-30 00:20:00\r\n\
        items[1].Flags[0]=Timing\r\n\
        items[2].StartTime=2026-10-30 10:00:00\r\n\
        items[2].EndTime=2026-10-30 10:05:30\r\n\
        items[2].Flags[0]=Event\r\n";

    #[test]
    fn days_come_as_merged_spans_cut_at_midnight() {
        let files = parse_files(FILES);
        assert_eq!(files.len(), 3);
        let spans = files
            .iter()
            .filter_map(|f| f.span_on("2026-10-30"))
            .collect();
        assert_eq!(
            merge_spans(spans),
            vec![
                Span {
                    start: 0,
                    end: 1200,
                    event: false
                },
                Span {
                    start: 36_000,
                    end: 36_330,
                    event: true
                },
            ]
        );
    }

    #[test]
    fn a_file_on_another_day_adds_nothing() {
        let file = RecordedFile {
            start: "2026-10-29 10:00:00".to_owned(),
            end: "2026-10-29 10:05:00".to_owned(),
            event: false,
        };
        assert_eq!(file.span_on("2026-10-30"), None);
    }

    #[test]
    fn times_and_channels_past_their_range_are_left_out() {
        assert_eq!(seconds_of("2026-10-30 24:00:00"), Some(86_400));
        assert_eq!(seconds_of("2026-10-30 24:59:59"), None);
        assert_eq!(channel_of("0"), Some(1));
        assert_eq!(channel_of("4294967295"), None);
        assert_eq!(channel_of(&MAX_CHANNELS.to_string()), None);
    }

    #[test]
    fn playback_starts_at_the_time_asked_for() {
        let client = Client::new("recorder.local", "admin", "secret");
        assert_eq!(
            client.playback_url(3, "2026-10-30", 36_005),
            "rtsp://recorder.local:554/cam/playback?channel=3\
             &starttime=2026_10_30_10_00_05&endtime=2026_10_30_23_59_59"
        );
    }

    #[test]
    fn ignores_the_proxy_of_the_system() {
        let client = Client::new("recorder.local", "admin", "secret");
        assert!(client.agent.config().proxy().is_none());
    }

    #[test]
    fn a_refused_password_stops_all_further_requests() {
        let (port, requests) = fake_recorder(
            b"HTTP/1.1 401 Unauthorized\r\nConnection: close\r\nContent-Length: 0\r\n\
              WWW-Authenticate: Digest realm=\"x\", qop=\"auth\", nonce=\"n\"\r\n\r\n",
        );
        let client = Client::new("127.0.0.1", "admin", "wrong").with_ports(port, 554);
        assert!(matches!(client.clock(), Err(Error::Unauthorized)));
        let tried = requests.load(Ordering::Relaxed);
        assert!(tried <= 2, "{tried}");
        assert!(matches!(
            client.recordings(1, "2026-10-30"),
            Err(Error::Unauthorized)
        ));
        assert_eq!(requests.load(Ordering::Relaxed), tried);
    }

    /// A recorder on a local port that answers every request with `answer`, counting them.
    fn fake_recorder(
        answer: &'static [u8],
    ) -> (u16, std::sync::Arc<std::sync::atomic::AtomicUsize>) {
        use std::io::{Read, Write};

        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let requests = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let counter = requests.clone();
        std::thread::spawn(move || {
            for mut stream in listener.incoming().flatten() {
                let mut buffer = [0; 2048];
                let _ = stream.read(&mut buffer);
                counter.fetch_add(1, Ordering::Relaxed);
                let _ = stream.write_all(answer);
            }
        });
        (port, requests)
    }

    /// Answers by what the request asks for, and returns the login attempts that had a password.
    fn fake_session_recorder() -> (u16, Arc<std::sync::atomic::AtomicUsize>) {
        use std::io::{Read, Write};

        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let logins = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let counter = logins.clone();
        std::thread::spawn(move || {
            for mut stream in listener.incoming().flatten() {
                let mut buffer = [0; 2048];
                let mut size = stream.read(&mut buffer).unwrap_or(0);
                std::thread::sleep(Duration::from_millis(5));
                let _ = stream.set_nonblocking(true);
                size += stream.read(&mut buffer[size..]).unwrap_or(0);
                let request = String::from_utf8_lossy(&buffer[..size]);
                let body = if !request.starts_with("POST") {
                    "result=1\r\n".to_owned()
                } else if request.contains(r#""password":"""#) {
                    json!({"result": false, "session": "s", "params": {
                        "encryption": "Default", "realm": "r", "random": "1",
                    }})
                    .to_string()
                } else {
                    counter.fetch_add(1, Ordering::Relaxed);
                    json!({"result": false, "session": "s"}).to_string()
                };
                let _ = stream.set_nonblocking(false);
                let _ = write!(
                    stream,
                    "HTTP/1.1 200 OK\r\nConnection: close\r\nContent-Length: {}\r\n\r\n{body}",
                    body.len()
                );
            }
        });
        (port, logins)
    }

    #[test]
    fn a_refused_session_login_is_not_tried_again_by_later_clients() {
        let (port, logins) = fake_session_recorder();
        let memory: Arc<Mutex<bool>> = Arc::default();
        for _ in 0..3 {
            let client = Client::new("127.0.0.1", "admin", "secret")
                .with_ports(port, 554)
                .with_session_memory(memory.clone());
            assert!(Session::open(&client).is_none());
        }
        assert_eq!(logins.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn a_recorder_without_sessions_is_not_asked_again() {
        let (port, requests) = fake_recorder(
            b"HTTP/1.1 404 Not Found\r\nConnection: close\r\nContent-Length: 0\r\n\r\n",
        );
        let memory: Arc<Mutex<bool>> = Arc::default();
        for _ in 0..3 {
            let client = Client::new("127.0.0.1", "admin", "secret")
                .with_ports(port, 554)
                .with_session_memory(memory.clone());
            assert!(Session::open(&client).is_none());
        }
        assert_eq!(requests.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn a_recorder_out_of_reach_is_asked_again_later() {
        let port = std::net::TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port();
        let memory: Arc<Mutex<bool>> = Arc::default();
        let client = Client::new("127.0.0.1", "admin", "secret")
            .with_ports(port, 554)
            .with_session_memory(memory.clone());
        assert!(Session::open(&client).is_none());
        assert!(!*memory.lock().unwrap());
    }

    #[test]
    fn logins_starting_together_spend_one_wrong_password() {
        let (port, logins) = fake_session_recorder();
        let memory: Arc<Mutex<bool>> = Arc::default();
        let attempts: Vec<_> = (0..4)
            .map(|_| {
                let memory = memory.clone();
                std::thread::spawn(move || {
                    let client = Client::new("127.0.0.1", "admin", "secret")
                        .with_ports(port, 554)
                        .with_session_memory(memory);
                    assert!(Session::open(&client).is_none());
                })
            })
            .collect();
        for attempt in attempts {
            attempt.join().unwrap();
        }
        assert_eq!(logins.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn hashes_a_login_as_the_recorder_expects() {
        assert_eq!(
            login_hash("admin", "secret", "realm", "123"),
            "46A71A5D47175C033CE8E04E111C4492"
        );
    }

    #[test]
    fn reads_the_files_of_a_session() {
        let answer = json!({"result": true, "params": {"found": 2, "infos": [
            {"StartTime": "2026-10-30 10:00:00", "EndTime": "2026-10-30 10:30:00", "Flags": ["Timing"]},
            {"StartTime": "2026-10-30 10:30:00", "EndTime": "2026-10-30 11:00:00", "Flags": ["Event"]},
        ]}});
        let files = files_of(&answer).unwrap();
        assert_eq!(files.len(), 2);
        assert_eq!(files[0].start, "2026-10-30 10:00:00");
        assert!(!files[0].event);
        assert!(files[1].event);
        assert!(
            files_of(&json!({"result": true, "params": {"found": 0, "infos": null}}))
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn reads_the_size_of_the_main_stream_of_each_channel() {
        let body = "table.Encode[0].MainFormat[0].Video.resolution=1920x1080\r\n\
                    table.Encode[0].ExtraFormat[0].Video.resolution=704x480\r\n\
                    table.Encode[1].MainFormat[0].Video.resolution=1280x960\r\n\
                    table.Encode[2].MainFormat[0].Video.resolution=1080P\r\n\
                    table.Encode[3].MainFormat[0].Video.resolution=1920x0\r\n";
        let sizes = parse_main_sizes(body);
        assert_eq!(sizes.len(), 2);
        assert_eq!(sizes[&1], (1920, 1080));
        assert_eq!(sizes[&2], (1280, 960));
    }

    #[test]
    fn a_call_without_an_object_does_not_carry_the_field() {
        let session = json!("abc");
        let without = rpc_request("m", Value::Null, 1, &session, &Value::Null);
        assert!(without.get("object").is_none());
        let with = rpc_request("m", Value::Null, 1, &session, &json!(7));
        assert_eq!(with["object"], 7);
    }

    #[test]
    fn reads_the_connection_state_of_a_channel() {
        let answer = json!({"result": true, "params": {"states": [
            {"channel": 0, "connectionState": "Connected"},
            {"channel": 22, "connectionState": "Unconnect"},
        ]}});
        assert_eq!(connection_state(&answer, 1), Some(true));
        assert_eq!(connection_state(&answer, 23), Some(false));
        assert_eq!(connection_state(&answer, 5), None);
        assert_eq!(connection_state(&json!({"result": false}), 1), None);
    }

    #[test]
    fn a_failed_search_is_not_an_empty_day() {
        let empty = json!({"result": false, "error": {"code": 285409284, "message": ""}});
        assert!(!search_found_files(&empty).unwrap());
        assert!(search_found_files(&json!({"result": true})).unwrap());

        let expired = json!({"result": false, "error": {"code": 287637505, "message": "Invalid"}});
        assert!(matches!(
            search_found_files(&expired),
            Err(Error::Unexpected(_))
        ));
        assert!(matches!(files_of(&expired), Err(Error::Unexpected(_))));
    }

    #[test]
    fn a_recorder_without_sessions_is_searched_through_cgi() {
        let (port, requests) = fake_recorder(
            b"HTTP/1.1 404 Not Found\r\nConnection: close\r\nContent-Length: 0\r\n\r\n",
        );
        let client = Client::new("127.0.0.1", "admin", "secret").with_ports(port, 554);
        assert!(matches!(
            client.recordings(1, "2026-10-30"),
            Err(Error::Status(404))
        ));
        // The login that failed, and then the CGI search.
        assert_eq!(requests.load(Ordering::Relaxed), 2);
    }

    #[test]
    fn reads_where_each_camera_answers() {
        let body =
            "table.RemoteDevice.uuid:System_CONFIG_NETCAMERA_INFO_0.Address=192.168.0.10\r\n\
                    table.RemoteDevice.uuid:System_CONFIG_NETCAMERA_INFO_0.RtspPort=8554\r\n\
                    table.RemoteDevice.uuid:System_CONFIG_NETCAMERA_INFO_1.Address=10.0.0.2\r\n\
                    table.RemoteDevice.uuid:System_CONFIG_NETCAMERA_INFO_2.Address=0.0.0.0\r\n\
                    table.RemoteDevice.uuid:System_CONFIG_NETCAMERA_INFO_3.Address=\r\n\
                    table.RemoteDevice.uuid:System_CONFIG_NETCAMERA_INFO_4.Address=8.8.8.8\r\n\
                    table.RemoteDevice.uuid:System_CONFIG_NETCAMERA_INFO_5.Address=a@b/c\r\n\
                    table.RemoteDevice.uuid:System_CONFIG_NETCAMERA_INFO_6.Address=192.168.0.11\r\n\
                    table.RemoteDevice.uuid:System_CONFIG_NETCAMERA_INFO_7.Address=192.168.3.10\r\n";
        let cameras = parse_cameras(body, &[Ipv4Addr::new(192, 168, 0, 4)]);
        assert_eq!(cameras.len(), 2);
        assert_eq!(
            cameras[&1].live_url(Stream::Sub),
            "rtsp://192.168.0.10:8554/cam/realmonitor?channel=1&subtype=1"
        );
        assert_eq!(cameras[&7].rtsp_port, 554);
        // Without an address of the recorder, no camera can be checked against it.
        assert!(parse_cameras(body, &[]).is_empty());
    }

    #[test]
    fn a_download_returns_the_body_of_one_request() {
        let client = Client::new("127.0.0.1", "admin", "secret");
        let (port, requests) = fake_recorder(
            b"HTTP/1.1 200 OK\r\nConnection: close\r\nContent-Length: 5\r\n\r\nvideo",
        );
        let client = Client {
            http_port: port,
            ..client
        };
        let mut body = client.download(3, "2026-10-31", 3661, 3725).unwrap();
        let mut video = String::new();
        body.read_to_string(&mut video).unwrap();
        assert_eq!(video, "video");
        assert_eq!(requests.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn a_redirect_is_not_followed_and_is_not_a_refused_password() {
        let (port, requests) = fake_recorder(
            b"HTTP/1.1 301 Moved\r\nLocation: /other\r\nConnection: close\r\nContent-Length: 0\r\n\r\n",
        );
        let client = Client::new("127.0.0.1", "admin", "secret").with_ports(port, 554);
        assert!(matches!(client.clock(), Err(Error::Status(301))));
        assert_eq!(requests.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn a_refused_gate_stays_closed_without_more_requests() {
        let (port, requests) = fake_recorder(
            b"HTTP/1.1 401 Unauthorized\r\nConnection: close\r\nContent-Length: 0\r\n\
              WWW-Authenticate: Digest realm=\"x\", qop=\"auth\", nonce=\"n\"\r\n\r\n",
        );
        let client = Client::new("127.0.0.1", "admin", "wrong").with_ports(port, 554);
        let gate = LoginGate::new(client);
        assert!(!gate.allows());
        let tried = requests.load(Ordering::Relaxed);
        assert!(tried <= 2, "{tried}");
        assert!(!gate.allows());
        assert_eq!(requests.load(Ordering::Relaxed), tried);
    }

    #[test]
    fn searches_starting_together_spend_one_login_on_a_changed_password() {
        let (port, requests) = fake_recorder(
            b"HTTP/1.1 401 Unauthorized\r\nConnection: close\r\nContent-Length: 0\r\n\
              WWW-Authenticate: Digest realm=\"x\", qop=\"auth\", nonce=\"n\"\r\n\r\n",
        );
        let client = Client::new("127.0.0.1", "admin", "wrong").with_ports(port, 554);
        let gate = Arc::new(LoginGate::new(client));
        let searches: Vec<_> = (0..4)
            .map(|_| {
                let gate = gate.clone();
                std::thread::spawn(move || gate.allows())
            })
            .collect();
        for search in searches {
            assert!(!search.join().unwrap());
        }
        assert!(gate.is_refused());
        let tried = requests.load(Ordering::Relaxed);
        assert!(tried <= 2, "{tried}");
    }

    #[test]
    fn a_confirmed_gate_opens_without_asking_and_a_refusal_closes_it() {
        let (port, requests) = fake_recorder(b"HTTP/1.1 500 Error\r\nConnection: close\r\n\r\n");
        let client = Client::new("127.0.0.1", "admin", "secret").with_ports(port, 554);
        let gate = LoginGate::new(client);
        gate.confirm();
        assert!(gate.allows());
        assert_eq!(requests.load(Ordering::Relaxed), 0);
        gate.refuse();
        assert!(!gate.allows());
    }

    #[test]
    fn a_recorder_that_does_not_answer_keeps_the_gate_open() {
        let client = Client::new("127.0.0.1", "admin", "secret").with_ports(1, 554);
        assert!(LoginGate::new(client).allows());
    }

    #[test]
    fn urls_use_the_chosen_ports() {
        let client = Client::new("recorder.local", "admin", "secret").with_ports(8080, 8554);
        assert_eq!(
            client.live_url(1, Stream::Sub),
            "rtsp://recorder.local:8554/cam/realmonitor?channel=1&subtype=1"
        );
    }

    #[test]
    fn finds_channels_without_a_camera() {
        let body = "table.RemoteDevice.uuid:System_CONFIG_NETCAMERA_INFO_0.Enable=true\r\n\
                    table.RemoteDevice.uuid:System_CONFIG_NETCAMERA_INFO_0.Address=192.0.2.1\r\n\
                    table.RemoteDevice.uuid:System_CONFIG_NETCAMERA_INFO_31.Enable=false\r\n\
                    table.RemoteDevice.uuid:System_CONFIG_NETCAMERA_INFO_31.VideoInputs[0].Enable=false\r\n";
        assert_eq!(parse_disabled_cameras(body), vec![32]);
    }

    #[test]
    fn channel_titles_are_numbered_from_one_and_sorted() {
        let body = "table.ChannelTitle[1].Name=Yard\r\ntable.ChannelTitle[0].Name=Gate = East\r\n";
        assert_eq!(
            parse_channel_titles(body),
            vec![
                Channel {
                    camera: None,
                    main_size: None,
                    number: 1,
                    name: "Gate = East".to_owned()
                },
                Channel {
                    camera: None,
                    main_size: None,
                    number: 2,
                    name: "Yard".to_owned()
                },
            ]
        );
    }

    #[test]
    fn unrelated_lines_are_ignored() {
        assert!(parse_channel_titles("Error\r\nBad Request!\r\n").is_empty());
    }

    #[test]
    fn value_is_found_by_exact_key() {
        assert_eq!(parse_value("type=NVD 1308\r\n", "type"), Some("NVD 1308"));
        assert_eq!(parse_value("subtype=1\r\n", "type"), None);
    }

    #[test]
    fn only_a_plain_day_and_clock_are_taken() {
        assert!(is_date("2026-11-03"));
        assert!(!is_date("..\\..\\x"));
        assert!(!is_date("2026-11-3"));
        assert!(!is_date("2026/11/03"));
        assert!(is_clock("2026-11-03 10:00:00"));
        assert!(!is_clock("2026-11-03"));
        assert!(!is_clock("..\\x 10:00:00"));
        assert!(!is_clock("2026-11-03 10:00"));
    }

    #[test]
    fn days_in_month_follows_the_calendar() {
        assert_eq!(days_in_month("2026-09"), 30);
        assert_eq!(days_in_month("2026-10"), 31);
        assert_eq!(days_in_month("2026-02"), 28);
        assert_eq!(days_in_month("2028-02"), 29);
        assert_eq!(days_in_month("2100-02"), 28);
        assert_eq!(days_in_month("2000-02"), 29);
    }

    #[test]
    fn live_url_has_no_credentials() {
        let client = Client::new("recorder.local", "admin", "secret");
        let url = client.live_url(3, Stream::Sub);
        assert_eq!(
            url,
            "rtsp://recorder.local:554/cam/realmonitor?channel=3&subtype=1"
        );
        assert!(!url.contains("secret"));
    }
}
