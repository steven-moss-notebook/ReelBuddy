use std::path::{Path, PathBuf};

use rusqlite::{params, Connection};

use crate::editor::ChoreView;
use crate::pet_state::{self, PetSnapshot};
use crate::taste;

pub struct Db {
    conn: Connection,
}

impl Db {
    pub fn open(path: &Path) -> Result<Self, String> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|err| err.to_string())?;
        }
        let conn = Connection::open(path).map_err(|err| err.to_string())?;
        conn.execute_batch(
            "
            PRAGMA journal_mode=WAL;
            CREATE TABLE IF NOT EXISTS sources (
              id INTEGER PRIMARY KEY,
              origin TEXT NOT NULL,
              duration_s REAL,
              status TEXT NOT NULL CHECK (status IN ('new','ready','error')),
              error TEXT,
              created_at TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS windows (
              id INTEGER PRIMARY KEY,
              source_id INTEGER NOT NULL,
              start_s REAL NOT NULL,
              end_s REAL NOT NULL,
              status TEXT NOT NULL CHECK (status IN ('candidate','scrap','picked','rendered')),
              h REAL, v REAL, e REAL, a REAL, p REAL, s REAL,
              crop_json TEXT,
              captions_json TEXT,
              note TEXT,
              created_at TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS clips (
              id INTEGER PRIMARY KEY,
              window_id INTEGER NOT NULL,
              path TEXT,
              hook TEXT,
              status TEXT NOT NULL CHECK (status IN ('rendering','ready','shipped','error')),
              error TEXT,
              created_at TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS chores (
              id INTEGER PRIMARY KEY,
              type TEXT NOT NULL CHECK (type IN ('hunt','cut','hook_duel','remix','ship')),
              status TEXT NOT NULL CHECK (status IN ('open','resolved','skipped')),
              source_id INTEGER,
              window_id INTEGER,
              clip_id INTEGER,
              payload_json TEXT NOT NULL,
              resolution_json TEXT,
              created_at TEXT NOT NULL,
              resolved_at TEXT
            );
            CREATE TABLE IF NOT EXISTS taste_events (
              id INTEGER PRIMARY KEY,
              reason TEXT NOT NULL,
              window_id INTEGER,
              created_at TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS pet (
              id INTEGER PRIMARY KEY CHECK (id = 1),
              hunger INTEGER NOT NULL,
              energy INTEGER NOT NULL,
              mood INTEGER NOT NULL,
              bond INTEGER NOT NULL,
              stage INTEGER NOT NULL,
              shipped INTEGER NOT NULL,
              line TEXT NOT NULL,
              updated_at TEXT NOT NULL
            );
            ",
        )
        .map_err(|err| err.to_string())?;
        ensure_window_columns(&conn)?;
        let db = Self { conn };
        if db.conn.query_row("SELECT COUNT(*) FROM pet", [], |row| row.get::<_, i64>(0)).unwrap_or(0) == 0 {
            let pet = PetSnapshot::seed_asleep(pet_state::seed_line());
            db.save_pet(&pet)?;
        }
        Ok(db)
    }

    pub fn load_pet(&self) -> Result<PetSnapshot, String> {
        self.conn
            .query_row(
                "SELECT hunger, energy, mood, bond, stage, shipped, line FROM pet WHERE id = 1",
                [],
                |row| {
                    Ok(PetSnapshot {
                        hunger: row.get::<_, i64>(0)? as u8,
                        energy: row.get::<_, i64>(1)? as u8,
                        mood: row.get::<_, i64>(2)? as u8,
                        bond: row.get::<_, i64>(3)? as u8,
                        stage: row.get::<_, i64>(4)? as u8,
                        shipped: row.get::<_, i64>(5)? as u32,
                        line: row.get(6)?,
                        model_present: false,
                        woke: false,
                        busy: false,
                        eating: false,
                        sulk: false,
                        collar: None,
                    })
                },
            )
            .map_err(|err| err.to_string())
    }

    pub fn save_pet(&self, pet: &PetSnapshot) -> Result<(), String> {
        self.conn
            .execute(
                "INSERT INTO pet (id, hunger, energy, mood, bond, stage, shipped, line, updated_at)
                 VALUES (1, ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
                 ON CONFLICT(id) DO UPDATE SET
                   hunger = excluded.hunger,
                   energy = excluded.energy,
                   mood = excluded.mood,
                   bond = excluded.bond,
                   stage = excluded.stage,
                   shipped = excluded.shipped,
                   line = excluded.line,
                   updated_at = excluded.updated_at",
                params![
                    pet.hunger as i64,
                    pet.energy as i64,
                    pet.mood as i64,
                    pet.bond as i64,
                    pet.stage as i64,
                    pet.shipped as i64,
                    pet.line,
                    now_unix().to_string(),
                ],
            )
            .map_err(|err| err.to_string())?;
        Ok(())
    }

    pub fn insert_source(&self, origin: &str, duration_s: Option<f64>, status: &str, error: Option<&str>) -> Result<i64, String> {
        self.conn
            .execute(
                "INSERT INTO sources (origin, duration_s, status, error, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![origin, duration_s, status, error, now_unix().to_string()],
            )
            .map_err(|err| err.to_string())?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn insert_window(&self, source_id: i64, start_s: f64, end_s: f64, status: &str) -> Result<i64, String> {
        self.conn
            .execute(
                "INSERT INTO windows (source_id, start_s, end_s, status, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![source_id, start_s, end_s, status, now_unix().to_string()],
            )
            .map_err(|err| err.to_string())?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn write_window_edit(
        &self,
        id: i64,
        status: &str,
        scores: Option<&crate::editor::Scores>,
        crop_json: Option<&str>,
        captions_json: Option<&str>,
        note: Option<&str>,
    ) -> Result<(), String> {
        if let Some(scores) = scores {
            self.conn
                .execute(
                    "UPDATE windows SET status=?1, h=?2, v=?3, e=?4, a=?5, p=?6, s=?7, crop_json=?8, captions_json=?9, note=?10 WHERE id=?11",
                    params![status, scores.h, scores.v, scores.e, scores.a, scores.p, scores.s, crop_json, captions_json, note, id],
                )
                .map_err(|err| err.to_string())?;
        } else {
            self.conn
                .execute(
                    "UPDATE windows SET status=?1, crop_json=?2, captions_json=?3, note=?4 WHERE id=?5",
                    params![status, crop_json, captions_json, note, id],
                )
                .map_err(|err| err.to_string())?;
        }
        Ok(())
    }

    pub fn write_window_offer(
        &self,
        id: i64,
        status: &str,
        scores: &crate::editor::Scores,
        crop_json: &str,
        captions_json: &str,
        note: &str,
        motion: &str,
        hooks_json: &str,
        lesson: &str,
        thumb_path: &str,
        segments_json: &str,
        framing_json: &str,
    ) -> Result<(), String> {
        self.conn
            .execute(
                "UPDATE windows SET status=?1, h=?2, v=?3, e=?4, a=?5, p=?6, s=?7, crop_json=?8, captions_json=?9, note=?10, motion=?11, hooks_json=?12, lesson=?13, thumb_path=?14, segments_json=?15, framing_json=?16 WHERE id=?17",
                params![
                    status,
                    scores.h,
                    scores.v,
                    scores.e,
                    scores.a,
                    scores.p,
                    scores.s,
                    crop_json,
                    captions_json,
                    note,
                    motion,
                    hooks_json,
                    lesson,
                    thumb_path,
                    segments_json,
                    framing_json,
                    id
                ],
            )
            .map_err(|err| err.to_string())?;
        Ok(())
    }

    pub fn nudge_window(&self, id: i64, start_s: f64, end_s: f64) -> Result<(), String> {
        self.conn
            .execute(
                "UPDATE windows SET start_s=?1, end_s=?2 WHERE id=?3",
                params![start_s, end_s, id],
            )
            .map_err(|err| err.to_string())?;
        Ok(())
    }

    pub fn window(&self, id: i64) -> Result<WindowRow, String> {
        self.conn
            .query_row(
                "SELECT id, source_id, start_s, end_s, status, s, crop_json, captions_json, note, motion, hooks_json, lesson, thumb_path, segments_json, framing_json FROM windows WHERE id=?1",
                params![id],
                |row| {
                    Ok(WindowRow {
                        id: row.get(0)?,
                        source_id: row.get(1)?,
                        start_s: row.get(2)?,
                        end_s: row.get(3)?,
                        status: row.get(4)?,
                        s: row.get(5)?,
                        crop_json: row.get(6)?,
                        captions_json: row.get(7)?,
                        note: row.get(8)?,
                        motion: row.get(9)?,
                        hooks_json: row.get(10)?,
                        lesson: row.get(11)?,
                        thumb_path: row.get(12)?,
                        segments_json: row.get(13)?,
                        framing_json: row.get(14)?,
                    })
                },
            )
            .map_err(|err| err.to_string())
    }

    pub fn source_origin(&self, id: i64) -> Result<String, String> {
        self.conn
            .query_row("SELECT origin FROM sources WHERE id=?1", params![id], |row| row.get(0))
            .map_err(|err| err.to_string())
    }

    pub fn insert_clip(&self, window_id: i64, path: &str, hook: &str, status: &str) -> Result<i64, String> {
        self.conn
            .execute(
                "INSERT INTO clips (window_id, path, hook, status, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![window_id, path, hook, status, now_unix().to_string()],
            )
            .map_err(|err| err.to_string())?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn clip(&self, id: i64) -> Result<ClipRow, String> {
        self.conn
            .query_row(
                "SELECT id, window_id, path, hook FROM clips WHERE id=?1",
                params![id],
                |row| {
                    Ok(ClipRow {
                        id: row.get(0)?,
                        window_id: row.get(1)?,
                        path: row.get(2)?,
                        hook: row.get(3)?,
                    })
                },
            )
            .map_err(|err| err.to_string())
    }

    pub fn update_clip(&self, id: i64, status: &str, path: Option<&str>, error: Option<&str>) -> Result<(), String> {
        self.conn
            .execute(
                "UPDATE clips SET status=?1, path=COALESCE(?2, path), error=?3 WHERE id=?4",
                params![status, path, error, id],
            )
            .map_err(|err| err.to_string())?;
        Ok(())
    }

    pub fn insert_chore(
        &self,
        kind: &str,
        source_id: Option<i64>,
        window_id: Option<i64>,
        clip_id: Option<i64>,
        payload_json: &str,
    ) -> Result<i64, String> {
        self.conn
            .execute(
                "INSERT INTO chores (type, status, source_id, window_id, clip_id, payload_json, created_at)
                 VALUES (?1, 'open', ?2, ?3, ?4, ?5, ?6)",
                params![kind, source_id, window_id, clip_id, payload_json, now_unix().to_string()],
            )
            .map_err(|err| err.to_string())?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn rewrite_chore(&self, id: i64, payload_json: &str) -> Result<(), String> {
        self.conn
            .execute(
                "UPDATE chores SET payload_json=?1 WHERE id=?2",
                params![payload_json, id],
            )
            .map_err(|err| err.to_string())?;
        Ok(())
    }

    pub fn resolve_chore(&self, id: i64, status: &str, resolution_json: &str) -> Result<(), String> {
        self.conn
            .execute(
                "UPDATE chores SET status=?1, resolution_json=?2, resolved_at=?3 WHERE id=?4",
                params![status, resolution_json, now_unix().to_string(), id],
            )
            .map_err(|err| err.to_string())?;
        Ok(())
    }

    pub fn open_count(&self) -> Result<i64, String> {
        self.conn
            .query_row("SELECT COUNT(*) FROM chores WHERE status='open'", [], |row| row.get(0))
            .map_err(|err| err.to_string())
    }

    pub fn open_chore(&self) -> Result<Option<(i64, String, i64)>, String> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, payload_json, created_at FROM chores WHERE status='open' ORDER BY id LIMIT 1")
            .map_err(|err| err.to_string())?;
        let mut rows = stmt.query([]).map_err(|err| err.to_string())?;
        if let Some(row) = rows.next().map_err(|err| err.to_string())? {
            let created: String = row.get(2).map_err(|err| err.to_string())?;
            Ok(Some((
                row.get(0).map_err(|err| err.to_string())?,
                row.get(1).map_err(|err| err.to_string())?,
                created.parse().unwrap_or(0),
            )))
        } else {
            Ok(None)
        }
    }

    pub fn insert_taste(&self, reason: &str, window_id: Option<i64>) -> Result<(), String> {
        self.conn
            .execute(
                "INSERT INTO taste_events (reason, window_id, created_at) VALUES (?1, ?2, ?3)",
                params![reason, window_id, now_unix().to_string()],
            )
            .map_err(|err| err.to_string())?;
        Ok(())
    }

    pub fn taste_counts(&self) -> Result<Vec<(String, u32)>, String> {
        let mut stmt = self
            .conn
            .prepare("SELECT reason, COUNT(*) FROM taste_events GROUP BY reason")
            .map_err(|err| err.to_string())?;
        let rows = stmt
            .query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)? as u32)))
            .map_err(|err| err.to_string())?;
        rows.collect::<Result<Vec<_>, _>>().map_err(|err| err.to_string())
    }

    pub fn banned_reasons(&self) -> Result<Vec<String>, String> {
        Ok(self
            .taste_counts()?
            .into_iter()
            .filter(|(reason, n)| *n >= 3 && taste::REASONS.contains(&reason.as_str()))
            .map(|(reason, _)| reason)
            .collect())
    }

    pub fn accept_reject_counts(&self) -> Result<(u32, u32), String> {
        let accepts: i64 = self
            .conn
            .query_row(
                "SELECT COUNT(*) FROM chores WHERE resolution_json LIKE '%\"accept\"%'",
                [],
                |row| row.get(0),
            )
            .unwrap_or(0);
        let rejects = self
            .taste_counts()
            .unwrap_or_default()
            .into_iter()
            .filter(|(reason, _)| taste::REASONS.contains(&reason.as_str()))
            .map(|(_, n)| n)
            .sum();
        Ok((accepts as u32, rejects))
    }

    pub fn recent_shipped_scores(&self) -> Result<Vec<f64>, String> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT w.s FROM clips c JOIN windows w ON w.id = c.window_id
                 WHERE c.status='shipped' AND w.s IS NOT NULL ORDER BY c.id DESC LIMIT 5",
            )
            .map_err(|err| err.to_string())?;
        let rows = stmt
            .query_map([], |row| row.get::<_, f64>(0))
            .map_err(|err| err.to_string())?;
        rows.collect::<Result<Vec<_>, _>>().map_err(|err| err.to_string())
    }

    pub fn remix_on_day(&self, day: &str) -> Result<bool, String> {
        let mut stmt = self
            .conn
            .prepare("SELECT created_at FROM chores WHERE type='remix'")
            .map_err(|err| err.to_string())?;
        let rows = stmt
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(|err| err.to_string())?;
        for created in rows {
            let created = created.map_err(|err| err.to_string())?;
            let unix = created.parse::<i64>().unwrap_or(0);
            if taste::utc_date(unix) == day {
                return Ok(true);
            }
        }
        Ok(false)
    }

    pub fn latest_ready_clip(&self) -> Result<Option<ClipRow>, String> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT c.id, c.window_id, c.path, c.hook FROM clips c
                 WHERE c.status IN ('ready','shipped') ORDER BY c.id DESC LIMIT 1",
            )
            .map_err(|err| err.to_string())?;
        let mut rows = stmt.query([]).map_err(|err| err.to_string())?;
        if let Some(row) = rows.next().map_err(|err| err.to_string())? {
            Ok(Some(ClipRow {
                id: row.get(0).map_err(|err| err.to_string())?,
                window_id: row.get(1).map_err(|err| err.to_string())?,
                path: row.get(2).map_err(|err| err.to_string())?,
                hook: row.get(3).map_err(|err| err.to_string())?,
            }))
        } else {
            Ok(None)
        }
    }

    pub fn restore_chore(payload: &str, id: i64) -> Option<ChoreView> {
        let mut view: ChoreView = serde_json::from_str(payload).ok()?;
        view.id = id;
        Some(view)
    }
}

#[derive(Clone, Debug)]
pub struct WindowRow {
    pub id: i64,
    pub source_id: i64,
    pub start_s: f64,
    pub end_s: f64,
    pub status: String,
    pub s: Option<f64>,
    pub crop_json: Option<String>,
    pub captions_json: Option<String>,
    pub note: Option<String>,
    pub motion: Option<String>,
    pub hooks_json: Option<String>,
    pub lesson: Option<String>,
    pub thumb_path: Option<String>,
    pub segments_json: Option<String>,
    pub framing_json: Option<String>,
}

impl WindowRow {
    pub fn segments(&self) -> Vec<(f64, f64)> {
        if let Some(json) = &self.segments_json {
            if let Ok(segs) = serde_json::from_str::<Vec<(f64, f64)>>(json) {
                if !segs.is_empty() {
                    return segs;
                }
            }
        }
        vec![(self.start_s, self.end_s)]
    }

    pub fn total_duration(&self) -> f64 {
        self.segments().iter().map(|(s, e)| (e - s).max(0.0)).sum()
    }
}

fn ensure_window_columns(conn: &Connection) -> Result<(), String> {
    let mut stmt = conn.prepare("PRAGMA table_info(windows)").map_err(|err| err.to_string())?;
    let names: Vec<String> = stmt
        .query_map([], |row| row.get::<_, String>(1))
        .map_err(|err| err.to_string())?
        .filter_map(Result::ok)
        .collect();
    for (column, kind) in [
        ("motion", "TEXT"),
        ("hooks_json", "TEXT"),
        ("lesson", "TEXT"),
        ("thumb_path", "TEXT"),
        ("segments_json", "TEXT"),
        ("framing_json", "TEXT"),
    ] {
        if !names.iter().any(|name| name == column) {
            conn.execute(&format!("ALTER TABLE windows ADD COLUMN {column} {kind}"), [])
                .map_err(|err| err.to_string())?;
        }
    }
    Ok(())
}

#[derive(Clone, Debug)]
pub struct ClipRow {
    pub id: i64,
    pub window_id: i64,
    pub path: Option<String>,
    pub hook: Option<String>,
}

pub fn db_path(app_data: &Path) -> PathBuf {
    app_data.join("reelbuddy.db")
}

pub fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_db() -> (std::path::PathBuf, Db) {
        let dir = std::env::temp_dir().join(format!("reelbuddy-db-{}-{}", std::process::id(), now_unix()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("reelbuddy.db");
        let db = Db::open(&path).unwrap();
        (dir, db)
    }

    #[test]
    fn pet_and_taste_survive_a_reopen() {
        let (dir, db) = temp_db();
        let mut pet = db.load_pet().unwrap();
        pet.hunger = 77;
        pet.line = "kept".into();
        db.save_pet(&pet).unwrap();
        db.insert_taste("slow_open", None).unwrap();
        db.insert_taste("slow_open", None).unwrap();
        db.insert_taste("slow_open", None).unwrap();
        drop(db);
        let db = Db::open(&dir.join("reelbuddy.db")).unwrap();
        let loaded = db.load_pet().unwrap();
        assert_eq!(loaded.hunger, 77);
        assert_eq!(loaded.line, "kept");
        let card = taste::card_line(&db.taste_counts().unwrap()).unwrap();
        assert!(card.contains("slow_open x3"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
