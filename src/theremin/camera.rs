/// Camera blob-tracking for the Theremin.
///
/// Background thread: capture → mirror → colour-filter → blob find → publish.
/// The tracking target is swappable at runtime from the UI thread.

use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};

// ─── Tracking target ──────────────────────────────────────────────────────────

/// What colour blob to look for in each frame.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum TrackTarget {
    /// Human skin — warm red-orange HSV range.
    Skin,
    /// Optic-yellow tennis ball — tight chartreuse-yellow window.
    TennisBall,
    /// Bright orange (orange ball, tangerine, marker cap …).
    OrangeBall,
}

impl TrackTarget {
    pub fn label(self) -> &'static str {
        match self {
            Self::Skin       => "Skin",
            Self::TennisBall => "Tennis ball",
            Self::OrangeBall => "Orange ball",
        }
    }
}

// ─── Public types ─────────────────────────────────────────────────────────────

/// Normalised (0–1) position of one detected blob, already in mirrored space.
#[derive(Clone, Copy, Debug)]
pub struct HandPos {
    pub x: f32,
    pub y: f32,
    /// Estimated pixel area of the blob (larger = more confident).
    pub area: usize,
}

/// Latest data shared from the capture thread to the UI thread.
pub struct CameraState {
    /// RGBA, row-major, already **mirrored** (flipped X).
    pub frame:  Vec<u8>,
    pub width:  u32,
    pub height: u32,
    /// Up to two detected blobs, sorted left-to-right on screen.
    pub blobs:  Vec<HandPos>,
    /// Debug mask: for each subsampled cell (step 4), 1 = matched colour.
    /// Same dimensions as frame ÷ 4, stored as a flat vec of booleans.
    pub debug_mask: Vec<bool>,
    pub mask_w: u32,
    pub mask_h: u32,
}

pub struct CameraTracker {
    pub state:  Arc<Mutex<Option<CameraState>>>,
    /// Write the desired target here; the capture thread reads it each frame.
    pub target: Arc<Mutex<TrackTarget>>,
    stop:       Arc<AtomicBool>,
    _thread:    std::thread::JoinHandle<()>,
}

impl CameraTracker {
    pub fn new(initial_target: TrackTarget) -> Self {
        let state  = Arc::new(Mutex::new(None::<CameraState>));
        let target = Arc::new(Mutex::new(initial_target));
        let stop   = Arc::new(AtomicBool::new(false));

        let (s2, t2, stop2) = (Arc::clone(&state), Arc::clone(&target), Arc::clone(&stop));
        let thread = std::thread::Builder::new()
            .name("camera-capture".into())
            .spawn(move || camera_loop(s2, t2, stop2))
            .expect("failed to spawn camera thread");

        Self { state, target, stop, _thread: thread }
    }
}

impl Drop for CameraTracker {
    fn drop(&mut self) { self.stop.store(true, Ordering::Relaxed); }
}

// ─── Background loop ──────────────────────────────────────────────────────────

fn camera_loop(
    state:  Arc<Mutex<Option<CameraState>>>,
    target: Arc<Mutex<TrackTarget>>,
    stop:   Arc<AtomicBool>,
) {
    use nokhwa::{
        pixel_format::RgbFormat,
        utils::{CameraIndex, RequestedFormat, RequestedFormatType},
        Camera,
    };

    let fmt = RequestedFormat::new::<RgbFormat>(RequestedFormatType::AbsoluteHighestFrameRate);
    let mut cam = match Camera::new(CameraIndex::Index(0), fmt) {
        Ok(c)  => c,
        Err(e) => { eprintln!("[camera] open failed: {e}"); return; }
    };
    if let Err(e) = cam.open_stream() {
        eprintln!("[camera] stream failed: {e}"); return;
    }
    eprintln!("[camera] stream open");

    while !stop.load(Ordering::Relaxed) {
        let buf = match cam.frame() {
            Ok(b)  => b,
            Err(_) => { std::thread::sleep(std::time::Duration::from_millis(33)); continue; }
        };
        let rgb = match buf.decode_image::<RgbFormat>() {
            Ok(img) => img,
            Err(_)  => continue,
        };

        let (w, h) = rgb.dimensions();
        let tgt = *target.lock().unwrap_or_else(|e| e.into_inner());

        // Subsampling stride — 4×4 blocks
        const STEP: u32 = 4;
        let mw = (w + STEP - 1) / STEP;
        let mh = (h + STEP - 1) / STEP;

        let mut rgba       = vec![0u8; (w * h * 4) as usize];
        let mut mask       = vec![false; (mw * mh) as usize];
        let mut match_pts: Vec<(u32, u32)> = Vec::new();  // (mirrored_x, y)

        for y in 0..h {
            for x in 0..w {
                let mx  = w - 1 - x;  // mirror X
                let idx = ((y * w + mx) * 4) as usize;
                let p   = rgb.get_pixel(x, y);
                rgba[idx]     = p[0];
                rgba[idx + 1] = p[1];
                rgba[idx + 2] = p[2];
                rgba[idx + 3] = 255;

                if x % STEP == 0 && y % STEP == 0 {
                    let hit = matches_target(p[0], p[1], p[2], tgt);
                    let mi  = ((y / STEP) * mw + mx / STEP) as usize;
                    if mi < mask.len() { mask[mi] = hit; }
                    if hit { match_pts.push((mx, y)); }
                }
            }
        }

        let blobs = find_blobs(&match_pts, w, h);

        if let Ok(mut s) = state.lock() {
            *s = Some(CameraState {
                frame: rgba, width: w, height: h, blobs,
                debug_mask: mask, mask_w: mw, mask_h: mh,
            });
        }

        std::thread::sleep(std::time::Duration::from_millis(33)); // ~30 fps
    }

    eprintln!("[camera] thread exiting");
}

// ─── Colour matching ──────────────────────────────────────────────────────────

fn matches_target(r: u8, g: u8, b: u8, tgt: TrackTarget) -> bool {
    let (h, s, v) = rgb_to_hsv(r, g, b);
    match tgt {
        TrackTarget::Skin => is_skin_hsv(h, s, v, r, g, b),
        TrackTarget::TennisBall => {
            // Optic yellow / chartreuse: H 45–75°, vivid, bright
            h >= 45.0 && h <= 75.0 && s > 0.35 && v > 0.40
        }
        TrackTarget::OrangeBall => {
            // Orange: H 10–35°, vivid, bright
            h >= 10.0 && h <= 35.0 && s > 0.50 && v > 0.35
        }
    }
}

fn rgb_to_hsv(r: u8, g: u8, b: u8) -> (f32, f32, f32) {
    let rf = r as f32 / 255.0;
    let gf = g as f32 / 255.0;
    let bf = b as f32 / 255.0;
    let max   = rf.max(gf).max(bf);
    let min   = rf.min(gf).min(bf);
    let delta = max - min;
    let h = if delta < 0.001 { 0.0 }
            else if max == rf { 60.0 * ((gf - bf) / delta).rem_euclid(6.0) }
            else if max == gf { 60.0 * ((bf - rf) / delta + 2.0) }
            else              { 60.0 * ((rf - gf) / delta + 4.0) };
    let s = if max < 0.001 { 0.0 } else { delta / max };
    (h, s, max)
}

fn is_skin_hsv(h: f32, s: f32, v: f32, r: u8, g: u8, b: u8) -> bool {
    if v < 0.20 || s < 0.08 { return false; }
    let warm = h < 38.0 || h > 322.0;
    let rgb_ok = r > 80 && g > 40 && b > 20 && r > b && (r as i32 - g as i32).abs() > 10;
    warm && s < 0.90 && rgb_ok
}

// ─── Blob finding ─────────────────────────────────────────────────────────────

/// Find up to two blobs via connected components on the subsampled mask grid.
/// Each 4×4 cell is one node; 4-connected flood fill labels islands.
/// Returns the two largest islands' centroids, sorted left-to-right.
fn find_blobs(pts: &[(u32, u32)], w: u32, h: u32) -> Vec<HandPos> {
    const STEP:     u32   = 4;
    const MIN_BLOB: usize = 20; // minimum cells to count as a real blob

    let mw = (w + STEP - 1) / STEP;
    let mh = (h + STEP - 1) / STEP;

    // Build boolean occupancy grid from the matched pixel list
    let mut grid = vec![false; (mw * mh) as usize];
    for &(px, py) in pts {
        let gx = px / STEP;
        let gy = py / STEP;
        if gx < mw && gy < mh {
            grid[(gy * mw + gx) as usize] = true;
        }
    }

    // Flood-fill to label connected components
    let mut labels = vec![0u16; (mw * mh) as usize];
    let mut next_label: u16 = 1;
    let mut component_cells: Vec<Vec<usize>> = Vec::new(); // label-1 → list of cell indices

    for start in 0..(mw * mh) as usize {
        if !grid[start] || labels[start] != 0 { continue; }

        // BFS from this seed
        let mut stack = vec![start];
        let mut cells = Vec::new();
        labels[start] = next_label;

        while let Some(idx) = stack.pop() {
            cells.push(idx);
            let gx = (idx as u32 % mw) as i32;
            let gy = (idx as u32 / mw) as i32;
            for (nx, ny) in [(gx-1,gy),(gx+1,gy),(gx,gy-1),(gx,gy+1)] {
                if nx < 0 || ny < 0 || nx >= mw as i32 || ny >= mh as i32 { continue; }
                let ni = (ny as u32 * mw + nx as u32) as usize;
                if grid[ni] && labels[ni] == 0 {
                    labels[ni] = next_label;
                    stack.push(ni);
                }
            }
        }

        component_cells.push(cells);
        next_label += 1;
    }

    // Pick the two largest components and compute their centroids.
    // Only keep the second blob if its area is at least 30% of the first —
    // a real second object will be similarly sized; noise / false positives won't be.
    component_cells.sort_by(|a, b| b.len().cmp(&a.len()));

    let mut blobs: Vec<HandPos> = Vec::new();
    for (rank, cells) in component_cells.iter().enumerate().take(2) {
        if cells.len() < MIN_BLOB { break; }
        if rank == 1 {
            let first_area = blobs[0].area;
            let this_area  = cells.len() * (STEP * STEP) as usize;
            if this_area < first_area * 3 / 10 { break; } // < 30% of largest → noise
        }
        let cx = cells.iter().map(|&i| (i as u32 % mw) as f64).sum::<f64>() / cells.len() as f64;
        let cy = cells.iter().map(|&i| (i as u32 / mw) as f64).sum::<f64>() / cells.len() as f64;
        blobs.push(HandPos {
            x:    ((cx * STEP as f64) / w as f64) as f32,
            y:    ((cy * STEP as f64) / h as f64) as f32,
            area: cells.len() * (STEP * STEP) as usize,
        });
    }

    blobs.sort_by(|a, b| a.x.partial_cmp(&b.x).unwrap());
    blobs
}
