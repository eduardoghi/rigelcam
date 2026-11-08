# Changelog

User-visible changes, newest first.

## [Unreleased]

### Added

- A button in the toolbar hides the names over the cameras of the grid, for cameras that print
  their own on the picture. A hidden name still shows under the mouse.
- The timeline of the recordings shows the time under the mouse, with a thin line on the bar.
- A list of the events of the day, for every open camera with its time, name and length, opens
  from the bar of the recordings. A click plays the event.
- The password of a recorder can be shown while typing it, to check it before saving, since a
  wrong one tried a few times can lock the user out of the recorder.

### Changed

- A more sober look, with neutral gray surfaces, a blue accent and a new logo, smaller corners,
  live and recordings as two halves of one control and the grid size as a row of buttons.
- An empty square of the grid shows a faint plus, and its hint only under the mouse or while a
  camera is dragged. About, the theme and full screen moved to a menu at the end of the toolbar.

### Fixed

- Shortcuts of the player and Ctrl+S no longer act behind an open dialog, list or menu, such as
  About, which the space bar used to pause the recording under.
- The list of the events of today keeps its place when the recordings are checked again each
  minute, instead of going back to the top.
- Closing the app while a camera was still closing could crash it on the way out, in the cleanup
  of the graphics driver.
- A recorder that answers without its list of channels is tried five times and then waits for
  the user, instead of every minute, which added a login to its log each time.
- On Windows, an update that failed to start can be tried again without reopening the app.
- A frame the decoder hands over that can't be read now shows as a camera without picture,
  instead of a black square that counts as live. On Windows, shaders that fail to compile no
  longer close the app.
- Marks that turn text around, such as U+202E, are replaced in the names of exported videos, so
  a camera name can't show a file name backwards.
- A recorder list edited by hand with two recorders on one id, or an id the interface can't hold,
  opens without them, since they would share a password in the keyring. A folder that refuses
  its permissions no longer stops the list from being saved.

## [0.1.4] (2026-11-07)

### Added

- More than one grid, each with its name, cameras and size. They switch from the toolbar or
  with Ctrl+PgUp and Ctrl+PgDown, and can take turns on their own every 10 s, 30 s or minute.
  The grid saved by an older version becomes the first one.
- Buttons next to -10 and +10, and Ctrl+Left and Ctrl+Right, jump to the previous or the next
  event of the recordings, such as motion or an alarm.
- The Windows package and the Flatpak carry the license notices of the code they bundle, from
  the Rust crates compiled into the app and, on Windows, from Qt, GStreamer and what comes with them.

### Security

- Reading a recording over HTTP, as exports do, holds about 64 MB of video at most while it checks
  the start of each stretch, so a recorder that sends frames of 1 ms each can't fill the memory.
- A camera the app opens directly, for a channel the recorder doesn't stream over RTSP, has to be
  in the same /24 network as the recorder, so a recorder can't hand its password to another host.

### Fixed

- Exports and photos of a camera with a long name in a script such as Chinese no longer fail to
  save because the file name is too long.
- A recorder that doesn't offer the login session searches try first is no longer asked for it
  again on every search.
- A camera moved, or a theme or language changed, just before saving a recorder that fails to save
  is no longer lost.
- A camera open alone stops using the network and the decoder 20 seconds after the window is
  minimized, like the cameras of the grid. Entering the recordings stops the live cameras at once.
- A minimized window frees the video memory of its stopped cameras, instead of holding it until
  the window comes back. With 31 cameras it went from about 760 MB to 260 MB.
- A camera without signal shows its state instead of the last picture, and frees the memory of the
  video while it waits to reconnect.
- The window no longer freezes while a slow recorder answers a login check.
- Moving a camera or changing the columns, the theme, the language or the side bar no longer
  pauses the window while the disk saves the list.
- A day sent by the recorder only goes into file names and URLs when it is a plain date.
- A cancel that arrives just after an export ends no longer cancels the next one.
- A wrong password found by the permission check of an export isn't tried again by the download.
- Saving or removing a recorder no longer freezes the window while the keyring is locked.
- The calendar only asks the recorder for days that exist.
- An export checks the permission of the camera first, also for a camera that was never played.
  The answer holds for 10 minutes, so exporting the same camera again adds no login to the recorder.
- A download stops 60 seconds after the recorder goes silent, instead of holding its connection.
- Starting over after a damaged list of recorders can't reuse an id with a saved password.
- `rigelcam-cli` cleans what the recorder sends before printing an error.
- Recordings of cameras from different recorders follow the clock of the one that is behind, so
  near midnight the next day and the part still to come don't follow a recorder that is ahead.
- Cameras sending 10 bit H.265 video can be shown, without RTX on Windows.
- The recordings of a day are read until the recorder sends an empty page, so a firmware with
  smaller pages no longer cuts the list short.
- Updating while a video is being exported waits for the export to end, instead of closing the app
  and losing the video.
- Editing a recorder no longer deletes its password when the keyring can't be read and the list
  then fails to save.
- An export no longer leaves out its last frames when the video happens to hold the mark the
  recorder uses between frames.
- "Open folder" after an export works when the path of the videos has `#` or `?`.
- A camera an export found denied is asked again on the next one, so a permission given later on
  the recorder counts without reopening the app.
- Days in the calendar and in the recordings no longer depend on the digits of the system language.
- On Windows, a graphics card that can't decode the cameras gets a message saying so, instead of
  one about plugins that come with the app.

## [0.1.3] (2026-11-02)

### Added

- A Flatpak bundle for Linux, with hardware decoding through VA-API. It doesn't update by itself,
  and it keeps passwords in an encrypted file of its own, away from the keyring of the session.

## [0.1.2] (2026-11-02)

### Added

- An About window with the installed version and links to the releases and to the repository.
- RTX Super Resolution on an open camera and on a recording of one camera, for Windows with an
  NVIDIA RTX card. It is off by default and photos keep the original picture.
- RTX HDR at the same places, for an RTX card with HDR on in Windows. The window opens in HDR on
  those computers, and the button works right away.
- Both RTX options also have to be on in the NVIDIA App. Otherwise the picture stays the same.

## [0.1.1] (2026-11-01)

### Added

- The executable has the Rigelcam icon.

### Fixed

- The notice of a new version has a solid background, so the cameras under it don't get in the way
  of reading.
- A failed check for a new version goes to the log, and it was silent before.

## [0.1.0] (2026-11-01)

First release, for Windows x64.

- Live grid of cameras from one or more recorders, with a large view, digital zoom and gamma.
- Recordings by day, with a calendar, a timeline and up to 4 cameras in sync.
- Export of a clip to MP4 and photos at the resolution of the video.
- English, Portuguese and Spanish, with dark and light themes.
- Updates from inside the app, which ask before installing and check a signature.
