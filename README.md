<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="assets/branding/logo-dark.svg">
    <img src="assets/branding/logo-light.svg" alt="rigelcam" height="80">
  </picture>
</p>

<p align="center">English · <a href="README.pt-BR.md">Português</a></p>

rigelcam is a desktop app for watching live cameras and browsing recordings on Intelbras and Dahua
NVRs, for Linux and Windows.

It talks to the recorder over the protocols the device already exposes, RTSP for video and the
Dahua HTTP API for everything else, so it does not need the vendor SDK and runs natively on Linux.

> [!NOTE]
> Early stage. There is no usable build yet. The protocol work below is confirmed on real devices,
> the app around it is being written.

## Planned features

**Live view**

- Grid of cameras from one or more NVRs, using the substream so a full grid stays light
- Double click to open a single camera on the main stream
- Channel names read from the recorder

**Recordings**

- Timeline per channel for any day, with motion events marked apart from continuous recording
- Click anywhere on the timeline to play from that moment
- Several channels played back in sync
- Fast forward, if the recorder honors it (still being tested)
- Export a clip to MP4

## How it works

| What | How |
| --- | --- |
| Live video | RTSP, `/cam/realmonitor?channel=N&subtype=0\|1` |
| Recordings list | HTTP Digest, `mediaFileFind.cgi` |
| Playback | RTSP, `/cam/playback?channel=N&starttime=...&endtime=...` |
| Device and channel info | HTTP Digest, `magicBox.cgi` and `configManager.cgi` |

Video is decoded by GStreamer with hardware acceleration where available (VA-API on Linux,
Direct3D 11 on Windows). The core is written in Rust and the interface in Qt 6 Quick.

Tested so far with the Intelbras NVD 1308 and NVD 7132. Other Dahua based recorders should work
the same way, reports are welcome.

Remote access through the Intelbras cloud (P2P) is out of scope. Use a VPN to reach the recorder
from outside its network.

## Credentials

Recorder passwords are kept in the system keyring (Secret Service on Linux, Credential Manager on
Windows) and never written to config files or logs.

## Contributing

Enable the commit message and formatting checks once per clone:

```sh
git config core.hooksPath .githooks
```

## License

[MIT](LICENSE). The Space Grotesk font used in the logo is under the
[SIL Open Font License](assets/branding/SpaceGrotesk-OFL.txt).

## Disclaimer

rigelcam is an independent project, not affiliated with or endorsed by Intelbras or Dahua.
Intelbras and Dahua are trademarks of their respective owners.
