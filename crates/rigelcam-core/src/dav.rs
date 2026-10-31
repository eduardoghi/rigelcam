//! Reads the `.dav` stream of a recording download and finds the video frames in it.
//!
//! A frame is the `DHAV` mark, a 24 byte header, extensions, the payload and an 8 byte trailer that
//! starts with `dhav`. Recorders send whole blocks of their disk, so a stretch comes with pieces of
//! older footage that was overwritten, some frames before it and more in place of a gap. Those are
//! valid frames of another day, and the numbering of the frames is what gives them away.
//!
//! The date inside a frame comes from the camera and can be minutes away from the clock of the
//! recorder, so it only says which frame is older, never which ones belong to the stretch. The
//! recorder itself cuts the stretch.

const MARK: &[u8; 4] = b"DHAV";
const TRAILER_MARK: &[u8; 4] = b"dhav";
const HEADER_SIZE: usize = 24;
const TRAILER_SIZE: usize = 8;
const KEY_FRAME: u8 = 0xfd;
const DELTA_FRAME: u8 = 0xfc;
/// Above any real frame, so a false mark in the middle of the data can't hold the reader back.
const MAX_FRAME_SIZE: usize = 16 * 1024 * 1024;
/// Frames in a row that make a stretch real. The leftovers before it are a fraction of a block,
/// which measured 5 frames, and they can't be told apart until the numbering breaks.
const FRAMES_TO_TRUST: usize = 64;
/// Time a frame takes when the one before it can't say, such as after a gap.
const DEFAULT_STEP_MS: u64 = 50;
/// A longer step is a pause in the recording, which the video skips.
const MAX_STEP_MS: u64 = 2_000;

/// A video frame, as its encoder wrote it. `elapsed_ms` starts at 0, without the gaps.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    pub key: bool,
    pub elapsed_ms: u64,
    pub data: Vec<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Codec {
    H264,
    H265,
}

impl Frame {
    /// The format of a key frame, which starts with the parameters of the encoder.
    pub fn codec(&self) -> Codec {
        let first_unit = match self.data.as_slice() {
            [0, 0, 0, 1, unit, ..] | [0, 0, 1, unit, ..] => *unit,
            _ => 0,
        };
        // H.265 keeps the type of a unit in other bits, and its parameters are types 32 to 34.
        if (32..=34).contains(&(first_unit >> 1 & 0x3f)) {
            Codec::H265
        } else {
            Codec::H264
        }
    }
}

struct Position {
    number: u32,
    milliseconds: u16,
}

/// Takes the stream in pieces of any size and returns the frames of the stretch.
#[derive(Default)]
pub struct Reader {
    buffer: Vec<u8>,
    previous: Option<Position>,
    /// Frames held until they are known not to be leftovers.
    held: Vec<Frame>,
    trusted: bool,
    /// A key frame was seen since the numbering broke, so the next frames can be decoded.
    decodable: bool,
    newest_date: u32,
    elapsed_ms: u64,
}

fn read_u32(bytes: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]])
}

impl Reader {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, bytes: &[u8]) -> Vec<Frame> {
        self.buffer.extend_from_slice(bytes);
        let mut frames = Vec::new();
        let mut at = 0;
        loop {
            match self.buffer[at..]
                .windows(MARK.len())
                .position(|w| w == MARK)
            {
                Some(found) => at += found,
                None => {
                    // The end can be the start of a mark that is still arriving.
                    at = at.max(self.buffer.len().saturating_sub(MARK.len() - 1));
                    break;
                }
            }
            let Some(header) = self.buffer.get(at..at + HEADER_SIZE) else {
                break;
            };
            let size = read_u32(header, 12) as usize;
            let payload_start = HEADER_SIZE + usize::from(header[22]);
            if size < payload_start + TRAILER_SIZE || size > MAX_FRAME_SIZE {
                at += MARK.len();
                continue;
            }
            let Some(frame) = self.buffer.get(at..at + size) else {
                break;
            };
            let trailer = size - TRAILER_SIZE;
            if &frame[trailer..trailer + TRAILER_MARK.len()] != TRAILER_MARK {
                at += MARK.len();
                continue;
            }
            let kind = frame[4];
            if kind == KEY_FRAME || kind == DELTA_FRAME {
                let date = read_u32(frame, 16);
                let position = Position {
                    number: read_u32(frame, 8),
                    milliseconds: u16::from_le_bytes([frame[20], frame[21]]),
                };
                let data = frame[payload_start..trailer].to_vec();
                self.accept(kind == KEY_FRAME, position, date, data, &mut frames);
            }
            at += size;
        }
        self.buffer.drain(..at);
        frames
    }

    /// Returns what was still held, for a stretch too short to be trusted before it ended.
    pub fn finish(&mut self) -> Vec<Frame> {
        std::mem::take(&mut self.held)
    }

    fn accept(
        &mut self,
        key: bool,
        position: Position,
        date: u32,
        data: Vec<u8>,
        frames: &mut Vec<Frame>,
    ) {
        // Dates pack the year down to the second in this order, so they compare as numbers.
        if self.trusted && date < self.newest_date {
            return;
        }
        let step = self
            .previous
            .as_ref()
            .filter(|previous| position.number == previous.number.wrapping_add(1))
            .map(|previous| u64::from(position.milliseconds.wrapping_sub(previous.milliseconds)));
        if step.is_none() {
            self.decodable = false;
            if !self.trusted {
                self.held.clear();
                self.newest_date = 0;
            }
        }
        self.previous = Some(position);
        self.decodable |= key;
        if !self.decodable {
            return;
        }
        if self.trusted || !self.held.is_empty() {
            self.elapsed_ms += step
                // Two frames can't show at the same moment, whatever time they carry.
                .filter(|step| (1..=MAX_STEP_MS).contains(step))
                .unwrap_or(DEFAULT_STEP_MS);
        } else {
            self.elapsed_ms = 0;
        }
        self.newest_date = self.newest_date.max(date);
        let frame = Frame {
            key,
            elapsed_ms: self.elapsed_ms,
            data,
        };
        if self.trusted {
            frames.push(frame);
            return;
        }
        self.held.push(frame);
        if self.held.len() >= FRAMES_TO_TRUST {
            self.trusted = true;
            frames.append(&mut self.held);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOON: u32 = 1 << 26 | 10 << 22 | 31 << 17 | 12 << 12;

    fn frame(kind: u8, number: u32, date: u32, milliseconds: u16, payload: &[u8]) -> Vec<u8> {
        let extension = [0x88u8, 0, 0, 0, 0, 0, 0, 0];
        let size = HEADER_SIZE + extension.len() + payload.len() + TRAILER_SIZE;
        let mut bytes = MARK.to_vec();
        bytes.extend([kind, 0, 0, 0]);
        bytes.extend(number.to_le_bytes());
        bytes.extend((size as u32).to_le_bytes());
        bytes.extend(date.to_le_bytes());
        bytes.extend(milliseconds.to_le_bytes());
        bytes.extend([extension.len() as u8, 0]);
        bytes.extend(extension);
        bytes.extend(payload);
        bytes.extend(TRAILER_MARK);
        bytes.extend((size as u32).to_le_bytes());
        bytes
    }

    /// A key frame and then enough frames in a row to be trusted.
    fn run(first: u32, date: u32, start_ms: u16) -> Vec<u8> {
        let mut bytes = Vec::new();
        for i in 0..FRAMES_TO_TRUST as u32 {
            let kind = if i == 0 { KEY_FRAME } else { DELTA_FRAME };
            let ms = start_ms.wrapping_add((i * 50) as u16);
            bytes.extend(frame(kind, first + i, date, ms, &i.to_le_bytes()));
        }
        bytes
    }

    fn read_all(bytes: &[u8]) -> Vec<Frame> {
        let mut reader = Reader::new();
        let mut frames = reader.push(bytes);
        frames.extend(reader.finish());
        frames
    }

    #[test]
    fn finds_frames_in_pieces_of_any_size() {
        let bytes = run(100, NOON, 0);
        let whole = read_all(&bytes);
        assert_eq!(whole.len(), FRAMES_TO_TRUST);

        let mut reader = Reader::new();
        let mut frames = Vec::new();
        for piece in bytes.chunks(7) {
            frames.extend(reader.push(piece));
        }
        frames.extend(reader.finish());
        assert_eq!(frames, whole);
    }

    #[test]
    fn skips_what_comes_before_the_first_frame() {
        let mut bytes = b"DHII some header DHAV and a false mark".to_vec();
        bytes.extend(run(100, NOON, 0));
        assert_eq!(read_all(&bytes).len(), FRAMES_TO_TRUST);
    }

    #[test]
    fn counts_time_from_the_first_frame_across_the_wrap_of_the_milliseconds() {
        let frames = read_all(&run(100, NOON, 65_500));
        let times: Vec<_> = frames.iter().take(3).map(|f| f.elapsed_ms).collect();
        assert_eq!(times, [0, 50, 100]);
    }

    #[test]
    fn drops_a_frame_cut_by_the_end_of_the_stream() {
        let mut bytes = frame(KEY_FRAME, 1, NOON, 0, &[1]);
        let cut = frame(DELTA_FRAME, 2, NOON, 50, &[2]);
        bytes.extend(&cut[..cut.len() - 3]);
        assert_eq!(read_all(&bytes).len(), 1);
    }

    #[test]
    fn delta_frames_before_the_first_key_frame_are_dropped() {
        let mut bytes = frame(DELTA_FRAME, 99, NOON, 0, &[0]);
        bytes.extend(run(100, NOON, 50));
        let frames = read_all(&bytes);
        assert_eq!(frames.len(), FRAMES_TO_TRUST);
        assert!(frames[0].key);
    }

    #[test]
    fn leftovers_before_the_stretch_are_dropped_even_with_a_key_frame() {
        let mut bytes = frame(KEY_FRAME, 5_000, NOON - 4_096, 0, &[9]);
        bytes.extend(frame(DELTA_FRAME, 5_001, NOON - 4_096, 50, &[9]));
        bytes.extend(run(100, NOON, 0));
        let frames = read_all(&bytes);
        assert_eq!(frames.len(), FRAMES_TO_TRUST);
        assert_eq!(frames[0].data, 0u32.to_le_bytes());
    }

    #[test]
    fn leftovers_in_a_gap_are_dropped_and_the_gap_is_skipped() {
        let mut bytes = run(100, NOON, 0);
        let end = 100 + FRAMES_TO_TRUST as u32;
        // Older frames in the place of the gap, with a key frame among them.
        bytes.extend(frame(KEY_FRAME, 7_000, NOON - 4_096, 0, &[9]));
        bytes.extend(frame(DELTA_FRAME, 7_001, NOON - 4_096, 50, &[9]));
        // The recording comes back without a key frame and that part can't be decoded.
        bytes.extend(frame(DELTA_FRAME, end + 900, NOON + 60, 0, &[8]));
        bytes.extend(frame(KEY_FRAME, end + 901, NOON + 60, 50, &[7]));
        bytes.extend(frame(DELTA_FRAME, end + 902, NOON + 60, 100, &[6]));

        let frames = read_all(&bytes);
        assert_eq!(frames.len(), FRAMES_TO_TRUST + 2);
        let last = &frames[FRAMES_TO_TRUST - 1];
        let after = &frames[FRAMES_TO_TRUST];
        assert_eq!(after.data, [7]);
        assert_eq!(after.elapsed_ms - last.elapsed_ms, 50);
        assert_eq!(
            frames[FRAMES_TO_TRUST + 1].elapsed_ms - after.elapsed_ms,
            50
        );
    }

    #[test]
    fn a_pause_inside_a_run_of_frames_is_skipped() {
        let mut bytes = run(100, NOON, 0);
        let end = 100 + FRAMES_TO_TRUST as u32;
        bytes.extend(frame(DELTA_FRAME, end, NOON + 30, 40_000, &[1]));
        let frames = read_all(&bytes);
        let last = &frames[FRAMES_TO_TRUST - 1];
        assert_eq!(frames[FRAMES_TO_TRUST].elapsed_ms - last.elapsed_ms, 50);
    }

    #[test]
    fn a_repeated_time_still_moves_forward() {
        let mut bytes = run(100, NOON, 0);
        let end = 100 + FRAMES_TO_TRUST as u32;
        let last_ms = (FRAMES_TO_TRUST as u16 - 1) * 50;
        bytes.extend(frame(DELTA_FRAME, end, NOON, last_ms, &[1]));
        let frames = read_all(&bytes);
        assert!(frames[FRAMES_TO_TRUST].elapsed_ms > frames[FRAMES_TO_TRUST - 1].elapsed_ms);
    }

    #[test]
    fn a_stretch_too_short_to_be_trusted_still_comes_out() {
        let mut reader = Reader::new();
        let bytes = frame(KEY_FRAME, 1, NOON, 0, &[1]);
        assert!(reader.push(&bytes).is_empty());
        assert_eq!(reader.finish().len(), 1);
    }

    #[test]
    fn ignores_what_is_not_video() {
        let mut bytes = frame(KEY_FRAME, 1, NOON, 0, &[1]);
        bytes.extend(frame(0xf1, 2, 7, 0, &[2]));
        bytes.extend(frame(DELTA_FRAME, 2, NOON, 50, &[3]));
        assert_eq!(read_all(&bytes).len(), 2);
    }

    #[test]
    fn tells_the_format_from_the_first_unit_of_a_key_frame() {
        let of = |data: &[u8]| {
            let frame = Frame {
                key: true,
                elapsed_ms: 0,
                data: data.to_vec(),
            };
            frame.codec()
        };
        assert_eq!(of(&[0, 0, 0, 1, 0x67, 0x64]), Codec::H264);
        assert_eq!(of(&[0, 0, 1, 0x67]), Codec::H264);
        assert_eq!(of(&[0, 0, 0, 1, 0x40, 0x01]), Codec::H265);
    }
}
