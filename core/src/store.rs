//! Stockage local et requêtes.
//!
//! Le parti pris structurant : les récurrences sont développées à l'import et
//! écrites en dur dans `occurrences`. Afficher une journée ou une semaine
//! devient alors un simple balayage d'index, et « quel est mon prochain cours »
//! une requête à une ligne. Le flux `.ics` d'origine est conservé pour pouvoir
//! re-développer plus loin dans le temps sans redemander le fichier.
//!
//! Les champs structurés de la description et les décisions des règles suivent
//! le même principe : matérialisés à l'écriture, rejoués d'un bloc quand une
//! règle change. Une vue ne calcule jamais rien, elle lit.

use chrono::{DateTime, Datelike, Duration, NaiveDate, TimeZone, Timelike, Utc};
use chrono_tz::Tz;
use rusqlite::{Connection, OptionalExtension, params};

use crate::conflict;
use crate::error::{Result, TimewrapError};
use crate::ics;
use crate::model::{
    CalendarKind, Category, Change, ChangeKind, Conflict, ConflictPair, DayAgenda, EventDraft,
    EventOrigin, ImportReport, NowView, Occurrence, PropertyKey, PropertyValue, Reminder,
    Resolution, Rule, RuleField, RuleMatch, RuleSuggestion, SaveOutcome, Settings, SyncReport,
    Task, Timetable,
};
use crate::properties::{self, Property};
use crate::rules;

/// Combien de passé on garde développé.
const HORIZON_PAST: Duration = Duration::days(90);
/// Combien d'avenir on développe d'avance.
const HORIZON_FUTURE: Duration = Duration::days(400);
/// En deçà de cette marge restante, on re-développe la fenêtre.
const HORIZON_MARGIN: Duration = Duration::days(60);
/// Fenêtre examinée autour d'un événement pour y chercher des chevauchements.
const CONFLICT_WINDOW: Duration = Duration::days(1);
/// Profondeur sur laquelle une synchronisation compare l'avant et l'après.
const DIFF_HORIZON: Duration = Duration::days(45);
/// Un décalage en cascade doit finir par retomber sur un créneau libre.
const MAX_SHIFT_STEPS: u8 = 8;
/// Au-delà, une liste de changements n'est plus lisible dans une notification.
const MAX_CHANGES: usize = 40;

/// Couleurs attribuées aux catégories dans l'ordre de création.
pub const PALETTE: [u32; 8] = [
    0xFF4C5FD5, // bleu-violet
    0xFF2E9E7A, // vert
    0xFFD2694B, // terre cuite
    0xFF8155C6, // violet
    0xFF3C87C8, // bleu
    0xFFC2528A, // framboise
    0xFF7A8A3C, // olive
    0xFFB08236, // ambre
];

/// Couleur de l'emploi du temps quand rien n'a encore été personnalisé.
const DEFAULT_COLOR: u32 = PALETTE[0];

/// Colonnes d'une séance telle que l'interface la reçoit.
///
/// La couleur et le titre sont résolus ici, en SQL : la catégorie posée par une
/// règle l'emporte sur la couleur par défaut, et le renommage sur l'intitulé
/// d'origine — que l'on continue de renvoyer à part, pour les écrans de réglage.
const OCC_COLUMNS: &str = "o.id, COALESCE(cat.color, c.color), o.uid,
     CASE WHEN o.display_title <> '' THEN o.display_title ELSE o.summary END,
     o.summary, o.location, o.description, o.start_utc, o.end_utc, o.all_day,
     o.cancelled, o.origin, o.category_id, COALESCE(cat.name, ''),
     COALESCE(cat.label, ''),
     CASE WHEN o.hidden = 1 OR o.muted = 1 THEN 1 ELSE 0 END";

const OCC_FROM: &str = "FROM occurrences o
     JOIN calendars c ON c.id = o.calendar_id
     LEFT JOIN categories cat ON cat.id = o.category_id";

/// Les séances visibles : ni masquées par une règle, ni écartées à la main.
const VISIBLE: &str = "o.hidden = 0 AND o.muted = 0";

pub struct Store {
    conn: Connection,
    tz: Tz,
}

impl Store {
    pub fn open(path: &str, display_tz: &str) -> Result<Self> {
        let conn = if path == ":memory:" {
            Connection::open_in_memory()?
        } else {
            Connection::open(path)?
        };
        conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA foreign_keys = ON;
             PRAGMA synchronous = NORMAL;",
        )?;

        let tz = parse_tz(display_tz)?;
        let store = Store { conn, tz };
        store.migrate()?;
        Ok(store)
    }

    fn migrate(&self) -> Result<()> {
        let version: i64 = self
            .conn
            .query_row("PRAGMA user_version", [], |row| row.get(0))?;

        if version < 1 {
            self.conn.execute_batch(
                "CREATE TABLE calendars (
                     id          TEXT PRIMARY KEY,
                     name        TEXT NOT NULL,
                     kind        TEXT NOT NULL,
                     source      TEXT NOT NULL DEFAULT '',
                     color       INTEGER NOT NULL,
                     visible     INTEGER NOT NULL DEFAULT 1,
                     etag        TEXT,
                     last_sync   INTEGER,
                     created_at  INTEGER NOT NULL,
                     raw_ics     TEXT NOT NULL DEFAULT ''
                 );

                 CREATE TABLE occurrences (
                     id          TEXT PRIMARY KEY,
                     calendar_id TEXT NOT NULL REFERENCES calendars(id) ON DELETE CASCADE,
                     uid         TEXT NOT NULL,
                     summary     TEXT NOT NULL,
                     location    TEXT NOT NULL,
                     description TEXT NOT NULL,
                     start_utc   INTEGER NOT NULL,
                     end_utc     INTEGER NOT NULL,
                     all_day     INTEGER NOT NULL DEFAULT 0,
                     cancelled   INTEGER NOT NULL DEFAULT 0
                 );

                 CREATE INDEX idx_occ_start ON occurrences(start_utc);
                 CREATE INDEX idx_occ_cal ON occurrences(calendar_id, start_utc);

                 CREATE TABLE settings (
                     key   TEXT PRIMARY KEY,
                     value TEXT NOT NULL
                 );

                 PRAGMA user_version = 1;",
            )?;
        }

        // v2 : événements saisis sur place, catégories et règles.
        //
        // Les décisions des règles sont stockées à côté des données brutes plutôt
        // qu'à leur place : `summary` reste ce que disait l'ENT, `display_title`
        // ce que l'on montre. Une règle retirée rend donc son intitulé d'origine
        // à la séance, sans réimport.
        if version < 2 {
            self.conn.execute_batch(
                "ALTER TABLE calendars ADD COLUMN position INTEGER NOT NULL DEFAULT 0;

                 ALTER TABLE occurrences ADD COLUMN origin TEXT NOT NULL DEFAULT 'ics';
                 ALTER TABLE occurrences ADD COLUMN display_title TEXT NOT NULL DEFAULT '';
                 ALTER TABLE occurrences ADD COLUMN category_id TEXT;
                 ALTER TABLE occurrences ADD COLUMN category_locked INTEGER NOT NULL DEFAULT 0;
                 ALTER TABLE occurrences ADD COLUMN hidden INTEGER NOT NULL DEFAULT 0;
                 ALTER TABLE occurrences ADD COLUMN muted INTEGER NOT NULL DEFAULT 0;

                 CREATE TABLE categories (
                     id       TEXT PRIMARY KEY,
                     name     TEXT NOT NULL,
                     label    TEXT NOT NULL DEFAULT '',
                     color    INTEGER NOT NULL,
                     position INTEGER NOT NULL DEFAULT 0
                 );

                 CREATE TABLE rules (
                     id             TEXT PRIMARY KEY,
                     name           TEXT NOT NULL,
                     calendar_id    TEXT,
                     field          TEXT NOT NULL DEFAULT 'title',
                     match_kind     TEXT NOT NULL DEFAULT 'contains',
                     pattern        TEXT NOT NULL,
                     case_sensitive INTEGER NOT NULL DEFAULT 0,
                     category_id    TEXT REFERENCES categories(id) ON DELETE SET NULL,
                     rename_to      TEXT,
                     hide           INTEGER NOT NULL DEFAULT 0,
                     priority       INTEGER NOT NULL DEFAULT 0,
                     enabled        INTEGER NOT NULL DEFAULT 1
                 );

                 CREATE INDEX idx_occ_category ON occurrences(category_id);
                 CREATE INDEX idx_rules_priority ON rules(priority);

                 PRAGMA user_version = 2;",
            )?;
        }

        // v3 : un seul emploi du temps, les champs structurés de la description,
        // et la liste de choses à faire.
        //
        // Gérer plusieurs agendas revenait à demander de ranger avant de
        // consulter. Ce qui n'a pas d'heure — un devoir, une démarche — n'était
        // de toute façon pas un créneau : c'est une tâche, et elle se reporte au
        // lendemain tant qu'elle n'est pas cochée.
        if version < 3 {
            self.conn.execute_batch(
                "ALTER TABLE rules ADD COLUMN property TEXT;

                 CREATE TABLE occurrence_props (
                     occurrence_id TEXT NOT NULL
                         REFERENCES occurrences(id) ON DELETE CASCADE,
                     key           TEXT NOT NULL,
                     label         TEXT NOT NULL,
                     value         TEXT NOT NULL,
                     PRIMARY KEY (occurrence_id, key)
                 );
                 CREATE INDEX idx_props_key ON occurrence_props(key, value);

                 CREATE TABLE tasks (
                     id          TEXT PRIMARY KEY,
                     title       TEXT NOT NULL,
                     notes       TEXT NOT NULL DEFAULT '',
                     planned_day INTEGER NOT NULL,
                     done        INTEGER NOT NULL DEFAULT 0,
                     done_day    INTEGER,
                     created_at  INTEGER NOT NULL,
                     position    INTEGER NOT NULL DEFAULT 0
                 );
                 CREATE INDEX idx_tasks_day ON tasks(planned_day, done);",
            )?;

            self.consolidate_calendars()?;
            self.conn.execute_batch("PRAGMA user_version = 3;")?;
        }

        Ok(())
    }

    /// Ramène d'anciennes bases à un emploi du temps unique.
    ///
    /// Les séances saisies à la main dans un agenda annexe sont rattachées à
    /// celui qu'on garde plutôt que supprimées : elles n'ont pas démérité parce
    /// que l'application a changé d'avis sur son organisation.
    fn consolidate_calendars(&self) -> Result<()> {
        let ids: Vec<String> = {
            let mut stmt = self.conn.prepare(
                "SELECT id FROM calendars
                 ORDER BY CASE WHEN raw_ics <> '' THEN 0 ELSE 1 END, created_at",
            )?;
            let rows = stmt.query_map([], |row| row.get(0))?;
            rows.collect::<std::result::Result<Vec<_>, _>>()?
        };

        let Some((keep, extras)) = ids.split_first() else {
            return Ok(());
        };
        for extra in extras {
            self.conn.execute(
                "UPDATE occurrences SET calendar_id = ?1
                 WHERE calendar_id = ?2 AND origin = 'local'",
                params![keep, extra],
            )?;
            self.conn
                .execute("DELETE FROM calendars WHERE id = ?1", params![extra])?;
        }
        self.conn.execute("UPDATE calendars SET visible = 1", [])?;
        Ok(())
    }

    /// Exécute un bloc d'écritures d'un seul tenant.
    ///
    /// Un import qui échoue à mi-course ne doit pas laisser un emploi du temps à
    /// moitié développé : soit tout est écrit, soit rien.
    fn transact<T>(&self, block: impl FnOnce() -> Result<T>) -> Result<T> {
        self.conn.execute_batch("BEGIN IMMEDIATE")?;
        match block() {
            Ok(value) => {
                self.conn.execute_batch("COMMIT")?;
                Ok(value)
            }
            Err(e) => {
                let _ = self.conn.execute_batch("ROLLBACK");
                Err(e)
            }
        }
    }

    pub fn display_timezone(&self) -> Tz {
        self.tz
    }

    pub fn set_display_timezone(&mut self, tz: &str) -> Result<()> {
        self.tz = parse_tz(tz)?;
        self.set_setting("display_tz", tz)
    }

    // ------------------------------------------------------- emploi du temps

    /// L'emploi du temps, s'il y en a un.
    pub fn timetable(&self) -> Result<Option<Timetable>> {
        let mut stmt = self.conn.prepare(
            "SELECT c.id, c.name, c.kind, c.source, c.color, c.last_sync,
                    (SELECT COUNT(DISTINCT o.uid) FROM occurrences o WHERE o.calendar_id = c.id),
                    (SELECT COUNT(*) FROM occurrences o WHERE o.calendar_id = c.id)
             FROM calendars c ORDER BY c.created_at LIMIT 1",
        )?;
        Ok(stmt
            .query_row([], |row| {
                Ok(Timetable {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    kind: CalendarKind::from_str(&row.get::<_, String>(2)?),
                    source: row.get(3)?,
                    color: row.get::<_, i64>(4)? as u32,
                    last_sync: row.get(5)?,
                    event_count: row.get::<_, i64>(6)? as u32,
                    occurrence_count: row.get::<_, i64>(7)? as u32,
                })
            })
            .optional()?)
    }

    fn timetable_id(&self) -> Result<String> {
        self.timetable()?
            .map(|t| t.id)
            .ok_or_else(|| TimewrapError::NotFound("aucun emploi du temps importé".into()))
    }

    /// Installe ou remplace le contenu de l'emploi du temps.
    ///
    /// L'identité de l'emploi du temps ne change jamais : réimporter un fichier
    /// ou resynchroniser une URL réécrit les séances venues du flux, et laisse
    /// intactes celles ajoutées ici, les masquages et les personnalisations.
    pub fn import_ics(
        &self,
        name: &str,
        kind: CalendarKind,
        source: &str,
        ics_text: &str,
        now_utc: i64,
    ) -> Result<SyncReport> {
        let id = match self.timetable()? {
            Some(existing) => {
                self.conn.execute(
                    "UPDATE calendars SET name = ?2, kind = ?3, source = ?4 WHERE id = ?1",
                    params![existing.id, name.trim(), kind.as_str(), source],
                )?;
                existing.id
            }
            None => {
                let id = new_id("cal", now_utc, name);
                self.conn.execute(
                    "INSERT INTO calendars
                     (id, name, kind, source, color, visible, created_at, raw_ics)
                     VALUES (?1, ?2, ?3, ?4, ?5, 1, ?6, '')",
                    params![
                        id,
                        name.trim(),
                        kind.as_str(),
                        source,
                        DEFAULT_COLOR as i64,
                        now_utc
                    ],
                )?;
                id
            }
        };

        self.conn.execute(
            "UPDATE calendars SET raw_ics = ?2 WHERE id = ?1",
            params![id, ics_text],
        )?;
        if kind == CalendarKind::IcsUrl {
            self.set_setting("source_url", source)?;
        }

        let before = self.diff_snapshot(now_utc)?;
        let import = self.rebuild(&id, ics_text, now_utc)?;
        let after = self.diff_snapshot(now_utc)?;

        Ok(SyncReport {
            import,
            changes: self.describe_changes(&before, &after),
        })
    }

    pub fn set_color(&self, color: u32) -> Result<()> {
        self.conn
            .execute("UPDATE calendars SET color = ?1", params![color as i64])?;
        Ok(())
    }

    pub fn rename(&self, name: &str) -> Result<()> {
        self.conn
            .execute("UPDATE calendars SET name = ?1", params![name.trim()])?;
        Ok(())
    }

    /// Efface l'emploi du temps et tout ce qui en dépend. Les tâches restent :
    /// elles ne viennent pas de l'ENT.
    pub fn clear_timetable(&self) -> Result<()> {
        self.conn.execute("DELETE FROM calendars", [])?;
        Ok(())
    }

    /// Développe le flux et réécrit les séances importées.
    ///
    /// Le remplacement est intégral, donc idempotent : réimporter deux fois le
    /// même fichier laisse exactement le même état. Deux choses y survivent
    /// pourtant, parce qu'elles n'appartiennent pas au flux : les événements
    /// saisis à la main, et les séances masquées à la suite d'un conflit — les
    /// identifiants d'occurrence étant stables, on les repose.
    fn rebuild(&self, calendar_id: &str, ics_text: &str, now_utc: i64) -> Result<ImportReport> {
        let (from, to) = horizon(now_utc);
        let expansion = ics::expand(ics_text, from, to, self.tz)?;
        let rules = self.rules_raw()?;

        self.transact(|| {
            let muted = self.muted_ids()?;

            self.conn.execute(
                "DELETE FROM occurrences WHERE calendar_id = ?1 AND origin = 'ics'",
                params![calendar_id],
            )?;

            {
                let mut stmt = self.conn.prepare(
                    "INSERT OR REPLACE INTO occurrences
                     (id, calendar_id, uid, summary, location, description,
                      start_utc, end_utc, all_day, cancelled, origin,
                      display_title, category_id, category_locked, hidden, muted)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, 0, ?14, 0)",
                )?;
                for occurrence in &expansion.occurrences {
                    let props = properties::parse(&occurrence.description);
                    let outcome = rules::apply(
                        &rules,
                        &rules::Fields {
                            title: &occurrence.summary,
                            location: &occurrence.location,
                            description: &occurrence.description,
                            properties: &props,
                        },
                    );
                    let id = occurrence_id(calendar_id, &occurrence.uid, occurrence.start_utc);
                    stmt.execute(params![
                        id,
                        calendar_id,
                        occurrence.uid,
                        occurrence.summary,
                        occurrence.location,
                        occurrence.description,
                        occurrence.start_utc,
                        occurrence.end_utc,
                        occurrence.all_day as i64,
                        occurrence.cancelled as i64,
                        EventOrigin::Ics.as_str(),
                        outcome.display_title.unwrap_or_default(),
                        outcome.category_id,
                        outcome.hidden as i64,
                    ])?;
                    self.write_properties(&id, &props)?;
                }
            }

            for id in muted {
                self.conn.execute(
                    "UPDATE occurrences SET muted = 1 WHERE id = ?1",
                    params![id],
                )?;
            }

            self.conn.execute(
                "UPDATE calendars SET last_sync = ?2 WHERE id = ?1",
                params![calendar_id, now_utc],
            )?;
            self.set_setting("horizon_from", &from.to_string())?;
            self.set_setting("horizon_to", &to.to_string())?;
            self.set_setting("last_sync_utc", &now_utc.to_string())?;
            Ok(())
        })?;

        Ok(ImportReport {
            events: expansion.events,
            occurrences: expansion.occurrences.len() as u32,
            skipped: expansion.skipped,
            first_start_utc: expansion.occurrences.first().map(|o| o.start_utc),
            last_start_utc: expansion.occurrences.last().map(|o| o.start_utc),
        })
    }

    fn write_properties(&self, occurrence_id: &str, props: &[Property]) -> Result<()> {
        self.conn.execute(
            "DELETE FROM occurrence_props WHERE occurrence_id = ?1",
            params![occurrence_id],
        )?;
        let mut stmt = self.conn.prepare(
            "INSERT OR REPLACE INTO occurrence_props (occurrence_id, key, label, value)
             VALUES (?1, ?2, ?3, ?4)",
        )?;
        for property in props {
            stmt.execute(params![
                occurrence_id,
                property.key,
                property.label,
                property.value
            ])?;
        }
        Ok(())
    }

    fn muted_ids(&self) -> Result<Vec<String>> {
        let mut stmt = self
            .conn
            .prepare("SELECT id FROM occurrences WHERE muted = 1")?;
        let rows = stmt.query_map([], |row| row.get(0))?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    /// Re-développe l'emploi du temps si l'horizon devient trop proche.
    pub fn ensure_horizon(&self, now_utc: i64) -> Result<bool> {
        let horizon_to: Option<i64> = self.get_setting("horizon_to")?.and_then(|v| v.parse().ok());
        let needs_refresh = match horizon_to {
            Some(to) => now_utc + HORIZON_MARGIN.num_seconds() > to,
            None => false,
        };
        if !needs_refresh {
            return Ok(false);
        }

        let source: Option<(String, String)> = self
            .conn
            .query_row(
                "SELECT id, raw_ics FROM calendars WHERE raw_ics <> '' LIMIT 1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;

        if let Some((id, raw)) = source {
            self.rebuild(&id, &raw, now_utc)?;
            return Ok(true);
        }
        Ok(false)
    }

    // ------------------------------------------------ différence entre imports

    /// Ce que l'on retient d'une séance pour savoir si elle a bougé.
    fn diff_snapshot(&self, now_utc: i64) -> Result<Vec<Snapshot>> {
        let to = now_utc + DIFF_HORIZON.num_seconds();
        let mut stmt = self.conn.prepare(
            "SELECT uid, summary, location, start_utc, cancelled
             FROM occurrences
             WHERE origin = 'ics' AND start_utc >= ?1 AND start_utc < ?2
             ORDER BY start_utc",
        )?;
        let rows = stmt.query_map(params![now_utc, to], |row| {
            Ok(Snapshot {
                uid: row.get(0)?,
                title: row.get(1)?,
                location: row.get(2)?,
                start_utc: row.get(3)?,
                cancelled: row.get::<_, i64>(4)? != 0,
            })
        })?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    /// Rédige, en français, ce qui a changé entre deux états.
    ///
    /// L'appariement se fait par UID : une séance qui garde le sien a été
    /// déplacée, pas supprimée puis recréée. C'est la différence entre « ton TD
    /// de mardi passe à 15h » et deux notifications illisibles.
    fn describe_changes(&self, before: &[Snapshot], after: &[Snapshot]) -> Vec<Change> {
        let mut changes = Vec::new();

        for old in before {
            match after.iter().find(|n| n.same_slot(old)) {
                Some(new) => {
                    if new.cancelled && !old.cancelled {
                        let summary = format!(
                            "« {} » du {} est annulé",
                            new.title,
                            self.day_label(new.start_utc)
                        );
                        changes.push(change(ChangeKind::Cancelled, new, summary));
                    } else if new.location != old.location && !new.location.is_empty() {
                        let summary = format!(
                            "« {} » du {} change de salle : {} → {}",
                            new.title,
                            self.day_label(new.start_utc),
                            if old.location.is_empty() {
                                "sans salle"
                            } else {
                                &old.location
                            },
                            new.location
                        );
                        changes.push(change(ChangeKind::Room, new, summary));
                    }
                }
                None => match after
                    .iter()
                    .find(|n| n.uid == old.uid && !n.matched(before))
                {
                    Some(moved) => {
                        let summary = format!(
                            "« {} » passe du {} au {}",
                            moved.title,
                            self.slot_label(old.start_utc),
                            self.slot_label(moved.start_utc)
                        );
                        changes.push(change(ChangeKind::Moved, moved, summary));
                    }
                    None => {
                        let summary = format!(
                            "« {} » du {} est retiré de l'emploi du temps",
                            old.title,
                            self.slot_label(old.start_utc)
                        );
                        changes.push(change(ChangeKind::Removed, old, summary));
                    }
                },
            }
        }

        for new in after {
            let known = before.iter().any(|o| o.same_slot(new) || o.uid == new.uid);
            if !known {
                let summary = format!(
                    "« {} » ajouté le {}",
                    new.title,
                    self.slot_label(new.start_utc)
                );
                changes.push(change(ChangeKind::Added, new, summary));
            }
        }

        changes.sort_by_key(|c| c.start_utc);
        changes.truncate(MAX_CHANGES);
        changes
    }

    /// « mardi 22 septembre »
    fn day_label(&self, utc: i64) -> String {
        let Some(dt) = DateTime::from_timestamp(utc, 0) else {
            return String::new();
        };
        let local = dt.with_timezone(&self.tz);
        format!(
            "{} {} {}",
            weekday_name(local.weekday().num_days_from_monday()),
            local.day(),
            month_name(local.month())
        )
    }

    /// « mardi 22 septembre à 13:30 »
    fn slot_label(&self, utc: i64) -> String {
        let Some(dt) = DateTime::from_timestamp(utc, 0) else {
            return String::new();
        };
        let local = dt.with_timezone(&self.tz);
        format!(
            "{} à {:02}:{:02}",
            self.day_label(utc),
            local.hour(),
            local.minute()
        )
    }

    // ------------------------------------------------------------- catégories

    pub fn categories(&self) -> Result<Vec<Category>> {
        let mut stmt = self.conn.prepare(
            "SELECT k.id, k.name, k.label, k.color, k.position,
                    (SELECT COUNT(*) FROM occurrences o WHERE o.category_id = k.id)
             FROM categories k
             ORDER BY k.position, k.name",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(Category {
                id: row.get(0)?,
                name: row.get(1)?,
                label: row.get(2)?,
                color: row.get::<_, i64>(3)? as u32,
                position: row.get::<_, i64>(4)? as i32,
                occurrence_count: row.get::<_, i64>(5)? as u32,
            })
        })?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    pub fn create_category(
        &self,
        name: &str,
        label: &str,
        color: Option<u32>,
        now_utc: i64,
    ) -> Result<Category> {
        let name = name.trim();
        if name.is_empty() {
            return Err(TimewrapError::InvalidEvent(
                "une catégorie a besoin d'un nom".into(),
            ));
        }
        let count: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM categories", [], |row| row.get(0))?;
        let color = color.unwrap_or_else(|| self.free_color(count as usize));
        let id = new_id("cat", now_utc, name);

        self.conn.execute(
            "INSERT INTO categories (id, name, label, color, position) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![id, name, label.trim(), color as i64, count],
        )?;
        self.category(&id)
    }

    pub fn update_category(
        &self,
        id: &str,
        name: &str,
        label: &str,
        color: u32,
    ) -> Result<Category> {
        let changed = self.conn.execute(
            "UPDATE categories SET name = ?2, label = ?3, color = ?4 WHERE id = ?1",
            params![id, name.trim(), label.trim(), color as i64],
        )?;
        if changed == 0 {
            return Err(TimewrapError::NotFound(format!("catégorie {id}")));
        }
        self.category(id)
    }

    pub fn delete_category(&self, id: &str) -> Result<()> {
        self.transact(|| {
            self.conn
                .execute("DELETE FROM rules WHERE category_id = ?1", params![id])?;
            self.conn
                .execute("DELETE FROM categories WHERE id = ?1", params![id])?;
            self.conn.execute(
                "UPDATE occurrences SET category_id = NULL WHERE category_id = ?1",
                params![id],
            )?;
            Ok(())
        })?;
        self.reapply_rules()?;
        Ok(())
    }

    fn category(&self, id: &str) -> Result<Category> {
        self.categories()?
            .into_iter()
            .find(|c| c.id == id)
            .ok_or_else(|| TimewrapError::NotFound(format!("catégorie {id}")))
    }

    /// Première couleur de la palette qu'aucune catégorie n'utilise déjà.
    fn free_color(&self, fallback_index: usize) -> u32 {
        let used: Vec<u32> = self
            .categories()
            .map(|list| list.into_iter().map(|c| c.color).collect())
            .unwrap_or_default();
        PALETTE
            .iter()
            .copied()
            .find(|color| !used.contains(color))
            .unwrap_or(PALETTE[fallback_index % PALETTE.len()])
    }

    // ------------------------------------------------------ champs structurés

    /// Les champs repérés dans les descriptions, du plus renseigné au moins.
    pub fn property_keys(&self) -> Result<Vec<PropertyKey>> {
        let mut stmt = self.conn.prepare(
            "SELECT p.key, MIN(p.label), COUNT(DISTINCT p.value), COUNT(*)
             FROM occurrence_props p
             GROUP BY p.key
             ORDER BY COUNT(*) DESC, p.key",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)? as u32,
                row.get::<_, i64>(3)? as u32,
            ))
        })?;

        let keys = rows.collect::<std::result::Result<Vec<_>, _>>()?;
        let rules = self.rules_raw()?;
        Ok(keys
            .into_iter()
            .map(|(key, label, distinct_values, occurrences)| {
                let colored_values = rules
                    .iter()
                    .filter(|r| {
                        r.property.as_deref() == Some(key.as_str()) && r.category_id.is_some()
                    })
                    .count() as u32;
                PropertyKey {
                    key,
                    label,
                    distinct_values,
                    occurrences,
                    colored_values,
                }
            })
            .collect())
    }

    /// Les valeurs d'un champ, avec la couleur que chacune porte aujourd'hui.
    ///
    /// C'est la matière de l'écran des couleurs : une ligne par matière, une
    /// ligne par type de cours, chacune avec sa pastille à changer.
    pub fn property_values(&self, key: &str) -> Result<Vec<PropertyValue>> {
        let key = properties::normalize(key);
        let default_color = self.timetable()?.map(|t| t.color).unwrap_or(DEFAULT_COLOR);

        let mut stmt = self.conn.prepare(
            "SELECT value, COUNT(*) FROM occurrence_props
             WHERE key = ?1 GROUP BY value ORDER BY COUNT(*) DESC, value",
        )?;
        let rows = stmt.query_map(params![key], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)? as u32))
        })?;
        let values = rows.collect::<std::result::Result<Vec<_>, _>>()?;

        let rules = self.rules_raw()?;
        let categories = self.categories()?;

        Ok(values
            .into_iter()
            .map(|(value, occurrences)| {
                let category = rules
                    .iter()
                    .find(|r| {
                        r.property.as_deref() == Some(key.as_str())
                            && r.pattern.eq_ignore_ascii_case(&value)
                            && r.category_id.is_some()
                    })
                    .and_then(|r| r.category_id.clone())
                    .and_then(|id| categories.iter().find(|c| c.id == id).cloned());

                PropertyValue {
                    key: key.clone(),
                    value,
                    occurrences,
                    color: category.as_ref().map(|c| c.color).unwrap_or(default_color),
                    colored: category.is_some(),
                    category_id: category.map(|c| c.id),
                }
            })
            .collect())
    }

    /// Donne une couleur à une valeur de champ — « Matière : Analyse » en vert.
    ///
    /// Sous le capot, une catégorie et la règle qui la pose. L'utilisateur, lui,
    /// n'a vu qu'une pastille : c'est tout l'intérêt de ne pas lui demander
    /// d'écrire une règle pour chaque matière.
    pub fn set_property_color(
        &self,
        key: &str,
        value: &str,
        color: u32,
        now_utc: i64,
    ) -> Result<Category> {
        let key = properties::normalize(key);
        let existing = self.rules_raw()?.into_iter().find(|r| {
            r.property.as_deref() == Some(key.as_str()) && r.pattern.eq_ignore_ascii_case(value)
        });

        if let Some(rule) = existing
            && let Some(category_id) = rule.category_id
        {
            let category = self.category(&category_id)?;
            return self.update_category(&category_id, &category.name, &category.label, color);
        }

        let label = self.property_label(&key)?;
        let category = self.create_category(value, &short_label(value), Some(color), now_utc)?;
        self.save_rule(
            &Rule {
                id: String::new(),
                name: format!("{label} : {value}"),
                field: RuleField::Property,
                property: Some(key),
                match_kind: RuleMatch::Equals,
                pattern: value.to_string(),
                case_sensitive: false,
                category_id: Some(category.id.clone()),
                rename_to: None,
                hide: false,
                priority: 0,
                enabled: true,
                match_count: 0,
            },
            now_utc,
        )?;
        self.category(&category.id)
    }

    /// Retire la couleur d'une valeur : la règle et sa catégorie disparaissent.
    pub fn clear_property_color(&self, key: &str, value: &str) -> Result<()> {
        let key = properties::normalize(key);
        let rules: Vec<Rule> = self
            .rules_raw()?
            .into_iter()
            .filter(|r| {
                r.property.as_deref() == Some(key.as_str()) && r.pattern.eq_ignore_ascii_case(value)
            })
            .collect();

        for rule in rules {
            self.conn
                .execute("DELETE FROM rules WHERE id = ?1", params![rule.id])?;
            if let Some(category_id) = &rule.category_id {
                self.delete_category(category_id)?;
            }
        }
        self.reapply_rules()?;
        Ok(())
    }

    /// Attribue d'un coup une couleur à chaque valeur d'un champ.
    ///
    /// « Colorier par matière » en un geste : c'est ce que l'on veut faire neuf
    /// fois sur dix, et le faire valeur par valeur serait absurde.
    pub fn auto_color_property(&self, key: &str, now_utc: i64) -> Result<u32> {
        let key = properties::normalize(key);
        let values = self.property_values(&key)?;
        let mut colored = 0;

        for (index, value) in values.iter().enumerate() {
            if value.colored {
                continue;
            }
            let color = PALETTE[index % PALETTE.len()];
            self.set_property_color(&key, &value.value, color, now_utc + index as i64)?;
            colored += 1;
        }
        Ok(colored)
    }

    fn property_label(&self, key: &str) -> Result<String> {
        Ok(self
            .conn
            .query_row(
                "SELECT label FROM occurrence_props WHERE key = ?1 LIMIT 1",
                params![key],
                |row| row.get::<_, String>(0),
            )
            .optional()?
            .unwrap_or_else(|| key.to_string()))
    }

    /// Les champs structurés d'une séance, tels que sa fiche les affiche.
    pub fn occurrence_properties(&self, id: &str) -> Result<Vec<PropertyValue>> {
        let default_color = self.timetable()?.map(|t| t.color).unwrap_or(DEFAULT_COLOR);
        let mut stmt = self
            .conn
            .prepare("SELECT key, value FROM occurrence_props WHERE occurrence_id = ?1")?;
        let rows = stmt.query_map(params![id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;
        let own = rows.collect::<std::result::Result<Vec<_>, _>>()?;

        let mut out = Vec::new();
        for (key, value) in own {
            let known = self
                .property_values(&key)?
                .into_iter()
                .find(|v| v.value == value);
            out.push(known.unwrap_or(PropertyValue {
                key,
                value,
                occurrences: 1,
                category_id: None,
                color: default_color,
                colored: false,
            }));
        }
        Ok(out)
    }

    // ----------------------------------------------------------------- règles

    pub fn rules(&self) -> Result<Vec<Rule>> {
        let mut rules = self.rules_raw()?;
        let samples = self.rule_inputs()?;
        for rule in &mut rules {
            rule.match_count = samples
                .iter()
                .filter(|sample| rules::matches(rule, &sample.fields()))
                .count() as u32;
        }
        Ok(rules)
    }

    /// Les règles sans leur compte de correspondances : c'est cette version
    /// qu'utilisent les écritures, qui n'ont que faire du chiffre.
    fn rules_raw(&self) -> Result<Vec<Rule>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, field, property, match_kind, pattern, case_sensitive,
                    category_id, rename_to, hide, priority, enabled
             FROM rules ORDER BY priority, rowid",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(Rule {
                id: row.get(0)?,
                name: row.get(1)?,
                field: RuleField::from_str(&row.get::<_, String>(2)?),
                property: row.get(3)?,
                match_kind: RuleMatch::from_str(&row.get::<_, String>(4)?),
                pattern: row.get(5)?,
                case_sensitive: row.get::<_, i64>(6)? != 0,
                category_id: row.get(7)?,
                rename_to: row.get(8)?,
                hide: row.get::<_, i64>(9)? != 0,
                priority: row.get::<_, i64>(10)? as i32,
                enabled: row.get::<_, i64>(11)? != 0,
                match_count: 0,
            })
        })?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    /// Enregistre une règle. Un identifiant vide en crée une nouvelle.
    pub fn save_rule(&self, rule: &Rule, now_utc: i64) -> Result<Rule> {
        if rule.pattern.trim().is_empty() {
            return Err(TimewrapError::InvalidEvent(
                "une règle a besoin d'un motif à reconnaître".into(),
            ));
        }
        if rule.field == RuleField::Property
            && rule.property.as_ref().is_none_or(|p| p.trim().is_empty())
        {
            return Err(TimewrapError::InvalidEvent(
                "une règle sur un champ doit dire lequel".into(),
            ));
        }
        let property = rule.property.as_ref().map(|p| properties::normalize(p));

        let id = if rule.id.trim().is_empty() {
            let id = new_id("rule", now_utc, &rule.pattern);
            let next: i64 = self.conn.query_row(
                "SELECT COALESCE(MAX(priority), -1) + 1 FROM rules",
                [],
                |row| row.get(0),
            )?;
            self.conn.execute(
                "INSERT INTO rules (id, name, field, property, match_kind, pattern,
                                    case_sensitive, category_id, rename_to, hide, priority, enabled)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
                params![
                    id,
                    rule_name(rule),
                    rule.field.as_str(),
                    property,
                    rule.match_kind.as_str(),
                    rule.pattern.trim(),
                    rule.case_sensitive as i64,
                    rule.category_id,
                    rule.rename_to,
                    rule.hide as i64,
                    next,
                    rule.enabled as i64,
                ],
            )?;
            id
        } else {
            let changed = self.conn.execute(
                "UPDATE rules SET name = ?2, field = ?3, property = ?4, match_kind = ?5,
                                  pattern = ?6, case_sensitive = ?7, category_id = ?8,
                                  rename_to = ?9, hide = ?10, priority = ?11, enabled = ?12
                 WHERE id = ?1",
                params![
                    rule.id,
                    rule_name(rule),
                    rule.field.as_str(),
                    property,
                    rule.match_kind.as_str(),
                    rule.pattern.trim(),
                    rule.case_sensitive as i64,
                    rule.category_id,
                    rule.rename_to,
                    rule.hide as i64,
                    rule.priority as i64,
                    rule.enabled as i64,
                ],
            )?;
            if changed == 0 {
                return Err(TimewrapError::NotFound(format!("règle {}", rule.id)));
            }
            rule.id.clone()
        };

        self.reapply_rules()?;
        self.rules()?
            .into_iter()
            .find(|r| r.id == id)
            .ok_or_else(|| TimewrapError::NotFound(format!("règle {id}")))
    }

    pub fn delete_rule(&self, id: &str) -> Result<()> {
        self.conn
            .execute("DELETE FROM rules WHERE id = ?1", params![id])?;
        self.reapply_rules()?;
        Ok(())
    }

    /// Rejoue toutes les règles sur toutes les séances.
    ///
    /// Renvoie le nombre de séances dont l'apparence a changé — c'est ce que
    /// l'écran des couleurs affiche, pour que l'effet soit visible même hors de
    /// la fenêtre consultée.
    pub fn reapply_rules(&self) -> Result<u32> {
        let rules = self.rules_raw()?;
        let rows = self.rule_inputs()?;

        let mut touched = 0u32;
        self.transact(|| {
            let mut stmt = self.conn.prepare(
                "UPDATE occurrences SET display_title = ?2, category_id = ?3, hidden = ?4
                 WHERE id = ?1",
            )?;
            for row in &rows {
                let outcome = rules::apply(&rules, &row.fields());
                let display_title = outcome.display_title.unwrap_or_default();
                // Une catégorie choisie à la main sur une séance l'emporte sur
                // les règles : l'exception doit survivre au moteur.
                let category_id = if row.locked {
                    row.category_id.clone()
                } else {
                    outcome.category_id
                };

                if display_title == row.display_title
                    && category_id == row.category_id
                    && outcome.hidden == row.hidden
                {
                    continue;
                }
                stmt.execute(params![
                    row.id,
                    display_title,
                    category_id,
                    outcome.hidden as i64
                ])?;
                touched += 1;
            }
            Ok(())
        })?;

        Ok(touched)
    }

    /// Toutes les séances, avec ce dont les règles ont besoin pour se prononcer.
    fn rule_inputs(&self) -> Result<Vec<RuleInput>> {
        let mut stmt = self.conn.prepare(
            "SELECT o.id, o.summary, o.location, o.description, o.display_title,
                    o.category_id, o.category_locked, o.hidden
             FROM occurrences o",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(RuleInput {
                id: row.get(0)?,
                summary: row.get(1)?,
                location: row.get(2)?,
                description: row.get(3)?,
                display_title: row.get(4)?,
                category_id: row.get(5)?,
                locked: row.get::<_, i64>(6)? != 0,
                hidden: row.get::<_, i64>(7)? != 0,
                properties: Vec::new(),
            })
        })?;
        let mut inputs = rows.collect::<std::result::Result<Vec<_>, _>>()?;

        // Les champs sont relus depuis la description plutôt que joints en SQL :
        // une requête de moins, et le résultat est le même puisque c'est d'elle
        // qu'ils viennent.
        for input in &mut inputs {
            input.properties = properties::parse(&input.description);
        }
        Ok(inputs)
    }

    /// Ce que le cœur propose de classer, au vu de ce qui a été importé.
    pub fn rule_suggestions(&self) -> Result<Vec<RuleSuggestion>> {
        let samples: Vec<rules::Sample> = self
            .rule_inputs()?
            .into_iter()
            .map(|input| rules::Sample {
                title: input.summary,
                properties: input.properties,
            })
            .collect();

        let existing = self.rules_raw()?;
        let used: Vec<u32> = self.categories()?.into_iter().map(|c| c.color).collect();
        let palette: Vec<u32> = PALETTE
            .iter()
            .copied()
            .filter(|color| !used.contains(color))
            .chain(PALETTE.iter().copied())
            .collect();

        Ok(rules::suggest(&samples, &existing, &palette))
    }

    /// Transforme une suggestion en catégorie plus règle, d'un geste.
    pub fn accept_suggestion(
        &self,
        suggestion: &RuleSuggestion,
        name: &str,
        label: &str,
        color: Option<u32>,
        now_utc: i64,
    ) -> Result<Rule> {
        let category = self.create_category(
            name,
            label,
            color.or(Some(suggestion.suggested_color)),
            now_utc,
        )?;
        self.save_rule(
            &Rule {
                id: String::new(),
                name: name.trim().to_string(),
                field: suggestion.field,
                property: suggestion.property.clone(),
                match_kind: suggestion.match_kind,
                pattern: suggestion.pattern.clone(),
                case_sensitive: false,
                category_id: Some(category.id),
                rename_to: None,
                hide: false,
                priority: 0,
                enabled: true,
                match_count: 0,
            },
            now_utc,
        )
    }

    // ------------------------------------------------------ événements saisis

    /// Écrit un événement, en traitant les chevauchements selon `resolution`.
    pub fn save_event(
        &self,
        draft: &EventDraft,
        resolution: Resolution,
        now_utc: i64,
    ) -> Result<SaveOutcome> {
        let calendar_id = self.timetable_id()?;
        if draft.end_utc <= draft.start_utc {
            return Err(TimewrapError::InvalidEvent(
                "un événement doit finir après avoir commencé".into(),
            ));
        }
        if let Some(id) = &draft.id
            && self.occurrence_origin(id)? != EventOrigin::Local
        {
            return Err(TimewrapError::InvalidEvent(
                "une séance importée ne se modifie pas ici : elle serait écrasée au prochain import".into(),
            ));
        }

        let duration = draft.end_utc - draft.start_utc;
        let mut start = draft.start_utc;
        let mut end = draft.end_utc;
        let mut conflicts = self.conflicts_for(draft.id.as_deref(), start, end)?;
        let mut shifted_minutes = 0i64;
        let mut removed = 0u32;
        let mut hidden = 0u32;

        if !conflicts.is_empty() {
            match resolution {
                Resolution::Cancel => {
                    return Ok(SaveOutcome {
                        saved: None,
                        conflicts,
                        blocked: true,
                        removed: 0,
                        hidden: 0,
                        shifted_minutes: 0,
                    });
                }
                Resolution::Ignore => {}
                Resolution::Replace => {
                    let (r, h) = self.clear_conflicts(&conflicts)?;
                    removed = r;
                    hidden = h;
                }
                Resolution::ShiftAfter => {
                    // Décaler peut heurter le créneau suivant : on recommence
                    // jusqu'à retomber sur du libre, sans boucler indéfiniment.
                    for _ in 0..MAX_SHIFT_STEPS {
                        let Some(target) = conflict::shift_after(&conflicts) else {
                            break;
                        };
                        start = target;
                        end = target + duration;
                        conflicts = self.conflicts_for(draft.id.as_deref(), start, end)?;
                        if conflicts.is_empty() {
                            break;
                        }
                    }
                    shifted_minutes = (start - draft.start_utc) / 60;
                    if !conflicts.is_empty() {
                        return Ok(SaveOutcome {
                            saved: None,
                            conflicts,
                            blocked: true,
                            removed: 0,
                            hidden: 0,
                            shifted_minutes,
                        });
                    }
                }
            }
        }

        let id = draft.id.clone().unwrap_or_else(|| {
            new_id(
                "evt",
                now_utc,
                &format!("{}{}", draft.title, draft.start_utc),
            )
        });
        let title = draft.title.trim();
        let title = if title.is_empty() {
            "Sans titre"
        } else {
            title
        };

        let props = properties::parse(&draft.description);
        let rules = self.rules_raw()?;
        let outcome = rules::apply(
            &rules,
            &rules::Fields {
                title,
                location: &draft.location,
                description: &draft.description,
                properties: &props,
            },
        );
        let locked = draft.category_id.is_some();
        let category_id = draft.category_id.clone().or(outcome.category_id);

        self.conn.execute(
            "INSERT INTO occurrences
             (id, calendar_id, uid, summary, location, description, start_utc, end_utc,
              all_day, cancelled, origin, display_title, category_id, category_locked,
              hidden, muted)
             VALUES (?1, ?2, ?1, ?3, ?4, ?5, ?6, ?7, ?8, 0, ?9, ?10, ?11, ?12, ?13, 0)
             ON CONFLICT(id) DO UPDATE SET
                 summary = excluded.summary,
                 location = excluded.location,
                 description = excluded.description,
                 start_utc = excluded.start_utc,
                 end_utc = excluded.end_utc,
                 all_day = excluded.all_day,
                 display_title = excluded.display_title,
                 category_id = excluded.category_id,
                 category_locked = excluded.category_locked,
                 hidden = excluded.hidden",
            params![
                id,
                calendar_id,
                title,
                draft.location.trim(),
                draft.description.trim(),
                start,
                end,
                draft.all_day as i64,
                EventOrigin::Local.as_str(),
                outcome.display_title.unwrap_or_default(),
                category_id,
                locked as i64,
                outcome.hidden as i64,
            ],
        )?;
        self.write_properties(&id, &props)?;

        Ok(SaveOutcome {
            saved: Some(self.occurrence(&id)?),
            conflicts,
            blocked: false,
            removed,
            hidden,
            shifted_minutes,
        })
    }

    /// Fait place nette : supprime ce qui a été saisi ici, masque ce qui est
    /// importé.
    fn clear_conflicts(&self, conflicts: &[Conflict]) -> Result<(u32, u32)> {
        let mut removed = 0;
        let mut hidden = 0;
        self.transact(|| {
            for conflict in conflicts {
                if conflict.other_deletable {
                    self.conn.execute(
                        "DELETE FROM occurrences WHERE id = ?1",
                        params![conflict.other.id],
                    )?;
                    removed += 1;
                } else {
                    self.conn.execute(
                        "UPDATE occurrences SET muted = 1 WHERE id = ?1",
                        params![conflict.other.id],
                    )?;
                    hidden += 1;
                }
            }
            Ok(())
        })?;
        Ok((removed, hidden))
    }

    pub fn delete_event(&self, id: &str) -> Result<()> {
        if self.occurrence_origin(id)? != EventOrigin::Local {
            return Err(TimewrapError::InvalidEvent(
                "une séance importée ne se supprime pas : masquez-la".into(),
            ));
        }
        self.conn
            .execute("DELETE FROM occurrences WHERE id = ?1", params![id])?;
        Ok(())
    }

    /// Masque ou réaffiche une séance précise.
    pub fn set_muted(&self, id: &str, muted: bool) -> Result<()> {
        let changed = self.conn.execute(
            "UPDATE occurrences SET muted = ?2 WHERE id = ?1",
            params![id, muted as i64],
        )?;
        if changed == 0 {
            return Err(TimewrapError::NotFound(format!("séance {id}")));
        }
        Ok(())
    }

    /// Force la catégorie d'une séance, règles ou pas.
    pub fn set_occurrence_category(&self, id: &str, category_id: Option<String>) -> Result<()> {
        let locked = category_id.is_some();
        let changed = self.conn.execute(
            "UPDATE occurrences SET category_id = ?2, category_locked = ?3 WHERE id = ?1",
            params![id, category_id, locked as i64],
        )?;
        if changed == 0 {
            return Err(TimewrapError::NotFound(format!("séance {id}")));
        }
        if !locked {
            self.reapply_rules()?;
        }
        Ok(())
    }

    fn occurrence_origin(&self, id: &str) -> Result<EventOrigin> {
        let origin: Option<String> = self
            .conn
            .query_row(
                "SELECT origin FROM occurrences WHERE id = ?1",
                params![id],
                |row| row.get(0),
            )
            .optional()?;
        origin
            .map(|o| EventOrigin::from_str(&o))
            .ok_or_else(|| TimewrapError::NotFound(format!("séance {id}")))
    }

    pub fn occurrence(&self, id: &str) -> Result<Occurrence> {
        let sql = format!("SELECT {OCC_COLUMNS} {OCC_FROM} WHERE o.id = ?1");
        self.conn
            .query_row(&sql, params![id], read_occurrence)
            .optional()?
            .ok_or_else(|| TimewrapError::NotFound(format!("séance {id}")))
    }

    // -------------------------------------------------------------- conflits

    /// Chevauchements d'un projet d'événement avec l'existant.
    pub fn conflicts_for(
        &self,
        draft_id: Option<&str>,
        start_utc: i64,
        end_utc: i64,
    ) -> Result<Vec<Conflict>> {
        let window = CONFLICT_WINDOW.num_seconds();
        let candidates = self.raw_occurrences(start_utc - window, end_utc + window)?;
        Ok(conflict::against_draft(
            draft_id,
            start_utc,
            end_utc,
            &candidates,
        ))
    }

    /// Tous les chevauchements déjà présents sur une fenêtre.
    pub fn conflicts_between(&self, from_utc: i64, to_utc: i64) -> Result<Vec<ConflictPair>> {
        Ok(conflict::pairs(&self.raw_occurrences(from_utc, to_utc)?))
    }

    fn raw_occurrences(&self, from_utc: i64, to_utc: i64) -> Result<Vec<Occurrence>> {
        let sql = format!(
            "SELECT {OCC_COLUMNS} {OCC_FROM}
             WHERE o.end_utc > ?1 AND o.start_utc < ?2
             ORDER BY o.start_utc"
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(params![from_utc, to_utc], read_occurrence)?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    /// Les séances écartées à la main, pour pouvoir les faire revenir.
    pub fn muted_between(&self, from_utc: i64, to_utc: i64) -> Result<Vec<Occurrence>> {
        let sql = format!(
            "SELECT {OCC_COLUMNS} {OCC_FROM}
             WHERE o.muted = 1 AND o.end_utc > ?1 AND o.start_utc < ?2
             ORDER BY o.start_utc"
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(params![from_utc, to_utc], read_occurrence)?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    // --------------------------------------------------------------- requêtes

    pub fn occurrences_between(&self, from_utc: i64, to_utc: i64) -> Result<Vec<Occurrence>> {
        let sql = format!(
            "SELECT {OCC_COLUMNS} {OCC_FROM}
             WHERE {VISIBLE} AND o.end_utc > ?1 AND o.start_utc < ?2
             ORDER BY o.all_day DESC, o.start_utc, o.summary"
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(params![from_utc, to_utc], read_occurrence)?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    /// Contenu d'une journée : les séances, et ce qu'il reste à y faire.
    pub fn day(&self, epoch_day: i64, now_utc: i64) -> Result<DayAgenda> {
        let (from, to) = self.day_bounds(epoch_day)?;
        let all_day_from = epoch_day * 86_400;
        let all_day_to = all_day_from + 86_400;

        let sql = format!(
            "SELECT {OCC_COLUMNS} {OCC_FROM}
             WHERE {VISIBLE} AND (
                 (o.all_day = 0 AND o.end_utc > ?1 AND o.start_utc < ?2)
                 OR (o.all_day = 1 AND o.start_utc >= ?3 AND o.start_utc < ?4)
             )
             ORDER BY o.all_day DESC, o.start_utc, o.summary"
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(params![from, to, all_day_from, all_day_to], read_occurrence)?;

        Ok(DayAgenda {
            epoch_day,
            occurrences: rows.collect::<std::result::Result<Vec<_>, _>>()?,
            tasks: self.tasks_for_day(epoch_day, self.epoch_day_of(now_utc)?)?,
        })
    }

    pub fn days(&self, epoch_day: i64, days: u32, now_utc: i64) -> Result<Vec<DayAgenda>> {
        (0..days as i64)
            .map(|offset| self.day(epoch_day + offset, now_utc))
            .collect()
    }

    /// Ce qui se passe maintenant, et ce qui vient après.
    pub fn now_view(&self, now_utc: i64) -> Result<NowView> {
        let current = self.query_one(
            "o.all_day = 0 AND o.cancelled = 0 AND o.start_utc <= ? AND o.end_utc > ?",
            "ORDER BY o.start_utc DESC",
            now_utc,
        )?;

        let next = self.query_one(
            "o.all_day = 0 AND o.cancelled = 0 AND o.start_utc > ?",
            "ORDER BY o.start_utc",
            now_utc,
        )?;

        let today = self.epoch_day_of(now_utc)?;
        let day = self.day(today, now_utc)?;
        let rest_of_day = day
            .occurrences
            .into_iter()
            .filter(|o| o.start_utc > now_utc)
            .collect();

        let pending: Vec<&Task> = day.tasks.iter().filter(|t| !t.done).collect();

        Ok(NowView {
            minutes_remaining: current
                .as_ref()
                .map(|o| (o.end_utc - now_utc).div_euclid(60)),
            minutes_until_next: next
                .as_ref()
                .map(|o| (o.start_utc - now_utc).div_euclid(60)),
            current,
            next,
            rest_of_day,
            pending_tasks: pending.len() as u32,
            late_tasks: pending.iter().filter(|t| t.days_late > 0).count() as u32,
        })
    }

    fn query_one(&self, filter: &str, order: &str, now_utc: i64) -> Result<Option<Occurrence>> {
        let sql = format!(
            "SELECT {OCC_COLUMNS} {OCC_FROM}
             WHERE {VISIBLE} AND {filter}
             {order} LIMIT 1"
        );
        // Le filtre porte une ou deux fois l'instant courant selon qu'il teste
        // un intervalle ou une borne : on compte les marqueurs plutôt que de
        // maintenir deux variantes de la requête.
        let values: Vec<i64> = std::iter::repeat_n(now_utc, filter.matches('?').count()).collect();
        Ok(self
            .conn
            .query_row(&sql, rusqlite::params_from_iter(values), read_occurrence)
            .optional()?)
    }

    // -------------------------------------------------------- choses à faire

    /// Les tâches qui concernent une journée.
    ///
    /// Une tâche non cochée reste due : consultée aujourd'hui, la liste montre
    /// donc tout ce qui traîne depuis les jours précédents, avec son retard. Un
    /// jour passé ou à venir, en revanche, ne montre que ce qui lui était propre
    /// — sans quoi la vue Semaine répéterait sept fois la même chose.
    pub fn tasks_for_day(&self, epoch_day: i64, today: i64) -> Result<Vec<Task>> {
        let sql = if epoch_day == today {
            "SELECT id, title, notes, planned_day, done, done_day, position
             FROM tasks
             WHERE (done = 0 AND planned_day <= ?1) OR done_day = ?1
             ORDER BY done, planned_day, position, rowid"
        } else {
            "SELECT id, title, notes, planned_day, done, done_day, position
             FROM tasks
             WHERE planned_day = ?1 OR done_day = ?1
             ORDER BY done, planned_day, position, rowid"
        };

        let mut stmt = self.conn.prepare(sql)?;
        let rows = stmt.query_map(params![epoch_day], |row| read_task(row, epoch_day))?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    /// Tout ce qui reste ouvert, du plus ancien au plus récent.
    pub fn pending_tasks(&self, today: i64) -> Result<Vec<Task>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, title, notes, planned_day, done, done_day, position
             FROM tasks WHERE done = 0 AND planned_day <= ?1
             ORDER BY planned_day, position, rowid",
        )?;
        let rows = stmt.query_map(params![today], |row| read_task(row, today))?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    pub fn add_task(&self, title: &str, planned_day: i64, now_utc: i64) -> Result<Task> {
        let title = title.trim();
        if title.is_empty() {
            return Err(TimewrapError::InvalidEvent(
                "une tâche a besoin d'un intitulé".into(),
            ));
        }
        let position: i64 = self.conn.query_row(
            "SELECT COALESCE(MAX(position), -1) + 1 FROM tasks WHERE planned_day = ?1",
            params![planned_day],
            |row| row.get(0),
        )?;
        let id = new_id("task", now_utc, title);
        self.conn.execute(
            "INSERT INTO tasks (id, title, notes, planned_day, done, created_at, position)
             VALUES (?1, ?2, '', ?3, 0, ?4, ?5)",
            params![id, title, planned_day, now_utc, position],
        )?;
        self.task(&id, planned_day)
    }

    pub fn set_task_done(&self, id: &str, done: bool, today: i64) -> Result<Task> {
        let changed = self.conn.execute(
            "UPDATE tasks SET done = ?2, done_day = ?3 WHERE id = ?1",
            params![id, done as i64, if done { Some(today) } else { None }],
        )?;
        if changed == 0 {
            return Err(TimewrapError::NotFound(format!("tâche {id}")));
        }
        self.task(id, today)
    }

    pub fn update_task(&self, id: &str, title: &str, notes: &str) -> Result<()> {
        let title = title.trim();
        if title.is_empty() {
            return Err(TimewrapError::InvalidEvent(
                "une tâche a besoin d'un intitulé".into(),
            ));
        }
        let changed = self.conn.execute(
            "UPDATE tasks SET title = ?2, notes = ?3 WHERE id = ?1",
            params![id, title, notes.trim()],
        )?;
        if changed == 0 {
            return Err(TimewrapError::NotFound(format!("tâche {id}")));
        }
        Ok(())
    }

    /// Repousse une tâche à un autre jour, explicitement.
    pub fn move_task(&self, id: &str, planned_day: i64) -> Result<()> {
        let changed = self.conn.execute(
            "UPDATE tasks SET planned_day = ?2 WHERE id = ?1",
            params![id, planned_day],
        )?;
        if changed == 0 {
            return Err(TimewrapError::NotFound(format!("tâche {id}")));
        }
        Ok(())
    }

    pub fn delete_task(&self, id: &str) -> Result<()> {
        self.conn
            .execute("DELETE FROM tasks WHERE id = ?1", params![id])?;
        Ok(())
    }

    fn task(&self, id: &str, reference_day: i64) -> Result<Task> {
        let mut stmt = self.conn.prepare(
            "SELECT id, title, notes, planned_day, done, done_day, position
             FROM tasks WHERE id = ?1",
        )?;
        stmt.query_row(params![id], |row| read_task(row, reference_day))
            .optional()?
            .ok_or_else(|| TimewrapError::NotFound(format!("tâche {id}")))
    }

    // ----------------------------------------------------------------- rappels

    /// Les rappels à programmer, du plus proche au plus lointain.
    ///
    /// Le cœur décide de l'instant, l'application se contente de poser les
    /// alarmes : la règle « quinze minutes avant, sauf séance annulée ou
    /// masquée » n'a aucune raison de vivre dans du Kotlin.
    pub fn reminders(&self, now_utc: i64, horizon_days: i64, limit: u32) -> Result<Vec<Reminder>> {
        let settings = self.settings()?;
        if !settings.reminders_enabled {
            return Ok(Vec::new());
        }
        let lead = settings.reminder_lead_minutes as i64 * 60;
        let to = now_utc + horizon_days * 86_400;

        Ok(self
            .occurrences_between(now_utc, to)?
            .into_iter()
            .filter(|o| !o.all_day && !o.cancelled && o.start_utc > now_utc)
            .map(|o| Reminder {
                occurrence_id: o.id,
                title: o.title,
                location: o.location,
                start_utc: o.start_utc,
                trigger_utc: o.start_utc - lead,
            })
            .filter(|r| r.trigger_utc > now_utc)
            .take(limit as usize)
            .collect())
    }

    // --------------------------------------------------------------- réglages

    pub fn settings(&self) -> Result<Settings> {
        Ok(Settings {
            source_url: self.get_setting("source_url")?.unwrap_or_default(),
            sync_enabled: self.flag("sync_enabled", false)?,
            sync_interval_hours: self.number("sync_interval_hours", 6)?,
            last_sync_utc: self
                .get_setting("last_sync_utc")?
                .and_then(|v| v.parse().ok()),
            notify_changes: self.flag("notify_changes", true)?,
            reminders_enabled: self.flag("reminders_enabled", false)?,
            reminder_lead_minutes: self.number("reminder_lead_minutes", 15)?,
            digest_enabled: self.flag("digest_enabled", false)?,
            digest_minutes: self.number("digest_minutes", 7 * 60)?,
        })
    }

    pub fn update_settings(&self, settings: &Settings) -> Result<Settings> {
        self.set_setting("source_url", settings.source_url.trim())?;
        self.set_setting("sync_enabled", bool_str(settings.sync_enabled))?;
        self.set_setting(
            "sync_interval_hours",
            &settings.sync_interval_hours.clamp(1, 168).to_string(),
        )?;
        self.set_setting("notify_changes", bool_str(settings.notify_changes))?;
        self.set_setting("reminders_enabled", bool_str(settings.reminders_enabled))?;
        self.set_setting(
            "reminder_lead_minutes",
            &settings.reminder_lead_minutes.clamp(0, 240).to_string(),
        )?;
        self.set_setting("digest_enabled", bool_str(settings.digest_enabled))?;
        self.set_setting(
            "digest_minutes",
            &settings.digest_minutes.min(24 * 60 - 1).to_string(),
        )?;
        self.settings()
    }

    fn flag(&self, key: &str, default: bool) -> Result<bool> {
        Ok(self.get_setting(key)?.map(|v| v == "1").unwrap_or(default))
    }

    fn number(&self, key: &str, default: u32) -> Result<u32> {
        Ok(self
            .get_setting(key)?
            .and_then(|v| v.parse().ok())
            .unwrap_or(default))
    }

    fn get_setting(&self, key: &str) -> Result<Option<String>> {
        Ok(self
            .conn
            .query_row(
                "SELECT value FROM settings WHERE key = ?1",
                params![key],
                |row| row.get(0),
            )
            .optional()?)
    }

    fn set_setting(&self, key: &str, value: &str) -> Result<()> {
        self.conn.execute(
            "INSERT INTO settings (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, value],
        )?;
        Ok(())
    }

    // ---------------------------------------------------------------- fuseaux

    /// Bornes UTC d'une journée locale. L'heure d'été rend ces journées longues
    /// de 23 ou 25 heures deux fois par an : on les calcule, on ne les suppose pas.
    fn day_bounds(&self, epoch_day: i64) -> Result<(i64, i64)> {
        Ok((
            self.local_midnight(epoch_day)?,
            self.local_midnight(epoch_day + 1)?,
        ))
    }

    fn local_midnight(&self, epoch_day: i64) -> Result<i64> {
        let date = epoch_day_to_date(epoch_day)?;
        let naive = date
            .and_hms_opt(0, 0, 0)
            .ok_or_else(|| TimewrapError::Parse("date invalide".into()))?;
        // Au passage à l'heure d'été, minuit local peut ne pas exister dans
        // certains fuseaux : on prend alors le premier instant valide du jour.
        let dt = self
            .tz
            .from_local_datetime(&naive)
            .earliest()
            .unwrap_or_else(|| self.tz.from_utc_datetime(&naive));
        Ok(dt.with_timezone(&Utc).timestamp())
    }

    pub fn epoch_day_of(&self, utc: i64) -> Result<i64> {
        let dt = DateTime::from_timestamp(utc, 0)
            .ok_or_else(|| TimewrapError::Parse(format!("horodatage hors limites : {utc}")))?;
        let date = dt.with_timezone(&self.tz).date_naive();
        Ok(date
            .signed_duration_since(NaiveDate::from_ymd_opt(1970, 1, 1).unwrap())
            .num_days())
    }
}

/// Une séance telle que la comparaison entre deux imports la retient.
struct Snapshot {
    uid: String,
    title: String,
    location: String,
    start_utc: i64,
    cancelled: bool,
}

impl Snapshot {
    /// Même série, même créneau : c'est la même séance.
    fn same_slot(&self, other: &Snapshot) -> bool {
        self.uid == other.uid && self.start_utc == other.start_utc
    }

    /// Vrai si cette séance retrouve son exact équivalent dans une liste.
    fn matched(&self, others: &[Snapshot]) -> bool {
        others.iter().any(|o| o.same_slot(self))
    }
}

fn change(kind: ChangeKind, snapshot: &Snapshot, summary: String) -> Change {
    Change {
        kind,
        title: snapshot.title.clone(),
        summary,
        start_utc: snapshot.start_utc,
    }
}

/// Une séance et ce dont les règles ont besoin pour se prononcer sur elle.
struct RuleInput {
    id: String,
    summary: String,
    location: String,
    description: String,
    display_title: String,
    category_id: Option<String>,
    locked: bool,
    hidden: bool,
    properties: Vec<Property>,
}

impl RuleInput {
    fn fields(&self) -> rules::Fields<'_> {
        rules::Fields {
            title: &self.summary,
            location: &self.location,
            description: &self.description,
            properties: &self.properties,
        }
    }
}

fn read_occurrence(row: &rusqlite::Row<'_>) -> rusqlite::Result<Occurrence> {
    Ok(Occurrence {
        id: row.get(0)?,
        color: row.get::<_, i64>(1)? as u32,
        uid: row.get(2)?,
        title: row.get(3)?,
        raw_title: row.get(4)?,
        location: row.get(5)?,
        description: row.get(6)?,
        start_utc: row.get(7)?,
        end_utc: row.get(8)?,
        all_day: row.get::<_, i64>(9)? != 0,
        cancelled: row.get::<_, i64>(10)? != 0,
        origin: EventOrigin::from_str(&row.get::<_, String>(11)?),
        category_id: row.get(12)?,
        category_name: row.get(13)?,
        category_label: row.get(14)?,
        hidden: row.get::<_, i64>(15)? != 0,
    })
}

fn read_task(row: &rusqlite::Row<'_>, reference_day: i64) -> rusqlite::Result<Task> {
    let planned_day: i64 = row.get(3)?;
    let done: bool = row.get::<_, i64>(4)? != 0;
    Ok(Task {
        id: row.get(0)?,
        title: row.get(1)?,
        notes: row.get(2)?,
        planned_day,
        done,
        done_day: row.get(5)?,
        days_late: if done {
            0
        } else {
            (reference_day - planned_day).max(0)
        },
        position: row.get::<_, i64>(6)? as i32,
    })
}

fn rule_name(rule: &Rule) -> String {
    let name = rule.name.trim();
    if name.is_empty() {
        rule.pattern.trim().to_string()
    } else {
        name.to_string()
    }
}

/// Une pastille tient en quatre caractères : on prend les initiales des mots,
/// ou le début du mot unique.
fn short_label(value: &str) -> String {
    let words: Vec<&str> = value.split_whitespace().collect();
    if words.len() >= 2 {
        words
            .iter()
            .take(3)
            .filter_map(|w| w.chars().next())
            .collect::<String>()
            .to_uppercase()
    } else {
        value.chars().take(4).collect::<String>().to_uppercase()
    }
}

fn bool_str(value: bool) -> &'static str {
    if value { "1" } else { "0" }
}

/// Identifiant stable : même emploi du temps, même UID, même début, même
/// identifiant. C'est ce qui permet aux personnalisations de survivre à un
/// réimport de l'ENT.
fn occurrence_id(calendar_id: &str, uid: &str, start_utc: i64) -> String {
    format!("{calendar_id}|{uid}|{start_utc}")
}

fn new_id(prefix: &str, now_utc: i64, seed: &str) -> String {
    let digest: u64 = seed.bytes().fold(0xcbf2_9ce4_8422_2325u64, |acc, b| {
        (acc ^ u64::from(b)).wrapping_mul(0x0000_0100_0000_01b3)
    });
    format!("{prefix}_{now_utc:x}_{digest:x}")
}

fn horizon(now_utc: i64) -> (i64, i64) {
    (
        now_utc - HORIZON_PAST.num_seconds(),
        now_utc + HORIZON_FUTURE.num_seconds(),
    )
}

fn epoch_day_to_date(epoch_day: i64) -> Result<NaiveDate> {
    NaiveDate::from_ymd_opt(1970, 1, 1)
        .and_then(|epoch| epoch.checked_add_signed(Duration::days(epoch_day)))
        .ok_or_else(|| TimewrapError::Parse(format!("jour hors limites : {epoch_day}")))
}

fn weekday_name(index: u32) -> &'static str {
    match index {
        0 => "lundi",
        1 => "mardi",
        2 => "mercredi",
        3 => "jeudi",
        4 => "vendredi",
        5 => "samedi",
        _ => "dimanche",
    }
}

fn month_name(month: u32) -> &'static str {
    match month {
        1 => "janvier",
        2 => "février",
        3 => "mars",
        4 => "avril",
        5 => "mai",
        6 => "juin",
        7 => "juillet",
        8 => "août",
        9 => "septembre",
        10 => "octobre",
        11 => "novembre",
        _ => "décembre",
    }
}

fn parse_tz(name: &str) -> Result<Tz> {
    name.parse()
        .map_err(|_| TimewrapError::UnknownTimezone(name.to_string()))
}
