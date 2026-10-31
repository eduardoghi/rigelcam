//! Which camera goes in each square of the grid.

use serde::{Deserialize, Serialize};

/// 6 by 6 fits the 32 channels of the largest recorder tested.
pub const MAX_COLUMNS: u32 = 6;

/// A camera of any saved recorder, since one grid can mix recorders.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct CameraId {
    pub recorder: u64,
    pub channel: u32,
}

/// A square grid, with `cameras` holding one entry per square in reading order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Layout {
    pub columns: u32,
    pub cameras: Vec<Option<CameraId>>,
}

fn squares(columns: u32) -> usize {
    (columns * columns) as usize
}

impl Layout {
    /// The smallest grid that fits the cameras, which stop at the largest grid.
    pub fn fitting(cameras: &[CameraId]) -> Self {
        let columns = (1..MAX_COLUMNS)
            .find(|&columns| squares(columns) >= cameras.len())
            .unwrap_or(MAX_COLUMNS);
        let mut filled: Vec<_> = cameras
            .iter()
            .take(squares(columns))
            .map(|&camera| Some(camera))
            .collect();
        filled.resize(squares(columns), None);
        Self {
            columns,
            cameras: filled,
        }
    }

    /// How many cameras the largest grid holds.
    pub const fn capacity() -> usize {
        (MAX_COLUMNS * MAX_COLUMNS) as usize
    }

    pub fn is_empty(&self) -> bool {
        self.cameras.iter().all(Option::is_none)
    }

    fn square_of(&self, camera: CameraId) -> Option<usize> {
        self.cameras.iter().position(|&c| c == Some(camera))
    }

    /// Puts a camera in the first empty square, growing a full grid to the next size. A camera
    /// already in the grid stays where it is. Returns false when the largest grid is full.
    pub fn add(&mut self, camera: CameraId) -> bool {
        if self.square_of(camera).is_some() {
            return true;
        }
        if !self.cameras.contains(&None) {
            self.resize(self.columns + 1);
        }
        match self.cameras.iter().position(Option::is_none) {
            Some(square) => {
                self.cameras[square] = Some(camera);
                true
            }
            None => false,
        }
    }

    /// Puts a camera in a square. A camera coming from another square trades places with the one
    /// there, and one from outside the grid replaces it.
    pub fn place(&mut self, camera: CameraId, square: usize) {
        if square >= self.cameras.len() {
            return;
        }
        if let Some(from) = self.square_of(camera) {
            self.cameras[from] = self.cameras[square];
        }
        self.cameras[square] = Some(camera);
    }

    /// Empties the square of a camera, so the others keep their places.
    pub fn remove(&mut self, camera: CameraId) {
        if let Some(square) = self.square_of(camera) {
            self.cameras[square] = None;
        }
    }

    /// Empties the squares of a recorder that was deleted.
    pub fn remove_recorder(&mut self, recorder: u64) {
        for camera in &mut self.cameras {
            if camera.is_some_and(|c| c.recorder == recorder) {
                *camera = None;
            }
        }
    }

    /// Changes the grid size. When shrinking cuts off a camera, the cameras first close the gaps
    /// between them, and only the ones that still don't fit leave the grid.
    pub fn resize(&mut self, columns: u32) {
        let columns = columns.clamp(1, MAX_COLUMNS);
        let wanted = squares(columns);
        if self.cameras.iter().skip(wanted).any(Option::is_some) {
            self.cameras.retain(Option::is_some);
        }
        self.cameras.resize(wanted, None);
        self.columns = columns;
    }

    /// Keeps a layout read from a file within the sizes the grid can show.
    pub fn normalize(&mut self) {
        self.columns = self.columns.clamp(1, MAX_COLUMNS);
        self.cameras.resize(squares(self.columns), None);
    }

    /// Fixes a saved layout with repeated cameras or a list of the wrong size, which only a file
    /// edited by hand would bring. Cameras a recorder doesn't list keep their squares, since a
    /// camera disabled for maintenance comes back and only the user knows it won't.
    pub fn repair(&mut self) {
        self.normalize();
        let mut seen = Vec::new();
        for camera in &mut self.cameras {
            if let Some(id) = *camera {
                if seen.contains(&id) {
                    *camera = None;
                }
                seen.push(id);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cam(channel: u32) -> CameraId {
        CameraId {
            recorder: 1,
            channel,
        }
    }

    fn cams(channels: &[u32]) -> Vec<CameraId> {
        channels.iter().map(|&channel| cam(channel)).collect()
    }

    fn filled(channels: &[u32]) -> Vec<Option<CameraId>> {
        channels.iter().map(|&channel| Some(cam(channel))).collect()
    }

    #[test]
    fn fitting_uses_the_smallest_grid_and_stops_at_the_largest() {
        let cameras = cams(&(1..=40).collect::<Vec<u32>>());
        assert_eq!(Layout::fitting(&cameras[..1]).columns, 1);
        assert_eq!(Layout::fitting(&cameras[..5]).columns, 3);
        assert_eq!(Layout::fitting(&cameras[..31]).columns, 6);
        let largest = Layout::fitting(&cameras);
        assert_eq!(largest.columns, 6);
        assert_eq!(largest.cameras.len(), 36);
        assert_eq!(largest.cameras[35], Some(cam(36)));
    }

    #[test]
    fn add_fills_the_first_gap_and_grows_a_full_grid() {
        let mut layout = Layout::fitting(&cams(&[1, 2, 3, 4]));
        layout.remove(cam(2));
        layout.add(cam(9));
        assert_eq!(layout.cameras, filled(&[1, 9, 3, 4]));
        layout.add(cam(9));
        assert_eq!(layout.columns, 2);
        layout.add(cam(8));
        assert_eq!(layout.columns, 3);
        assert_eq!(layout.cameras[4], Some(cam(8)));
    }

    #[test]
    fn add_does_nothing_when_the_largest_grid_is_full() {
        let cameras = cams(&(1..=36).collect::<Vec<u32>>());
        let mut layout = Layout::fitting(&cameras);
        assert!(!layout.add(cam(37)));
        assert!(layout.add(cam(36)));
        assert_eq!(layout, Layout::fitting(&cameras));
    }

    #[test]
    fn place_swaps_inside_the_grid_and_replaces_from_outside() {
        let mut layout = Layout::fitting(&cams(&[1, 2, 3, 4]));
        layout.place(cam(1), 3);
        assert_eq!(layout.cameras, filled(&[4, 2, 3, 1]));
        layout.place(cam(9), 1);
        assert_eq!(layout.cameras, filled(&[4, 9, 3, 1]));
        layout.place(cam(9), 1);
        assert_eq!(layout.cameras, filled(&[4, 9, 3, 1]));
        layout.place(cam(7), 4);
        assert_eq!(layout.cameras, filled(&[4, 9, 3, 1]));
    }

    #[test]
    fn remove_keeps_the_other_cameras_in_place() {
        let mut layout = Layout::fitting(&cams(&[1, 2, 3]));
        layout.remove(cam(1));
        assert_eq!(layout.cameras, vec![None, Some(cam(2)), Some(cam(3)), None]);
    }

    #[test]
    fn same_channel_of_two_recorders_are_different_cameras() {
        let other = CameraId {
            recorder: 2,
            channel: 1,
        };
        let mut layout = Layout::fitting(&[cam(1)]);
        layout.add(other);
        assert_eq!(layout.columns, 2);
        assert_eq!(layout.cameras[..2], [Some(cam(1)), Some(other)]);
        layout.remove_recorder(1);
        assert_eq!(layout.cameras[..2], [None, Some(other)]);
    }

    #[test]
    fn shrinking_keeps_as_many_cameras_as_fit() {
        let mut layout = Layout::fitting(&cams(&[1, 2, 3, 4, 5, 6, 7, 8, 9]));
        for channel in [1, 2, 3, 4] {
            layout.remove(cam(channel));
        }
        layout.resize(2);
        assert_eq!(layout.cameras, filled(&[5, 6, 7, 8]));
    }

    #[test]
    fn shrinking_without_cutting_a_camera_keeps_the_gaps() {
        let mut layout = Layout::fitting(&cams(&[1, 2, 3, 4]));
        layout.remove(cam(1));
        layout.resize(3);
        layout.resize(2);
        assert_eq!(
            layout.cameras,
            vec![None, Some(cam(2)), Some(cam(3)), Some(cam(4))]
        );
    }

    #[test]
    fn resize_stays_within_the_limits() {
        let mut layout = Layout::fitting(&[cam(1)]);
        layout.resize(0);
        assert_eq!(layout.columns, 1);
        layout.resize(9);
        assert_eq!((layout.columns, layout.cameras.len()), (6, 36));
    }

    #[test]
    fn repair_drops_repeated_cameras_and_fixes_the_size() {
        let mut layout = Layout {
            columns: 2,
            cameras: vec![Some(cam(1)), Some(cam(1)), Some(cam(9)), None, Some(cam(2))],
        };
        layout.repair();
        assert_eq!(layout.cameras, vec![Some(cam(1)), None, Some(cam(9)), None]);
    }
}
