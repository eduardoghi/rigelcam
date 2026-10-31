//! Client for the Dahua HTTP API, which answers plain `key=value` lines behind Digest auth.

use std::time::Duration;

use digest_auth::AuthContext;
use ureq::http::StatusCode;

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

/// A recorder channel. `number` starts at 1, as in the RTSP URLs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Channel {
    pub number: u32,
    pub name: String,
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
}

impl Client {
    pub fn new(host: &str, user: &str, password: &str) -> Self {
        // Status codes are handled here because a 401 is the normal first step of Digest.
        let agent = ureq::Agent::config_builder()
            .http_status_as_error(false)
            .timeout_global(Some(Duration::from_secs(15)))
            .build()
            .into();
        Self {
            host: host.to_owned(),
            http_port: 80,
            rtsp_port: 554,
            user: user.to_owned(),
            password: password.to_owned(),
            agent,
        }
    }

    pub fn device_type(&self) -> Result<String, Error> {
        let body = self.get("/cgi-bin/magicBox.cgi?action=getDeviceType")?;
        parse_value(&body, "type")
            .map(str::to_owned)
            .ok_or_else(|| Error::Unexpected(body.clone()))
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
        }
        Ok(channels)
    }

    /// RTSP address of the live stream. It has no credentials, so it is safe to log.
    pub fn live_url(&self, channel: u32, stream: Stream) -> String {
        let subtype = match stream {
            Stream::Main => 0,
            Stream::Sub => 1,
        };
        format!(
            "rtsp://{}:{}/cam/realmonitor?channel={channel}&subtype={subtype}",
            self.host, self.rtsp_port
        )
    }

    fn get(&self, path: &str) -> Result<String, Error> {
        let url = format!("http://{}:{}{path}", self.host, self.http_port);
        let mut response = self.agent.get(&url).call()?;
        if response.status() == StatusCode::UNAUTHORIZED {
            let challenge = response
                .headers()
                .get("www-authenticate")
                .and_then(|value| value.to_str().ok())
                .ok_or_else(|| Error::Challenge("no WWW-Authenticate header".to_owned()))?
                .to_owned();
            let mut prompt =
                digest_auth::parse(&challenge).map_err(|e| Error::Challenge(e.to_string()))?;
            let context = AuthContext::new(self.user.as_str(), self.password.as_str(), path);
            let answer = prompt
                .respond(&context)
                .map_err(|e| Error::Challenge(e.to_string()))?
                .to_header_string();
            response = self
                .agent
                .get(&url)
                .header("Authorization", answer)
                .call()?;
        }
        match response.status() {
            StatusCode::OK => Ok(response.body_mut().read_to_string()?),
            StatusCode::UNAUTHORIZED => Err(Error::Unauthorized),
            other => Err(Error::Status(other.as_u16())),
        }
    }
}

fn parse_value<'a>(body: &'a str, key: &str) -> Option<&'a str> {
    body.lines()
        .filter_map(|line| line.trim_end().split_once('='))
        .find_map(|(k, v)| (k == key).then_some(v))
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
                number: index.parse::<u32>().ok()? + 1,
                name: name.to_owned(),
            })
        })
        .collect();
    channels.sort_by_key(|channel| channel.number);
    channels
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
                    number: 1,
                    name: "Gate = East".to_owned()
                },
                Channel {
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
