//! Client for the Dahua HTTP API, which answers plain `key=value` lines behind Digest auth.

use std::collections::HashMap;
use std::io::Read;
use std::time::{Duration, Instant};

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use digest_auth::{AuthContext, WwwAuthenticateHeader};
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

/// Files asked for at a time, which is about two days of a channel recording all the time.
const SEARCH_PAGE: usize = 100;

/// A recorder channel. `number` starts at 1, as in the RTSP URLs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Channel {
    pub number: u32,
    pub name: String,
    /// Where the camera of this channel answers, when the recorder tells.
    pub camera: Option<Camera>,
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

#[derive(Default)]
struct LoginState {
    checked: Option<Instant>,
    refused: bool,
}

/// Decides whether the streams of a recorder may try to log in. `rtspsrc` repeats a wrong password
/// by itself and every camera has its own, so a changed password would reach the recorder hundreds
/// of times at once and can lock the user. One HTTP login answers for all of them.
pub struct LoginGate {
    client: Client,
    state: Mutex<LoginState>,
}

impl LoginGate {
    pub fn new(client: Client) -> Self {
        Self {
            client,
            state: Mutex::default(),
        }
    }

    /// Records a login that just worked on this recorder.
    pub fn confirm(&self) {
        self.state.lock().unwrap().checked = Some(Instant::now());
    }

    /// Stops all further logins, after a stream was refused.
    pub fn refuse(&self) {
        self.state.lock().unwrap().refused = true;
    }

    /// Whether a stream may log in now. The lock stays held during the check, so cameras opening
    /// together wait for one answer. A recorder that doesn't answer lets them through, since it
    /// can't count a login that doesn't reach it.
    pub fn allows(&self) -> bool {
        let mut state = self.state.lock().unwrap();
        if state.refused {
            return false;
        }
        if state
            .checked
            .is_some_and(|checked| checked.elapsed() < LOGIN_CHECK_INTERVAL)
        {
            return true;
        }
        if matches!(self.client.clock(), Err(Error::Unauthorized)) {
            state.refused = true;
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

    /// Lists every channel the recorder has, including the ones with no camera connected.
    pub fn channels(&self) -> Result<Vec<Channel>, Error> {
        let body = self.get("/cgi-bin/configManager.cgi?action=getConfig&name=ChannelTitle")?;
        let mut channels = parse_channel_titles(&body);
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
            let cameras = parse_cameras(&devices);
            for channel in &mut channels {
                channel.camera = cameras.get(&channel.number).cloned();
            }
        }
        Ok(channels)
    }

    /// Current time of the recorder as `YYYY-MM-DD HH:MM:SS`, which can differ from this computer
    /// and is the one its recordings use.
    pub fn clock(&self) -> Result<String, Error> {
        let body = self.get("/cgi-bin/global.cgi?action=getCurrentTime")?;
        parse_value(&body, "result")
            .map(str::to_owned)
            .ok_or_else(|| Error::Unexpected(body.clone()))
    }

    /// Lists what a channel recorded on a day given as `YYYY-MM-DD`. The recorder answers an error
    /// to a search that finds nothing, so an old day comes back empty.
    pub fn recordings(&self, channel: u32, date: &str) -> Result<Vec<Span>, Error> {
        let created = self.get("/cgi-bin/mediaFileFind.cgi?action=factory.create")?;
        let finder = parse_value(&created, "result")
            .ok_or_else(|| Error::Unexpected(created.clone()))?
            .to_owned();
        let result = self.search_files(&finder, channel, date);
        // Recorders allow few searches at a time, so this one is closed whatever happened.
        let base = "/cgi-bin/mediaFileFind.cgi?action";
        let _ = self.get(&format!("{base}=close&object={finder}"));
        let _ = self.get(&format!("{base}=destroy&object={finder}"));
        result
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
        let created = self.get("/cgi-bin/mediaFileFind.cgi?action=factory.create")?;
        let finder = parse_value(&created, "result")
            .ok_or_else(|| Error::Unexpected(created.clone()))?
            .to_owned();
        let mut result = Ok(());
        for date in dates {
            let answer = self.find_files(&finder, channel, date).and_then(|any| {
                if !any {
                    return Ok(false);
                }
                let body = self.get(&format!(
                    "/cgi-bin/mediaFileFind.cgi?action=findNextFile&object={finder}&count=1"
                ))?;
                Ok(!parse_files(&body).is_empty())
            });
            match answer {
                Ok(recorded) => {
                    if !found(date, recorded) {
                        break;
                    }
                }
                Err(error) => {
                    result = Err(error);
                    break;
                }
            }
        }
        let base = "/cgi-bin/mediaFileFind.cgi?action";
        let _ = self.get(&format!("{base}=close&object={finder}"));
        let _ = self.get(&format!("{base}=destroy&object={finder}"));
        result
    }

    /// Starts a search. The recorder answers an error to one that finds nothing.
    fn find_files(&self, finder: &str, channel: u32, date: &str) -> Result<bool, Error> {
        let path = format!(
            "/cgi-bin/mediaFileFind.cgi?action=findFile&object={finder}&condition.Channel={channel}\
             &condition.StartTime={date}%2000:00:00&condition.EndTime={date}%2023:59:59"
        );
        match self.get(&path) {
            Ok(_) => Ok(true),
            Err(Error::Status(400)) => Ok(false),
            Err(error) => Err(error),
        }
    }

    fn search_files(&self, finder: &str, channel: u32, date: &str) -> Result<Vec<Span>, Error> {
        if !self.find_files(finder, channel, date)? {
            return Ok(Vec::new());
        }
        let mut spans = Vec::new();
        loop {
            let body = self.get(&format!(
                "/cgi-bin/mediaFileFind.cgi?action=findNextFile&object={finder}\
                 &count={SEARCH_PAGE}"
            ))?;
            let files = parse_files(&body);
            let found = files.len();
            spans.extend(files.iter().filter_map(|file| file.span_on(date)));
            if found < SEARCH_PAGE {
                break;
            }
        }
        Ok(merge_spans(spans))
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
                .build()
        };
        let mut pending = request();
        if let Some(answer) = answer {
            pending = pending.header("Authorization", answer);
        }
        let mut response = pending.call()?;
        if response.status() == StatusCode::UNAUTHORIZED {
            let challenge = response
                .headers()
                .get("www-authenticate")
                .and_then(|value| value.to_str().ok())
                .ok_or_else(|| Error::Challenge("no WWW-Authenticate header".to_owned()))?
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

fn seconds_of(time: &str) -> Option<u32> {
    let (_, clock) = time.split_once(' ')?;
    let mut parts = clock.split(':').map(|part| part.parse::<u32>().ok());
    Some(parts.next()?? * 3600 + parts.next()?? * 60 + parts.next()??)
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
            Some(Channel {
                camera: None,
                number: index.parse::<u32>().ok()? + 1,
                name: name.to_owned(),
            })
        })
        .collect();
    channels.sort_by_key(|channel| channel.number);
    channels
}

/// The address of each channel that has one, from the fields of its entry.
fn parse_cameras(body: &str) -> HashMap<u32, Camera> {
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
        let Ok(channel) = index.parse::<u32>().map(|index| index + 1) else {
            continue;
        };
        if let Some(host) = field.strip_prefix("Address=") {
            if !host.is_empty() && host != "0.0.0.0" {
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
            Some(index.parse::<u32>().ok()? + 1)
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

    #[test]
    fn reads_where_each_camera_answers() {
        let body = "table.RemoteDevice.uuid:System_CONFIG_NETCAMERA_INFO_0.Address=192.0.2.1\r\n\
                    table.RemoteDevice.uuid:System_CONFIG_NETCAMERA_INFO_0.RtspPort=8554\r\n\
                    table.RemoteDevice.uuid:System_CONFIG_NETCAMERA_INFO_1.Address=192.0.2.2\r\n\
                    table.RemoteDevice.uuid:System_CONFIG_NETCAMERA_INFO_2.Address=0.0.0.0\r\n\
                    table.RemoteDevice.uuid:System_CONFIG_NETCAMERA_INFO_3.Address=\r\n";
        let cameras = parse_cameras(body);
        assert_eq!(cameras.len(), 2);
        assert_eq!(
            cameras[&1].live_url(Stream::Sub),
            "rtsp://192.0.2.1:8554/cam/realmonitor?channel=1&subtype=1"
        );
        assert_eq!(cameras[&2].rtsp_port, 554);
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
                    number: 1,
                    name: "Gate = East".to_owned()
                },
                Channel {
                    camera: None,
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
