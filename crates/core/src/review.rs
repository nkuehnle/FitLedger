//! Library seeding and mapping imported exercise names to library exercises.

use std::collections::HashMap;

use rusqlite::{OptionalExtension, params};
use serde::Serialize;

use crate::Result;
use crate::db::Db;
use crate::library;
use crate::matcher::{AUTO_ACCEPT, Candidate, Index};

#[derive(Debug, Clone, Serialize)]
pub struct NoteEra {
    pub note: Option<String>,
    pub from: String,
    pub to: String,
    pub sessions: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct MappingRule {
    pub id: i64,
    pub from_date: Option<String>,
    pub to_date: Option<String>,
    pub exercise_slug: String,
    pub exercise_name: String,
    pub confidence: Option<f64>,
    pub confirmed: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct SuggestionView {
    pub slug: String,
    pub name: String,
    pub equipment: String,
    pub confidence: f64,
}

/// Everything the review screen needs about one imported exercise name.
#[derive(Debug, Clone, Serialize)]
pub struct NameReview {
    pub source_name: String,
    pub sessions: usize,
    pub sets: usize,
    pub first_date: String,
    pub last_date: String,
    pub note_eras: Vec<NoteEra>,
    pub rules: Vec<MappingRule>,
    pub suggestions: Vec<SuggestionView>,
    /// Sessions no rule covers.
    pub unmapped_sessions: usize,
    /// Some sessions unmapped, or mapped only by a low-confidence guess.
    pub needs_review: bool,
}

struct OwnedCandidate {
    slug: String,
    name: String,
    names: Vec<String>,
    equipment: String,
}

impl Db {
    /// Copies the embedded library into the database when it has changed. Exercises the user has
    /// edited (`user_modified`) are left alone.
    pub(crate) fn seed_library(&mut self) -> Result<()> {
        let version = format!("{:x}", library::version());
        if self.setting("library_version")?.as_deref() == Some(version.as_str()) {
            return Ok(());
        }
        let lib = library::library();
        let tx = self.conn_mut().transaction()?;
        for (i, g) in lib.groups.iter().enumerate() {
            tx.execute(
                "INSERT INTO muscle_group (id, name, position) VALUES (?1, ?2, ?3)
                 ON CONFLICT (id) DO UPDATE SET name = excluded.name, position = excluded.position",
                params![g.id, g.name, i as i64],
            )?;
        }
        for (i, m) in lib.muscles.iter().enumerate() {
            tx.execute(
                "INSERT INTO muscle (id, name, group_id, position) VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT (id) DO UPDATE SET name = excluded.name, group_id = excluded.group_id, position = excluded.position",
                params![m.id, m.name, m.group, i as i64],
            )?;
        }
        {
            let mut upsert = tx.prepare(
                "INSERT INTO exercise (slug, name, equipment, pattern, kind, source)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                 ON CONFLICT (slug) DO UPDATE SET name = excluded.name, equipment = excluded.equipment,
                     pattern = excluded.pattern, kind = excluded.kind, source = excluded.source
                 WHERE exercise.user_modified = 0",
            )?;
            let mut lookup = tx.prepare("SELECT id, user_modified FROM exercise WHERE slug = ?1")?;
            let mut del_alias = tx.prepare("DELETE FROM exercise_alias WHERE exercise_id = ?1")?;
            let mut del_muscle = tx.prepare("DELETE FROM exercise_muscle WHERE exercise_id = ?1")?;
            let mut ins_alias = tx.prepare("INSERT OR IGNORE INTO exercise_alias (exercise_id, alias) VALUES (?1, ?2)")?;
            let mut ins_muscle =
                tx.prepare("INSERT INTO exercise_muscle (exercise_id, muscle_id, role) VALUES (?1, ?2, ?3)")?;
            for e in &lib.exercises {
                upsert.execute(params![e.slug, e.name, e.equipment, e.pattern, e.kind, e.source])?;
                let (id, modified): (i64, bool) = lookup.query_row([&e.slug], |r| Ok((r.get(0)?, r.get(1)?)))?;
                if modified {
                    continue;
                }
                del_alias.execute([id])?;
                del_muscle.execute([id])?;
                for a in &e.aliases {
                    ins_alias.execute(params![id, a])?;
                }
                for m in &e.primary {
                    ins_muscle.execute(params![id, m, "primary"])?;
                }
                for m in &e.secondary {
                    ins_muscle.execute(params![id, m, "secondary"])?;
                }
            }
        }
        tx.execute(
            "INSERT INTO setting (key, value) VALUES ('library_version', ?1)
             ON CONFLICT (key) DO UPDATE SET value = excluded.value",
            [&version],
        )?;
        tx.commit()?;
        Ok(())
    }

    fn match_candidates(&self) -> Result<Vec<OwnedCandidate>> {
        let mut aliases: HashMap<i64, Vec<String>> = HashMap::new();
        let mut stmt = self.conn().prepare("SELECT exercise_id, alias FROM exercise_alias")?;
        for row in stmt.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))? {
            let (id, a) = row?;
            aliases.entry(id).or_default().push(a);
        }
        let mut stmt = self.conn().prepare("SELECT id, slug, name, coalesce(equipment, '') FROM exercise WHERE hidden = 0")?;
        let rows = stmt.query_map([], |r| {
            Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?, r.get::<_, String>(3)?))
        })?;
        let mut out = Vec::new();
        for row in rows {
            let (id, slug, name, equipment) = row?;
            let mut names = vec![name.clone()];
            names.extend(aliases.remove(&id).unwrap_or_default());
            out.push(OwnedCandidate { slug, name, names, equipment });
        }
        Ok(out)
    }

    fn match_index(cands: &[OwnedCandidate]) -> Index {
        let borrowed: Vec<Candidate> = cands
            .iter()
            .map(|c| Candidate { slug: &c.slug, names: c.names.iter().map(String::as_str).collect(), equipment: &c.equipment })
            .collect();
        Index::new(&borrowed)
    }

    fn suggestions_for(cands: &[OwnedCandidate], index: &Index, name: &str, limit: usize) -> Vec<SuggestionView> {
        let by_slug: HashMap<&str, &OwnedCandidate> = cands.iter().map(|c| (c.slug.as_str(), c)).collect();
        index
            .suggest(name, limit)
            .into_iter()
            .map(|s| {
                let c = by_slug[s.slug.as_str()];
                SuggestionView { slug: s.slug, name: c.name.clone(), equipment: c.equipment.clone(), confidence: s.confidence }
            })
            .collect()
    }

    /// Fuzzy search over exercise names and aliases, for picking a mapping by hand.
    pub fn search_exercises(&self, query: &str, limit: usize) -> Result<Vec<SuggestionView>> {
        let cands = self.match_candidates()?;
        let index = Self::match_index(&cands);
        Ok(Self::suggestions_for(&cands, &index, query, limit))
    }

    fn unmapped_names(&self) -> Result<Vec<String>> {
        let mut stmt = self.conn().prepare(
            "SELECT DISTINCT source_name FROM workout_exercise
             WHERE source_name NOT IN (SELECT source_name FROM name_mapping) ORDER BY source_name",
        )?;
        let names = stmt.query_map([], |r| r.get(0))?.collect::<rusqlite::Result<_>>()?;
        Ok(names)
    }

    /// Maps every name that has no rule yet to its best suggestion when the match is confident.
    /// Returns how many names were mapped.
    pub fn auto_map_names(&mut self) -> Result<usize> {
        let cands = self.match_candidates()?;
        let index = Self::match_index(&cands);
        let mut mapped = 0;
        for name in self.unmapped_names()? {
            let Some(best) = Self::suggestions_for(&cands, &index, &name, 1).pop() else { continue };
            if best.confidence >= AUTO_ACCEPT {
                let id = self.exercise_id(&best.slug)?.expect("suggested slug exists");
                self.conn().execute(
                    "INSERT INTO name_mapping (source_name, exercise_id, confidence, confirmed) VALUES (?1, ?2, ?3, 0)",
                    params![name, id, best.confidence],
                )?;
                self.apply_mappings(&name)?;
                mapped += 1;
            }
        }
        Ok(mapped)
    }

    /// Sets the mapping for `source_name`, for all history (`from`/`to` both `None`) or for an
    /// inclusive date range (`YYYY-MM-DD`). Replaces any rule with the same range.
    pub fn set_name_mapping(&mut self, source_name: &str, from: Option<&str>, to: Option<&str>, slug: &str) -> Result<()> {
        let id = self
            .exercise_id(slug)?
            .ok_or_else(|| crate::Error::Format(format!("unknown exercise {slug:?}")))?;
        self.conn().execute(
            "DELETE FROM name_mapping WHERE source_name = ?1 AND from_date IS ?2 AND to_date IS ?3",
            params![source_name, from, to],
        )?;
        self.conn().execute(
            "INSERT INTO name_mapping (source_name, from_date, to_date, exercise_id, confirmed) VALUES (?1, ?2, ?3, ?4, 1)",
            params![source_name, from, to, id],
        )?;
        self.apply_mappings(source_name)
    }

    pub fn remove_name_mapping(&mut self, rule_id: i64) -> Result<()> {
        let name: Option<String> = self
            .conn()
            .query_row("SELECT source_name FROM name_mapping WHERE id = ?1", [rule_id], |r| r.get(0))
            .optional()?;
        if let Some(name) = name {
            self.conn().execute("DELETE FROM name_mapping WHERE id = ?1", [rule_id])?;
            self.apply_mappings(&name)?;
        }
        Ok(())
    }

    pub fn confirm_name_mapping(&mut self, rule_id: i64) -> Result<()> {
        self.conn().execute("UPDATE name_mapping SET confirmed = 1 WHERE id = ?1", [rule_id])?;
        Ok(())
    }

    /// Recomputes `workout_exercise.exercise_id` for one imported name from its rules:
    /// whole-history rules first, then date-ranged ones on top.
    fn apply_mappings(&mut self, source_name: &str) -> Result<()> {
        let tx = self.conn_mut().transaction()?;
        tx.execute("UPDATE workout_exercise SET exercise_id = NULL WHERE source_name = ?1", [source_name])?;
        tx.execute(
            "UPDATE workout_exercise SET exercise_id = (
                 SELECT m.exercise_id FROM name_mapping m, workout w
                 WHERE m.source_name = workout_exercise.source_name AND w.id = workout_exercise.workout_id
                   AND substr(w.started_at_local, 1, 10) BETWEEN coalesce(m.from_date, '0000-00-00') AND coalesce(m.to_date, '9999-99-99')
                 ORDER BY (m.from_date IS NOT NULL OR m.to_date IS NOT NULL) DESC, m.id DESC
                 LIMIT 1)
             WHERE source_name = ?1",
            [source_name],
        )?;
        tx.commit()?;
        Ok(())
    }

    pub fn exercise_id(&self, slug: &str) -> Result<Option<i64>> {
        Ok(self.conn().query_row("SELECT id FROM exercise WHERE slug = ?1", [slug], |r| r.get(0)).optional()?)
    }

    /// Review data for every imported exercise name, names needing attention first.
    pub fn name_review(&self) -> Result<Vec<NameReview>> {
        let cands = self.match_candidates()?;
        let index = Self::match_index(&cands);
        let mut stmt = self.conn().prepare(
            "SELECT we.source_name, COUNT(DISTINCT we.id), COUNT(s.id),
                    min(substr(w.started_at_local, 1, 10)), max(substr(w.started_at_local, 1, 10))
             FROM workout_exercise we JOIN workout w ON w.id = we.workout_id
             LEFT JOIN workout_set s ON s.workout_exercise_id = we.id
             GROUP BY we.source_name ORDER BY we.source_name",
        )?;
        let names: Vec<(String, usize, usize, String, String)> = stmt
            .query_map([], |r| Ok((r.get(0)?, r.get::<_, i64>(1)? as usize, r.get::<_, i64>(2)? as usize, r.get(3)?, r.get(4)?)))?
            .collect::<rusqlite::Result<_>>()?;

        let mut out = Vec::new();
        for (source_name, sessions, sets, first_date, last_date) in names {
            let rules = self.rules_for(&source_name)?;
            let unmapped_sessions: i64 = self.conn().query_row(
                "SELECT COUNT(*) FROM workout_exercise WHERE source_name = ?1 AND exercise_id IS NULL",
                [&source_name],
                |r| r.get(0),
            )?;
            let unmapped_sessions = unmapped_sessions as usize;
            let needs_review = unmapped_sessions > 0
                || rules.iter().any(|r| !r.confirmed && r.confidence.unwrap_or(0.0) < AUTO_ACCEPT);
            out.push(NameReview {
                note_eras: self.note_eras(&source_name)?,
                suggestions: Self::suggestions_for(&cands, &index, &source_name, 5),
                rules,
                unmapped_sessions,
                needs_review,
                source_name,
                sessions,
                sets,
                first_date,
                last_date,
            });
        }
        out.sort_by_key(|r| !r.needs_review);
        Ok(out)
    }

    fn rules_for(&self, source_name: &str) -> Result<Vec<MappingRule>> {
        let mut stmt = self.conn().prepare(
            "SELECT m.id, m.from_date, m.to_date, e.slug, e.name, m.confidence, m.confirmed
             FROM name_mapping m JOIN exercise e ON e.id = m.exercise_id
             WHERE m.source_name = ?1 ORDER BY coalesce(m.from_date, ''), m.id",
        )?;
        let rules = stmt
            .query_map([source_name], |r| {
                Ok(MappingRule {
                    id: r.get(0)?,
                    from_date: r.get(1)?,
                    to_date: r.get(2)?,
                    exercise_slug: r.get(3)?,
                    exercise_name: r.get(4)?,
                    confidence: r.get(5)?,
                    confirmed: r.get(6)?,
                })
            })?
            .collect::<rusqlite::Result<_>>()?;
        Ok(rules)
    }

    /// Consecutive sessions with the same note, oldest first. Shows when a name changed meaning.
    fn note_eras(&self, source_name: &str) -> Result<Vec<NoteEra>> {
        let mut stmt = self.conn().prepare(
            "SELECT substr(w.started_at_local, 1, 10), we.notes FROM workout_exercise we
             JOIN workout w ON w.id = we.workout_id WHERE we.source_name = ?1 ORDER BY w.started_at_local",
        )?;
        let mut eras: Vec<NoteEra> = Vec::new();
        for row in stmt.query_map([source_name], |r| Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?)))? {
            let (date, note) = row?;
            match eras.last_mut() {
                Some(e) if e.note == note => {
                    e.to = date;
                    e.sessions += 1;
                }
                _ => eras.push(NoteEra { note, from: date.clone(), to: date, sessions: 1 }),
            }
        }
        Ok(eras)
    }
}
