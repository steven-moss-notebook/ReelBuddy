use bevy::prelude::*;

use crate::sorter::sand_hanabi::SandTheme;

pub const GRID_W: usize = 96;
pub const GRID_H: usize = 120;
pub const CELL_PX: u32 = 3;
pub const TEX_W: u32 = GRID_W as u32 * CELL_PX;
pub const TEX_H: u32 = GRID_H as u32 * CELL_PX;
pub const SHORE_ROWS: usize = 18;
pub const RAIL_TOP: f32 = 28.0;
pub const RAIL_BOTTOM: f32 = 22.0;
pub const STILE: f32 = 14.0;
/// A finished job fills this fraction of the glass. The top quarter stays water.
pub const FILL_FRACTION: f32 = 0.75;
/// A column may stand this many cells above a neighbor before the top grain spills.
const REPOSE: usize = 2;
/// Rows to let out when the picture changes. One row per step, then the new pour starts.
const TURN_ROWS: u16 = 10;
/// Grains released in one frame while progress is catching up.
const POUR_BATCH: usize = 24;
/// New grains let into the glass per step. Each still falls one row per step.
const ADMIT_PER_STEP: usize = 12;
/// How far one landing may slump. A crest spills; it does not cross the glass.
const SLUMP_STEPS: usize = 3;

/// How many rows a job at `progress` (0..1) may fill. Full progress stops at 75% of the glass.
pub fn progress_rows(progress: f32) -> usize {
    let progress = progress.clamp(0.0, 1.0);
    let rows = (progress * FILL_FRACTION * GRID_H as f32).round() as usize;
    rows.min(GRID_H)
}

/// Loading used to release `pct * 14` bubbles. Forty percent fewer is 8 at full progress.
pub fn loading_bubble_cap(pct: f32) -> u32 {
    (pct.clamp(0.0, 1.0) * 8.0).round() as u32
}

/// One bubble on three of five energetic frames.
pub fn frame_bubble(energy: f32, column: usize) -> bool {
    energy > 0.4 && column % 5 < 3
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GrainKind {
    Sand,
    Oil,
    Bubble,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cell {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub base_r: u8,
    pub base_g: u8,
    pub base_b: u8,
    pub kind: GrainKind,
}

impl Cell {
    pub fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self {
            r,
            g,
            b,
            base_r: r,
            base_g: g,
            base_b: b,
            kind: GrainKind::Sand,
        }
    }

    pub fn with_base(r: u8, g: u8, b: u8, base_r: u8, base_g: u8, base_b: u8) -> Self {
        Self {
            r,
            g,
            b,
            base_r,
            base_g,
            base_b,
            kind: GrainKind::Sand,
        }
    }

    pub fn oil(r: u8, g: u8, b: u8) -> Self {
        Self {
            r,
            g,
            b,
            base_r: r,
            base_g: g,
            base_b: b,
            kind: GrainKind::Oil,
        }
    }

    pub fn bubble() -> Self {
        Self {
            r: 226,
            g: 238,
            b: 244,
            base_r: 226,
            base_g: 238,
            base_b: 244,
            kind: GrainKind::Bubble,
        }
    }

    pub fn is_sand(self) -> bool {
        self.kind == GrainKind::Sand
    }
}

#[derive(Clone, Debug)]
pub struct Glint {
    pub id: u64,
    pub window_id: i64,
    /// Section start while the moment is still being found. Negative until then.
    pub start_s: f64,
    pub column: usize,
    pub row: usize,
    pub buried_depth: f32,
    pub initial_cover: u32,
    pub score: f64,
    pub rgb: [u8; 3],
    pub announced: bool,
}

#[derive(Resource, Clone, Copy, Debug)]
pub struct ChamberLayout {
    pub width: f32,
    pub height: f32,
    pub offset_y: f32,
}

impl ChamberLayout {
    pub fn from_window(window_w: f32, window_h: f32) -> Self {
        let top = window_h * 0.5 - RAIL_TOP;
        let bottom = -window_h * 0.5 + RAIL_BOTTOM;
        Self {
            width: (window_w - STILE * 2.0).max(48.0),
            height: (top - bottom).max(48.0),
            offset_y: (top + bottom) * 0.5,
        }
    }

    pub fn left(&self) -> f32 {
        -self.width * 0.5
    }

    pub fn bottom(&self) -> f32 {
        self.offset_y - self.height * 0.5
    }

    pub fn top(&self) -> f32 {
        self.offset_y + self.height * 0.5
    }

    pub fn cell_w(&self) -> f32 {
        self.width / GRID_W as f32
    }

    pub fn cell_h(&self) -> f32 {
        self.height / GRID_H as f32
    }

    pub fn column_at(&self, world_x: f32) -> usize {
        let t = ((world_x - self.left()) / self.width).clamp(0.0, 0.999);
        (t * GRID_W as f32) as usize
    }

    pub fn row_at(&self, world_y: f32) -> usize {
        let t = ((world_y - self.bottom()) / self.height).clamp(0.0, 0.999);
        (t * GRID_H as f32) as usize
    }

    pub fn world_x(&self, col: usize) -> f32 {
        self.left() + (col as f32 + 0.5) * self.cell_w()
    }

    pub fn surface_y(&self, pile_top: usize) -> f32 {
        self.bottom() + pile_top as f32 * self.cell_h()
    }
}

#[derive(Resource, Clone, Debug)]
pub struct SandCanvas {
    cells: Vec<Option<Cell>>,
    pub dirty: bool,
    pub active_theme: SandTheme,
    pub column_cursor: usize,
    pub occupied: u32,
    /// A new video is letting the old floor out while the next pour comes in.
    pub turning: bool,
    pub last_sample_t: Option<f64>,
    samples: Vec<(f64, usize)>,
    glints: Vec<Glint>,
    next_glint: u64,
    queue: Vec<(i32, Cell)>,
    drain_left: u16,
    rows_drained: u16,
    poured_since_turn: u32,
    step_clock: u32,
    /// Sand may not stack above this row. Progress raises it, up to 75% of the glass.
    fill_limit: usize,
    pub settled: bool,
    quiescent_steps: u32,
}

impl Default for SandCanvas {
    fn default() -> Self {
        Self::new()
    }
}

impl SandCanvas {
    pub fn new() -> Self {
        Self {
            cells: vec![None; GRID_W * GRID_H],
            dirty: true,
            active_theme: SandTheme::GoldenSunrise,
            column_cursor: 0,
            occupied: 0,
            turning: false,
            last_sample_t: None,
            samples: Vec::new(),
            glints: Vec::new(),
            next_glint: 1,
            queue: Vec::new(),
            drain_left: 0,
            rows_drained: 0,
            poured_since_turn: 0,
            step_clock: 0,
            fill_limit: progress_rows(1.0),
            settled: false,
            quiescent_steps: 0,
        }
    }

    pub fn apply_theme(&mut self, theme: SandTheme) {
        let changed = self.active_theme != theme;
        self.active_theme = theme;
        if changed {
            for opt in &mut self.cells {
                if let Some(cell) = opt {
                    if cell.kind == GrainKind::Sand {
                        let stamped = theme.stamp_color(cell.base_r, cell.base_g, cell.base_b);
                        cell.r = stamped[0];
                        cell.g = stamped[1];
                        cell.b = stamped[2];
                    }
                }
            }
            self.dirty = true;
        }
    }

    pub fn is_settled(&self) -> bool {
        self.settled
    }

    pub fn wake(&mut self) {
        self.settled = false;
        self.quiescent_steps = 0;
    }

    pub fn clear_unsettled_above_limit(&mut self) {
        let mut cleared = false;
        for y in self.fill_limit..GRID_H {
            for x in 0..GRID_W {
                if let Some(cell) = self.cell(x, y) {
                    if cell.is_sand() {
                        self.set_cell(x, y, None);
                        cleared = true;
                    }
                }
            }
        }
        if cleared {
            self.dirty = true;
        }
    }

    pub fn cell(&self, x: usize, y: usize) -> Option<Cell> {
        self.cells.get(self.idx(x, y)).copied().flatten()
    }

    pub fn glint(&self, id: u64) -> Option<&Glint> {
        self.glints.iter().find(|g| g.id == id)
    }

    /// World center of the painted jewel, matching `paint_jewel`.
    pub fn jewel_center(&self, layout: &ChamberLayout, id: u64) -> Option<Vec2> {
        let glint = self.glint(id)?;
        Some(Vec2::new(
            layout.world_x(glint.column),
            layout.bottom() + (glint.row as f32 + 0.5) * layout.cell_h(),
        ))
    }

    pub fn has_span(&self, start_s: f64) -> bool {
        self.glints
            .iter()
            .any(|g| g.start_s >= 0.0 && (g.start_s - start_s).abs() < 0.05)
    }

    pub fn attach_window(&mut self, start_s: f64, end_s: f64, window_id: i64) {
        for g in &mut self.glints {
            if g.start_s >= 0.0 && g.start_s + 0.05 >= start_s && g.start_s <= end_s + 0.05 {
                g.window_id = window_id;
            }
        }
    }

    pub fn remove_glint(&mut self, id: u64) {
        let before = self.glints.len();
        self.glints.retain(|g| g.id != id);
        if self.glints.len() != before {
            self.dirty = true;
        }
    }

    /// Lowest empty row, the height a new grain stacks to.
    pub fn column_height(&self, x: usize) -> usize {
        (0..GRID_H)
            .find(|y| self.cell(x, *y).is_none())
            .unwrap_or(GRID_H)
    }

    /// Cells resting on the floor, ignoring grains still falling above them.
    fn resting_height(&self, x: usize) -> usize {
        (0..GRID_H)
            .take_while(|&y| self.cell(x, y).is_some())
            .count()
    }

    /// Where the boot pour is sprinkling. Progress does not slide this to the edge.
    pub fn stream_column(&self) -> usize {
        dune_site(self.column_cursor)
    }

    /// One past the highest occupied cell. This is the surface a falling grain hits.
    pub fn pile_top(&self, x: usize) -> usize {
        (0..GRID_H)
            .rev()
            .find(|y| self.cell(x, *y).is_some())
            .map(|y| y + 1)
            .unwrap_or(0)
    }

    pub fn deposit(&mut self, x: i32, cell: Cell) -> bool {
        let mut x = x.clamp(0, GRID_W as i32 - 1) as usize;
        if self.pile_top(x) >= self.fill_limit {
            let left = if x > 0 {
                self.pile_top(x - 1)
            } else {
                usize::MAX
            };
            let right = if x + 1 < GRID_W {
                self.pile_top(x + 1)
            } else {
                usize::MAX
            };
            if left <= right && left < self.fill_limit {
                x -= 1;
            } else if right < self.fill_limit {
                x += 1;
            } else if left < self.fill_limit {
                x -= 1;
            } else {
                return false;
            }
        }
        let y = self.pile_top(x);
        if y >= GRID_H || y >= self.fill_limit || self.cell(x, y).is_some() {
            return false;
        }
        self.wake();
        self.set_cell(x, y, Some(cell));
        self.avalanche_from(x);
        true
    }

    /// Drop a stream across a dune, not a single neck. Grains wait at the top and sink.
    pub fn pour_stream(&mut self, column: i32, cells: &[Cell], oil: Option<Cell>) {
        for (i, cell) in cells.iter().enumerate() {
            let h = grain_hash(column.max(0) as usize, i, cells.len() as u32);
            let jitter = (h % 15) as i32 - 7;
            self.pour(column + jitter, *cell);
        }
        if let Some(oil) = oil {
            let h = grain_hash(column.max(0) as usize, 3, 11);
            let jitter = (h % 9) as i32 - 4;
            self.pour(column + jitter, oil);
        }
    }

    pub fn pour(&mut self, column: i32, cell: Cell) {
        self.wake();
        self.queue.push((column, cell));
        if self.turning {
            self.poured_since_turn = self.poured_since_turn.saturating_add(1);
        }
        self.dirty = true;
    }

    pub fn count_kind(&self, kind: GrainKind) -> u32 {
        self.cells.iter().flatten().filter(|cell| cell.kind == kind).count() as u32
    }

    /// A bounded set of moves so the fall stays visible.
    pub fn step(&mut self, budget: usize) {
        if budget == 0 || self.settled {
            return;
        }
        self.step_clock = self.step_clock.wrapping_add(1);
        let mut moved = 0;
        let admit = budget.min(ADMIT_PER_STEP);
        while moved < admit {
            let Some((col, cell)) = self.queue.pop() else {
                break;
            };
            if self.admit(col, cell) {
                moved += 1;
            }
        }
        // Every falling grain drops one row. The scan does not revisit a cell
        // it just moved, so a grain cannot cross the glass in one update.
        for y in 1..GRID_H {
            for x in 0..GRID_W {
                if self.try_sink_sand(x, y) {
                    moved += 1;
                }
            }
        }
        let bubble_cap = moved + budget / 4 + 4;
        for y in (0..GRID_H.saturating_sub(1)).rev() {
            if moved >= bubble_cap {
                break;
            }
            for x in 0..GRID_W {
                if moved >= bubble_cap {
                    break;
                }
                if self.try_rise_bubble(x, y) {
                    moved += 1;
                }
            }
        }
        if self.step_clock % 2 == 0 {
            let oil_cap = moved + 8;
            for y in (0..GRID_H).rev() {
                if moved >= oil_cap {
                    break;
                }
                for x in 0..GRID_W {
                    if moved >= oil_cap {
                        break;
                    }
                    if self.try_move_oil(x, y) {
                        moved += 1;
                    }
                }
            }
        }
        if moved == 0 && self.queue.is_empty() {
            self.quiescent_steps += 1;
            if self.quiescent_steps >= 8 {
                self.settled = true;
            }
        } else {
            self.quiescent_steps = 0;
        }
    }

    /// Next stroke site. Samples linger on a dune, then jump to another,
    /// so the pile grows crests instead of a level sheet.
    pub fn advance_column(&mut self) -> usize {
        self.column_cursor = self.column_cursor.wrapping_add(1);
        dune_site(self.column_cursor)
    }

    pub fn note_sample(&mut self, t: f64, col: usize) {
        self.samples.push((t, col));
        self.last_sample_t = Some(t);
    }

    pub fn column_for_time(&self, t: f64) -> Option<usize> {
        self.samples
            .iter()
            .min_by(|a, b| {
                let da = (a.0 - t).abs();
                let db = (b.0 - t).abs();
                da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
            })
            .map(|(_, col)| *col)
    }

    /// Release a short pour for `pct` (0–100). Grains wait at the top and fall.
    /// 100% stops at 75% of the frame. `dunes` uses the scan crest.
    pub fn feed_pour(
        &mut self,
        pct: f32,
        dunes: bool,
        mut color_at: impl FnMut(usize, usize, f32) -> Cell,
    ) -> u32 {
        let progress = (pct / 100.0).clamp(0.0, 1.0);
        let rows = progress_rows(progress);
        self.fill_limit = rows;
        let target = self.crest_cells(rows, dunes);
        let pending = self.occupied.saturating_add(self.queue.len() as u32);
        if rows == 0 || pending >= target {
            return 0;
        }
        let budget = ((target - pending) as usize).min(POUR_BATCH);
        let mut added = 0u32;
        for i in 0..budget {
            let prefer = dune_site(self.column_cursor.wrapping_add(i));
            let Some(x) = self.column_under_crest(prefer, rows, dunes) else {
                break;
            };
            let jitter = (grain_hash(x, i, self.step_clock) % 7) as i32 - 3;
            let x = (x as i32 + jitter).rem_euclid(GRID_W as i32) as usize;
            let x = self.column_under_crest(x, rows, dunes).unwrap_or(x);
            self.pour(x as i32, color_at(x, i % rows.max(1), progress));
            added += 1;
        }
        if dunes {
            let bubbles = ((progress * 16.0).round() as u32).min(16);
            let waiting = self
                .queue
                .iter()
                .filter(|(_, cell)| cell.kind == GrainKind::Bubble)
                .count() as u32;
            if self.count_kind(GrainKind::Bubble) + waiting < bubbles {
                let x = dune_site(self.column_cursor);
                self.pour(x as i32, Cell::bubble());
            }
        }
        self.column_cursor = self.column_cursor.wrapping_add(budget.max(1));
        added
    }

    fn crest_cells(&self, rows: usize, dunes: bool) -> u32 {
        (0..GRID_W)
            .map(|x| {
                if dunes {
                    dune_crest(x, rows)
                } else {
                    column_crest(x, rows)
                }
            })
            .sum::<usize>() as u32
    }

    fn column_under_crest(&self, prefer: usize, rows: usize, dunes: bool) -> Option<usize> {
        (0..GRID_W).find_map(|i| {
            let x = (prefer + i) % GRID_W;
            let crest = if dunes {
                dune_crest(x, rows)
            } else {
                column_crest(x, rows)
            };
            (crest > 0 && self.resting_height(x) < crest).then_some(x)
        })
    }

    pub fn shore_count(&self) -> u32 {
        self.band_count(0, SHORE_ROWS)
    }

    pub fn bury(&mut self, column: usize, window_id: i64, score: f64, rgb: [u8; 3]) -> u64 {
        let column = column.min(GRID_W - 1);
        let cover = self.cells_in_column(column);
        let top = self.resting_height(column);
        let floor = SHORE_ROWS.saturating_add(3).min(GRID_H - 1);
        let row = if top <= floor {
            floor.min(top.saturating_sub(1))
        } else {
            let span = (top - floor).max(1);
            floor + ((self.next_glint as usize).wrapping_mul(17) % span)
        };
        let id = self.next_glint;
        self.next_glint += 1;
        self.glints.push(Glint {
            id,
            window_id,
            start_s: -1.0,
            column,
            row,
            buried_depth: if cover == 0 { 0.0 } else { 1.0 },
            initial_cover: cover.max(1),
            score,
            rgb,
            announced: false,
        });
        id
    }

    /// A jewel for a section that was just identified. The same start is not buried twice.
    pub fn bury_section(&mut self, column: usize, start_s: f64, score: f64, rgb: [u8; 3]) -> u64 {
        if let Some(existing) = self
            .glints
            .iter()
            .find(|g| g.start_s >= 0.0 && (g.start_s - start_s).abs() < 0.05)
        {
            return existing.id;
        }
        let id = self.bury(column, 0, score, rgb);
        if let Some(g) = self.glints.iter_mut().find(|g| g.id == id) {
            g.start_s = start_s;
        }
        id
    }

    /// Push sand sideways inside the brush. Cells move; they are not deleted.
    pub fn brush(&mut self, col: i32, row: i32, radius: i32) {
        self.wake();
        let radius = radius.max(1);
        let y0 = (row - radius).max(0);
        let y1 = (row + radius).min(GRID_H as i32 - 1);
        for y in y0..=y1 {
            let y = y as usize;
            let right_end = (col + radius).min(GRID_W as i32 - 1);
            for x in ((col + 1)..=right_end).rev() {
                self.push_aside(x, y as i32, x + 1, y as i32);
            }
            let left_start = (col - radius).max(0);
            for x in left_start..col {
                self.push_aside(x, y as i32, x - 1, y as i32);
            }
            self.push_aside(col, y as i32, col + 1, y as i32);
            if self.in_bounds(col, y as i32) && self.cell(col as usize, y).is_some() {
                self.push_aside(col, y as i32, col - 1, y as i32);
            }
        }
        self.refresh_glints();
    }

    pub fn refresh_glints(&mut self) {
        for g in &mut self.glints {
            let above = cells_in_column_of(&self.cells, g.column);
            g.buried_depth = if above == 0 {
                0.0
            } else {
                (above as f32 / g.initial_cover as f32).min(1.0)
            };
        }
    }

    pub fn newly_uncovered(&mut self) -> Vec<u64> {
        let mut ids = Vec::new();
        for g in &mut self.glints {
            if g.buried_depth <= 0.0 && !g.announced {
                g.announced = true;
                ids.push(g.id);
            }
        }
        ids
    }

    /// A new video turns the frame. The floor leaves one row at a time.
    pub fn begin_turn(&mut self) {
        if self.occupied == 0 && self.queue.is_empty() {
            return;
        }
        self.wake();
        self.turning = true;
        self.drain_left = TURN_ROWS;
        self.rows_drained = 0;
        self.poured_since_turn = 0;
    }

    /// Let one floor row out. Stops once the new dune is standing, and never
    /// clears the last grains while nothing new has landed.
    pub fn drain_floor(&mut self) -> bool {
        if !self.turning {
            return false;
        }
        if self.turn_finished() {
            self.turning = false;
            return false;
        }
        self.shift_down();
        self.rows_drained = self.rows_drained.saturating_add(1);
        self.drain_left = self.drain_left.saturating_sub(1);
        if self.turn_finished() || self.occupied == 0 {
            self.turning = false;
        }
        true
    }

    fn turn_finished(&self) -> bool {
        if self.drain_left == 0 {
            return true;
        }
        if self.occupied > 0 && self.occupied <= GRID_W as u32 && self.queue.is_empty() && self.poured_since_turn < 12
        {
            return true;
        }
        if self.rows_drained >= 12 && self.poured_since_turn >= 24 {
            let crest = (0..GRID_W).map(|x| self.pile_top(x)).max().unwrap_or(0);
            if crest >= 8 {
                return true;
            }
        }
        false
    }

    fn shift_down(&mut self) {
        let mut next = vec![None; GRID_W * GRID_H];
        let mut occupied = 0u32;
        for y in 0..GRID_H - 1 {
            for x in 0..GRID_W {
                if let Some(cell) = self.cell(x, y + 1) {
                    next[x + y * GRID_W] = Some(cell);
                    occupied += 1;
                }
            }
        }
        self.cells = next;
        self.occupied = occupied;
        self.dirty = true;
        if !self.glints.is_empty() {
            self.refresh_glints();
        }
    }

    fn admit(&mut self, col: i32, cell: Cell) -> bool {
        let x = col.clamp(0, GRID_W as i32 - 1) as usize;
        if self.fill_limit == 0 {
            return false;
        }
        if cell.kind == GrainKind::Bubble {
            let y = self.column_height(x).min(self.fill_limit - 1);
            if self.cell(x, y).is_some() {
                return false;
            }
            self.set_cell(x, y, Some(cell));
            return true;
        }
        if self.resting_height(x) >= self.fill_limit {
            return false;
        }
        let Some(y) = (0..GRID_H).rev().find(|&y| self.cell(x, y).is_none()) else {
            return false;
        };
        self.set_cell(x, y, Some(cell));
        true
    }

    fn try_sink_sand(&mut self, x: usize, y: usize) -> bool {
        let Some(cell) = self.cell(x, y) else {
            return false;
        };
        if !cell.is_sand() || y == 0 || self.cell(x, y - 1).is_some() {
            return false;
        }
        let drift = (grain_hash(x, y, self.step_clock) % 3) as i32 - 1;
        let options = [(drift, -1), (0, -1), (-drift, -1)];
        for (dx, dy) in options {
            let nx = x as i32 + dx;
            let ny = y as i32 + dy;
            if !self.in_bounds(nx, ny) || self.cell(nx as usize, ny as usize).is_some() {
                continue;
            }
            let landed = ny == 0 || self.cell(nx as usize, ny as usize - 1).is_some();
            if landed && (ny as usize) >= self.fill_limit {
                continue;
            }
            self.set_cell(x, y, None);
            self.set_cell(nx as usize, ny as usize, Some(cell));
            if landed {
                self.avalanche_from(nx as usize);
            }
            return true;
        }
        false
    }

    fn try_rise_bubble(&mut self, x: usize, y: usize) -> bool {
        let Some(cell) = self.cell(x, y) else {
            return false;
        };
        if cell.kind != GrainKind::Bubble {
            return false;
        }
        if y + 1 >= GRID_H {
            self.set_cell(x, y, None);
            return true;
        }
        if self.cell(x, y + 1).is_none() {
            let wobble = (grain_hash(x, y, self.step_clock) % 3) as i32 - 1;
            let nx = (x as i32 + wobble).clamp(0, GRID_W as i32 - 1) as usize;
            let dest_x = if self.cell(nx, y + 1).is_none() { nx } else { x };
            self.set_cell(x, y, None);
            self.set_cell(dest_x, y + 1, Some(cell));
            return true;
        }
        let above = self.cell(x, y + 1);
        let blocked = above.is_some_and(|other| other.kind != GrainKind::Bubble);
        if !blocked {
            return false;
        }
        if grain_hash(x, y, self.step_clock) % 5 != 0 {
            return false;
        }
        let dir = if grain_hash(y, x, self.step_clock) % 2 == 0 { 1 } else { -1 };
        let nx = x as i32 + dir;
        if self.in_bounds(nx, y as i32) && self.cell(nx as usize, y).is_none() {
            let Some(sand) = self.cell(x, y + 1) else {
                return false;
            };
            if !sand.is_sand() {
                return false;
            }
            self.set_cell(x, y + 1, None);
            self.set_cell(nx as usize, y, Some(sand));
            self.set_cell(x, y, None);
            self.set_cell(x, y + 1, Some(cell));
            return true;
        }
        false
    }

    fn try_move_oil(&mut self, x: usize, y: usize) -> bool {
        let Some(cell) = self.cell(x, y) else {
            return false;
        };
        if cell.kind != GrainKind::Oil {
            return false;
        }
        let here = self.oil_neighbors(x as i32, y as i32);
        let mut best: Option<(usize, usize, i32)> = None;
        for dx in [-1, 1] {
            let nx = x as i32 + dx;
            let ny = y as i32;
            if !self.in_bounds(nx, ny) || self.cell(nx as usize, ny as usize).is_some() {
                continue;
            }
            let score = self.oil_neighbors(nx, ny);
            if score <= here {
                continue;
            }
            if best.is_none_or(|(_, _, prev)| score > prev) {
                best = Some((nx as usize, ny as usize, score));
            }
        }
        if let Some((nx, ny, _)) = best {
            self.set_cell(x, y, None);
            self.set_cell(nx, ny, Some(cell));
            return true;
        }
        if y > 0 && self.cell(x, y - 1).is_none() {
            self.set_cell(x, y, None);
            self.set_cell(x, y - 1, Some(cell));
            return true;
        }
        false
    }

    fn oil_neighbors(&self, x: i32, y: i32) -> i32 {
        let mut n = 0;
        for dy in -1..=1 {
            for dx in -1..=1 {
                if dx == 0 && dy == 0 {
                    continue;
                }
                let nx = x + dx;
                let ny = y + dy;
                if self.in_bounds(nx, ny) && self.cell(nx as usize, ny as usize).is_some_and(|cell| cell.kind == GrainKind::Oil)
                {
                    n += 1;
                }
            }
        }
        n
    }

    pub fn to_rgba(&self) -> Vec<u8> {
        let mut crest = [0usize; GRID_W];
        for x in 0..GRID_W {
            crest[x] = self.pile_top(x);
        }
        let mut data = vec![0u8; (TEX_W * TEX_H * 4) as usize];
        for y in 0..GRID_H {
            for x in 0..GRID_W {
                let cell = self.cell(x, y);
                let ground = ground_rgb(x, y, self.active_theme);
                let oil_edge = cell.is_some_and(|c| c.kind == GrainKind::Oil) && self.oil_rim(x, y);
                for py in 0..CELL_PX {
                    for px_i in 0..CELL_PX {
                        let tx = x as u32 * CELL_PX + px_i;
                        let ty = (GRID_H as u32 - 1 - y as u32) * CELL_PX + py;
                        let i = ((ty * TEX_W + tx) * 4) as usize;
                        let (r, g, b) = if let Some(c) = cell {
                            match c.kind {
                                GrainKind::Sand => shade_grain(c, x, y, px_i, py, crest[x] == y + 1),
                                GrainKind::Oil => shade_oil(c, px_i, py, oil_edge),
                                GrainKind::Bubble => shade_bubble(px_i, py, ground),
                            }
                        } else {
                            ground
                        };
                        data[i] = r;
                        data[i + 1] = g;
                        data[i + 2] = b;
                        data[i + 3] = 255;
                    }
                }
            }
        }
        for glint in &self.glints {
            if glint.buried_depth <= 0.0 {
                continue;
            }
            paint_jewel(&mut data, glint.column, glint.row, glint.rgb, glint.score);
        }
        data
    }

    /// Spill the resting crest a few columns onto the neighbor's surface.
    fn avalanche_from(&mut self, start: usize) {
        let mut x = start.min(GRID_W - 1);
        for _ in 0..SLUMP_STEPS {
            let h = self.resting_height(x);
            if h == 0 {
                break;
            }
            let left = if x > 0 {
                self.resting_height(x - 1)
            } else {
                usize::MAX
            };
            let right = if x + 1 < GRID_W {
                self.resting_height(x + 1)
            } else {
                usize::MAX
            };
            let downhill = if x > 0 && left <= right {
                Some((x - 1, left))
            } else if x + 1 < GRID_W {
                Some((x + 1, right))
            } else {
                None
            };
            let Some((nx, nh)) = downhill else {
                break;
            };
            if nh == usize::MAX || h <= nh + REPOSE || nh >= GRID_H {
                break;
            }
            if self.cell(nx, nh).is_some() {
                break;
            }
            let Some(cell) = self.cell(x, h - 1) else {
                break;
            };
            self.set_cell(x, h - 1, None);
            self.set_cell(nx, nh, Some(cell));
            x = nx;
        }
        if !self.glints.is_empty() {
            self.refresh_glints();
        }
    }

    fn band_count(&self, row: usize, rows: usize) -> u32 {
        let mut n = 0;
        for y in row..(row + rows).min(GRID_H) {
            for x in 0..GRID_W {
                if self.cell(x, y).is_some() {
                    n += 1;
                }
            }
        }
        n
    }

    fn jewel_row(&self, col: usize, id: u64) -> usize {
        if let Some(glint) = self.glint(id) {
            return glint.row;
        }
        let top = self.pile_top(col);
        let floor = SHORE_ROWS.saturating_add(3).min(GRID_H - 1);
        if top <= floor {
            return floor.min(top.saturating_sub(1));
        }
        let span = (top - floor).max(1);
        floor + ((id as usize).wrapping_mul(17) % span)
    }

    fn first_empty(&self, row: usize, rows: usize) -> Option<(usize, usize)> {
        for y in row..(row + rows).min(GRID_H) {
            for x in 0..GRID_W {
                if self.cell(x, y).is_none() {
                    return Some((x, y));
                }
            }
        }
        None
    }

    fn cells_in_column(&self, x: usize) -> u32 {
        cells_in_column_of(&self.cells, x)
    }

    fn push_aside(&mut self, from_x: i32, from_y: i32, to_x: i32, to_y: i32) {
        if !self.in_bounds(from_x, from_y) || !self.in_bounds(to_x, to_y) {
            return;
        }
        let from = self.cell(from_x as usize, from_y as usize);
        let Some(cell) = from else {
            return;
        };
        if self.cell(to_x as usize, to_y as usize).is_some() {
            return;
        }
        self.set_cell(to_x as usize, to_y as usize, Some(cell));
        self.set_cell(from_x as usize, from_y as usize, None);
    }

    fn oil_rim(&self, x: usize, y: usize) -> bool {
        for (dx, dy) in [(0, 1), (0, -1), (1, 0), (-1, 0)] {
            let nx = x as i32 + dx;
            let ny = y as i32 + dy;
            if !self.in_bounds(nx, ny) {
                return true;
            }
            if !self.cell(nx as usize, ny as usize).is_some_and(|cell| cell.kind == GrainKind::Oil) {
                return true;
            }
        }
        false
    }

    fn in_bounds(&self, x: i32, y: i32) -> bool {
        x >= 0 && y >= 0 && (x as usize) < GRID_W && (y as usize) < GRID_H
    }

    fn set_cell(&mut self, x: usize, y: usize, cell: Option<Cell>) {
        let i = self.idx(x, y);
        let was = self.cells[i].is_some();
        let now = cell.is_some();
        if was && !now {
            self.occupied = self.occupied.saturating_sub(1);
        } else if !was && now {
            self.occupied += 1;
        }
        self.cells[i] = cell;
        self.dirty = true;
    }

    fn idx(&self, x: usize, y: usize) -> usize {
        x + y * GRID_W
    }
}

fn cells_in_column_of(cells: &[Option<Cell>], x: usize) -> u32 {
    let mut n = 0;
    for y in 0..GRID_H {
        if cells[x + y * GRID_W].is_some() {
            n += 1;
        }
    }
    n
}

fn grain_hash(x: usize, y: usize, n: u32) -> u32 {
    let mut h = x
        .wrapping_mul(0x9E37_79B1)
        .wrapping_add(y.wrapping_mul(0x85EB_CA77))
        .wrapping_add(n as usize * 0xC2B2_AE3D);
    h ^= h >> 16;
    h = h.wrapping_mul(0x7FEB_352D);
    h ^= h >> 15;
    h as u32
}

fn column_crest(x: usize, rows: usize) -> usize {
    if rows == 0 {
        return 0;
    }
    let stagger = (grain_hash(x, 1, 7) % 3) as usize;
    rows.saturating_sub(stagger)
}

/// Natural organic wandering pour site across the full glass frame width.
fn dune_site(cursor: usize) -> usize {
    let t = cursor as f32 * 0.16;
    let wave1 = (t * 0.72).sin() * 36.0;
    let wave2 = (t * 1.83 + 1.3).sin() * 9.0;
    let center = 48.0 + wave1 + wave2;
    (center.round() as i32).rem_euclid(GRID_W as i32) as usize
}

fn dune_crest(x: usize, rows: usize) -> usize {
    if rows == 0 {
        return 0;
    }
    let a = (x * 2) % 18;
    let b = (x * 5) % 11;
    let wave = a as i32 - 9 + (b as i32 - 5) / 2;
    let amp = (rows / 3).max(1) as i32;
    let delta = wave * amp / 9;
    (rows as i32 + delta).clamp(1, rows as i32) as usize
}

fn ground_rgb(x: usize, y: usize, theme: SandTheme) -> (u8, u8, u8) {
    let depth = 1.0 - y as f32 / GRID_H as f32;
    let shade = 22.0 - depth * 10.0;
    let n = ((x.wrapping_mul(13) + y.wrapping_mul(29)) % 5) as f32 - 2.0;
    let base_r = shade + 2.0 + n;
    let base_g = shade + 6.0;
    let base_b = shade + 16.0 - n;

    let (tr, tg, tb, keep) = match theme {
        SandTheme::GoldenSunrise => (28.0, 20.0, 12.0, 0.70),
        SandTheme::Bioluminescent => (8.0, 26.0, 36.0, 0.65),
        SandTheme::VolcanicObsidian => (24.0, 12.0, 10.0, 0.70),
        SandTheme::CoralPink => (32.0, 14.0, 20.0, 0.68),
        SandTheme::Snow => (20.0, 26.0, 38.0, 0.65),
        SandTheme::Halloween => (24.0, 10.0, 32.0, 0.68),
        SandTheme::Space => (10.0, 12.0, 30.0, 0.65),
    };

    let r = (base_r * keep + tr * (1.0 - keep)).round().clamp(0.0, 255.0) as u8;
    let g = (base_g * keep + tg * (1.0 - keep)).round().clamp(0.0, 255.0) as u8;
    let b = (base_b * keep + tb * (1.0 - keep)).round().clamp(0.0, 255.0) as u8;
    (r, g, b)
}

fn shade_grain(c: Cell, x: usize, y: usize, px: u32, py: u32, crest: bool) -> (u8, u8, u8) {
    let salt = grain_hash(x, y, px);
    if salt % 53 == 0 && py == 1 {
        return (248, 236, 206);
    }
    let n = (salt % 31) as i16 - 15;
    let lit = if crest && py == 0 {
        18
    } else {
        0
    };
    let r = (c.r as i16 + n + lit).clamp(0, 255) as u8;
    let g = (c.g as i16 + n / 2 + lit / 2).clamp(0, 255) as u8;
    let b = (c.b as i16).clamp(0, 255) as u8;
    (r, g, b)
}

fn shade_oil(c: Cell, px: u32, py: u32, rim: bool) -> (u8, u8, u8) {
    let edge = rim && (px == 0 || py == 0 || px + 1 == CELL_PX);
    if edge {
        return (
            c.r.saturating_add(48).min(255),
            c.g.saturating_add(36).min(255),
            c.b.saturating_add(18).min(255),
        );
    }
    (c.r, c.g, c.b)
}

fn shade_bubble(px: u32, py: u32, ground: (u8, u8, u8)) -> (u8, u8, u8) {
    let cx = px as i32 - (CELL_PX as i32 / 2);
    let cy = py as i32 - (CELL_PX as i32 / 2);
    if cx == 0 && cy == 0 {
        (236, 246, 250)
    } else if cx.abs() + cy.abs() <= 1 {
        (198, 220, 230)
    } else {
        (
            ground.0.saturating_add(10),
            ground.1.saturating_add(14),
            ground.2.saturating_add(18),
        )
    }
}

fn paint_jewel(data: &mut [u8], col: usize, row: usize, rgb: [u8; 3], score: f64) {
    let reach = if score >= 90.0 {
        10
    } else if score >= 80.0 {
        8
    } else {
        6
    };
    let reach_f = reach as f32;
    let body_r = reach_f * 0.52;
    let cx = col as i32 * CELL_PX as i32 + CELL_PX as i32 / 2;
    let cy = (GRID_H as i32 - 1 - row as i32) * CELL_PX as i32 + CELL_PX as i32 / 2;
    let reach_sq = reach * reach;
    for dy in -reach..=reach {
        for dx in -reach..=reach {
            let d2 = dx * dx + dy * dy;
            if d2 > reach_sq {
                continue;
            }
            let x = cx + dx;
            let y = cy + dy;
            if x < 0 || y < 0 || x >= TEX_W as i32 || y >= TEX_H as i32 {
                continue;
            }
            let i = ((y as u32 * TEX_W + x as u32) * 4) as usize;
            let dist = (d2 as f32).sqrt();

            if dist <= 1.8 {
                // Radiant core highlight
                data[i] = 255;
                data[i + 1] = 252;
                data[i + 2] = 240;
                data[i + 3] = 255;
            } else if dist <= body_r {
                // Vibrant jewel tier color with slight bevel shine
                let shine = if dy < 0 && dx < 0 { 24 } else { 0 };
                data[i] = rgb[0].saturating_add(shine);
                data[i + 1] = rgb[1].saturating_add(shine);
                data[i + 2] = rgb[2].saturating_add(shine);
                data[i + 3] = 255;
            } else if dist <= reach_f {
                // Soft luminous aura blended with background
                let falloff = 1.0 - (dist - body_r) / (reach_f - body_r);
                let alpha = (falloff * 0.65).clamp(0.0, 1.0);
                let bg_r = data[i] as f32;
                let bg_g = data[i + 1] as f32;
                let bg_b = data[i + 2] as f32;
                data[i] = (rgb[0] as f32 * alpha + bg_r * (1.0 - alpha)).round() as u8;
                data[i + 1] = (rgb[1] as f32 * alpha + bg_g * (1.0 - alpha)).round() as u8;
                data[i + 2] = (rgb[2] as f32 * alpha + bg_b * (1.0 - alpha)).round() as u8;
                data[i + 3] = 255;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deposited_grain_stacks_then_spreads_into_a_dune() {
        let mut canvas = SandCanvas::new();
        assert!(canvas.deposit(4, Cell::rgb(10, 20, 30)));
        assert_eq!(canvas.cell(4, 0).unwrap().r, 10);
        assert!(canvas.deposit(4, Cell::rgb(40, 50, 60)));
        assert_eq!(canvas.cell(4, 1).unwrap().r, 40);

        for i in 0..40 {
            assert!(canvas.deposit(4, Cell::rgb(i as u8, 1, 1)));
        }
        assert!(canvas.pile_top(4) < 16, "tower {}", canvas.pile_top(4));
        assert!(canvas.pile_top(2) > 0 && canvas.pile_top(6) > 0);
        assert!(canvas.cell(4, 0).is_some());
    }

    #[test]
    fn one_pour_stays_in_a_narrow_neck() {
        let mut canvas = SandCanvas::new();
        let stroke = [Cell::rgb(200, 170, 90); 8];
        canvas.pour_stream(40, &stroke, None);
        for _ in 0..48 {
            canvas.step(64);
        }
        let mut cols_used = 0;
        for x in 0..GRID_W {
            if (0..GRID_H).any(|y| canvas.cell(x, y).is_some_and(|cell| cell.is_sand())) {
                cols_used += 1;
            }
        }
        assert!(cols_used > 0, "pour landed nothing");
        assert!(cols_used < 24, "neck spread across {cols_used} columns");
    }

    #[test]
    fn sand_sinks_through_water_and_stacks() {
        let mut canvas = SandCanvas::new();
        canvas.set_cell(8, 12, Some(Cell::rgb(180, 140, 70)));
        canvas.step(8);
        assert!(canvas.cell(8, 12).is_none());
        let landed = [7, 8, 9].iter().any(|x| canvas.cell(*x, 11).is_some_and(|cell| cell.is_sand()));
        assert!(landed);

        let mut canvas = SandCanvas::new();
        canvas.set_cell(4, 0, Some(Cell::rgb(10, 20, 30)));
        canvas.set_cell(4, 6, Some(Cell::rgb(40, 50, 60)));
        for _ in 0..12 {
            canvas.step(32);
        }
        assert_eq!(canvas.count_kind(GrainKind::Sand), 2);
        assert!(canvas.cell(4, 6).is_none());
    }

    #[test]
    fn a_bubble_rises_and_stays_under_a_sand_shelf() {
        let mut canvas = SandCanvas::new();
        canvas.set_cell(5, 2, Some(Cell::bubble()));
        canvas.step(16);
        assert!(canvas.cell(5, 2).is_none());
        let rose = (3..GRID_H).any(|y| {
            (0..GRID_W).any(|x| canvas.cell(x, y).is_some_and(|cell| cell.kind == GrainKind::Bubble))
        });
        assert!(rose);

        let mut canvas = SandCanvas::new();
        for x in 4..=6 {
            for y in 0..=5 {
                canvas.set_cell(x, y, Some(Cell::rgb(160, 120, 60)));
            }
        }
        canvas.set_cell(5, 4, Some(Cell::bubble()));
        for _ in 0..6 {
            canvas.step(64);
        }
        assert!(canvas.cell(5, 4).is_some_and(|cell| cell.kind == GrainKind::Bubble));
        assert!(canvas.cell(5, 5).is_some_and(|cell| cell.is_sand()));
    }

    #[test]
    fn oil_beads_instead_of_dissolving() {
        let mut canvas = SandCanvas::new();
        for y in 0..=19 {
            for x in 0..GRID_W {
                canvas.set_cell(x, y, Some(Cell::rgb(90, 60, 30)));
            }
        }
        canvas.set_cell(10, 20, Some(Cell::oil(180, 90, 30)));
        canvas.set_cell(12, 20, Some(Cell::oil(180, 90, 30)));
        for _ in 0..24 {
            canvas.step(80);
        }
        let mut oils = Vec::new();
        for y in 0..GRID_H {
            for x in 0..GRID_W {
                if canvas.cell(x, y).is_some_and(|cell| cell.kind == GrainKind::Oil) {
                    oils.push((x, y));
                }
            }
        }
        assert_eq!(oils.len(), 2, "oil dissolved");
        let (a, b) = (oils[0], oils[1]);
        let near = (a.0 as i32 - b.0 as i32).abs() <= 1 && (a.1 as i32 - b.1 as i32).abs() <= 1;
        assert!(near, "oil stayed apart at {a:?} {b:?}");
    }

    #[test]
    fn one_turnover_step_drops_a_single_floor_row() {
        let mut canvas = SandCanvas::new();
        for x in 0..GRID_W {
            canvas.set_cell(x, 0, Some(Cell::rgb(120, 80, 40)));
            canvas.set_cell(x, 1, Some(Cell::rgb(160, 110, 50)));
        }
        let before = canvas.occupied;
        canvas.begin_turn();
        assert!(canvas.drain_floor());
        assert!(canvas.occupied > 0);
        assert!(canvas.occupied < before);
        assert_eq!(canvas.occupied, GRID_W as u32);
        assert!(canvas.cell(0, 0).is_some_and(|cell| cell.r == 160));
    }

    #[test]
    fn a_section_is_buried_once_and_later_keeps_its_window() {
        let mut canvas = SandCanvas::new();
        for _ in 0..6 {
            canvas.deposit(12, Cell::rgb(180, 150, 90));
        }
        let first = canvas.bury_section(12, 4.0, 82.0, [80, 220, 255]);
        let again = canvas.bury_section(12, 4.02, 82.0, [80, 220, 255]);
        assert_eq!(first, again);
        assert!(canvas.has_span(4.0));
        canvas.attach_window(4.0, 8.0, 19);
        assert_eq!(canvas.glint(first).unwrap().window_id, 19);
    }

    #[test]
    fn empty_glass_is_opaque_and_a_glint_paints_a_jewel() {
        let canvas = SandCanvas::new();
        let rgba = canvas.to_rgba();
        assert!(rgba.chunks(4).all(|px| px[3] == 255));

        let mut canvas = SandCanvas::new();
        for _ in 0..8 {
            canvas.deposit(20, Cell::rgb(180, 150, 90));
        }
        canvas.bury(20, 7, 92.0, [80, 220, 255]);
        let rgba = canvas.to_rgba();
        let bright = rgba
            .chunks(4)
            .filter(|px| px[2] > 180 && px[2] > px[0].saturating_add(30))
            .count();
        assert!(bright > 8, "jewel pixels {bright}");
    }

    #[test]
    fn half_progress_falls_instead_of_filling_the_crest() {
        let mut canvas = SandCanvas::new();
        let added = canvas.feed_pour(50.0, false, |_, _, _| Cell::rgb(180, 140, 70));
        let limit = progress_rows(0.5);
        assert!(added > 0);
        assert!(limit < progress_rows(1.0));
        let on_floor = (0..GRID_W).filter(|x| canvas.cell(*x, 0).is_some()).count();
        assert!(on_floor < GRID_W / 4, "pour wrote {on_floor} floor cells");

        for _ in 0..700 {
            canvas.feed_pour(50.0, false, |_, _, _| Cell::rgb(180, 140, 70));
            canvas.step(48);
        }
        for _ in 0..GRID_H {
            canvas.step(48);
        }
        let mut highest = 0;
        for x in 0..GRID_W {
            let top = canvas.resting_height(x);
            highest = highest.max(top);
            assert!(top <= limit, "column {x} crossed the ceiling at {top}");
        }
        assert!(highest + 4 >= limit, "crest {highest} never reached {limit}");
        assert!(highest <= progress_rows(1.0));
    }

    #[test]
    fn a_slump_stays_near_the_column() {
        let mut canvas = SandCanvas::new();
        for y in 0..20 {
            canvas.set_cell(20, y, Some(Cell::rgb(180, 140, 70)));
        }
        assert!(canvas.deposit(20, Cell::rgb(255, 0, 0)));
        let red = (0..GRID_W).find(|&x| {
            (0..GRID_H).any(|y| canvas.cell(x, y).is_some_and(|cell| cell.r == 255))
        });
        let red = red.expect("red grain vanished");
        assert!((red as i32 - 20).abs() <= SLUMP_STEPS as i32, "slumped to {red}");
        assert!(red < GRID_W - 2, "slumped to the end of the frame");
        assert!(canvas.cell(20, 0).is_some_and(|cell| cell.r == 180));
    }

    #[test]
    fn a_short_clear_lets_out_ten_rows() {
        let mut canvas = SandCanvas::new();
        for y in 0..20 {
            for x in 0..GRID_W {
                canvas.set_cell(x, y, Some(Cell::rgb(120, 80, 40)));
            }
        }
        canvas.begin_turn();
        let mut steps = 0;
        while canvas.turning && steps < 30 {
            assert!(canvas.drain_floor());
            steps += 1;
        }
        assert_eq!(steps, 10);
        assert!(!canvas.turning);
    }

    #[test]
    fn full_load_releases_about_sixty_percent_of_the_old_bubbles() {
        let cap = loading_bubble_cap(1.0);
        assert_eq!(cap, 8);
        assert!(cap as f32 <= 14.0 * 0.6 + 0.01);
        let mut bubbles = 0;
        for column in 0..5 {
            if frame_bubble(1.0, column) {
                bubbles += 1;
            }
        }
        assert_eq!(bubbles, 3);
        assert!(!frame_bubble(0.4, 0));
    }

    #[test]
    fn water_has_no_bright_center_line() {
        let canvas = SandCanvas::new();
        let rgba = canvas.to_rgba();
        let px = |x: u32, y: u32| {
            let i = ((y * TEX_W + x) * 4) as usize;
            rgba[i] as i32
        };
        let mid = TEX_W / 2;
        let mut center = 0;
        let mut edge = 0;
        for y in (TEX_H / 2 - 5)..(TEX_H / 2 + 5) {
            center += px(mid, y);
            edge += px(2, y);
        }
        assert!(
            (center - edge).abs() < 25,
            "center {center} edge {edge}"
        );
    }

    #[test]
    fn shore_pour_adds_cells_without_clearing() {
        let mut canvas = SandCanvas::new();
        canvas.set_cell(0, 0, Some(Cell::rgb(12, 24, 36)));
        let added = canvas.feed_pour(20.0, false, |_, _, _| Cell::rgb(12, 24, 36));
        assert!(added > 0);
        assert!(!canvas.queue.is_empty());
        let before = canvas.occupied;
        for _ in 0..8 {
            canvas.feed_pour(55.0, false, |_, _, _| Cell::rgb(200, 10, 10));
            canvas.step(48);
        }
        assert!(canvas.occupied > before);
        assert_eq!(canvas.cell(0, 0).unwrap().r, 12);
    }

    #[test]
    fn a_fresh_grain_enters_at_the_top() {
        let mut canvas = SandCanvas::new();
        canvas.pour(10, Cell::rgb(9, 9, 9));
        canvas.step(12);
        let spot = (0..GRID_W).find_map(|x| {
            (0..GRID_H)
                .rev()
                .find(|&y| canvas.cell(x, y).is_some_and(|cell| cell.r == 9))
                .map(|y| (x, y))
        });
        let (x, y) = spot.expect("grain never entered");
        assert!(y > GRID_H - 4, "entered at row {y}");
        assert!((x as i32 - 10).abs() <= 1, "entered at column {x}");
        assert!(canvas.cell(x, 0).is_none());
    }

    #[test]
    fn brushing_cover_uncovers_a_buried_glint() {
        let mut canvas = SandCanvas::new();
        for _ in 0..6 {
            canvas.deposit(10, Cell::rgb(180, 140, 60));
        }
        let id = canvas.bury(10, 1, 88.0, [255, 210, 80]);
        assert!(canvas.glint(id).unwrap().buried_depth > 0.0);
        canvas.brush(10, 3, 8);
        assert_eq!(canvas.glint(id).unwrap().buried_depth, 0.0);
        assert!(canvas.occupied > 0);
    }

    #[test]
    fn sand_slides_diagonally_and_does_not_form_single_column_spire() {
        let mut canvas = SandCanvas::new();
        // Pour multiple grains into column 20
        for _ in 0..24 {
            canvas.deposit(20, Cell::rgb(200, 160, 80));
        }
        // If sand only stacked vertically, column 20 would have height 24 and neighbors would have 0.
        // With diagonal sliding and repose avalanches, neighbors must receive sand.
        let center_h = canvas.pile_top(20);
        let left_h = canvas.pile_top(19);
        let right_h = canvas.pile_top(21);
        assert!(center_h < 20, "column 20 formed a rigid spire of height {center_h}");
        assert!(left_h > 0 || right_h > 0, "sand failed to cascade to adjacent columns");
    }

    #[test]
    fn buried_jewel_retains_fixed_row_even_when_pile_top_changes() {
        let mut canvas = SandCanvas::new();
        for _ in 0..10 {
            canvas.deposit(15, Cell::rgb(190, 150, 70));
        }
        let id = canvas.bury(15, 1, 95.0, [255, 220, 90]);
        let initial_row = canvas.glint(id).unwrap().row;
        let layout = ChamberLayout::from_window(300.0, 400.0);
        let initial_center = canvas.jewel_center(&layout, id).unwrap();

        // Deposit many more grains in column 15 to change pile height
        for _ in 0..20 {
            canvas.deposit(15, Cell::rgb(190, 150, 70));
        }
        let updated_row = canvas.glint(id).unwrap().row;
        let updated_center = canvas.jewel_center(&layout, id).unwrap();

        assert_eq!(initial_row, updated_row, "jewel row changed after more sand was deposited");
        assert_eq!(initial_center, updated_center, "jewel world position changed after more sand was deposited");
    }

    #[test]
    fn clear_unsettled_above_limit_removes_ceiling_sand() {
        let mut canvas = SandCanvas::new();
        let limit = canvas.fill_limit;
        canvas.set_cell(10, limit + 2, Some(Cell::rgb(100, 80, 50)));
        canvas.set_cell(11, limit + 5, Some(Cell::rgb(100, 80, 50)));
        assert!(canvas.cell(10, limit + 2).is_some());
        assert!(canvas.cell(11, limit + 5).is_some());

        canvas.clear_unsettled_above_limit();

        assert!(canvas.cell(10, limit + 2).is_none(), "sand above fill_limit was not cleared");
        assert!(canvas.cell(11, limit + 5).is_none(), "sand above fill_limit was not cleared");
    }

    #[test]
    fn sand_settles_and_stops_simulating_when_quiescent() {
        let mut canvas = SandCanvas::new();
        canvas.deposit(30, Cell::rgb(200, 150, 80));
        // Step enough times for avalanche and fall to finish
        for _ in 0..64 {
            canvas.step(16);
        }
        assert!(canvas.is_settled(), "canvas should have marked itself settled");
        let occupied_before = canvas.occupied;
        let clock_before = canvas.step_clock;
        canvas.step(16);
        assert_eq!(canvas.step_clock, clock_before, "step clock should not advance when settled");
        assert_eq!(canvas.occupied, occupied_before, "occupied cells should not change when settled");
    }

    #[test]
    fn applying_theme_immediately_recolors_existing_sand_and_marks_dirty() {
        let mut canvas = SandCanvas::new();
        let stamped_gold = SandTheme::GoldenSunrise.stamp_color(200, 100, 50);
        canvas.deposit(10, Cell::with_base(stamped_gold[0], stamped_gold[1], stamped_gold[2], 200, 100, 50));

        let cell_before = canvas.cell(10, 0).unwrap();
        assert_eq!(cell_before.r, stamped_gold[0]);
        assert_eq!(cell_before.g, stamped_gold[1]);
        assert_eq!(cell_before.b, stamped_gold[2]);

        canvas.dirty = false;
        canvas.apply_theme(SandTheme::Bioluminescent);

        assert!(canvas.dirty, "canvas must be marked dirty when theme changes");
        assert_eq!(canvas.active_theme, SandTheme::Bioluminescent);

        let cell_after = canvas.cell(10, 0).unwrap();
        let stamped_bio = SandTheme::Bioluminescent.stamp_color(200, 100, 50);
        assert_eq!(cell_after.r, stamped_bio[0]);
        assert_eq!(cell_after.g, stamped_bio[1]);
        assert_eq!(cell_after.b, stamped_bio[2]);
        assert_ne!(cell_before.b, cell_after.b, "blue channel should differ between GoldenSunrise and Bioluminescent");
    }

    #[test]
    fn theme_change_alters_to_rgba_even_on_empty_canvas() {
        let mut canvas = SandCanvas::new();
        let rgba_gold = canvas.to_rgba();

        canvas.apply_theme(SandTheme::Bioluminescent);
        let rgba_bio = canvas.to_rgba();

        assert_ne!(rgba_gold, rgba_bio, "fluid ground in to_rgba must visually reflect active theme");
    }
}
