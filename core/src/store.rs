//! Stockage local et requêtes d'agenda.
//!
//! Le parti pris structurant : les récurrences sont développées à l'import et
//! écrites en dur dans `occurrences`. Afficher une journée ou une semaine
//! devient alors un simple balayage d'index, et « quel est mon prochain cours »
//! une requête à une ligne. Le flux `.ics` d'origine est conservé pour pouvoir
//! re-développer plus loin dans le temps sans redemander le fichier.
//!
//! Les décisions des règles visuelles suivent le même principe : elles sont
//! matérialisées dans les colonnes de `occurrences` au moment de l'écriture,
//! puis rejouées d'un bloc quand une règle change. Une vue ne calcule jamais
//! rien, elle lit.

use chrono::{DateTime, Duration, NaiveDate, TimeZone, Utc};
use chrono_tz::Tz;
use rusqlite::types::Value;
use rusqlite::{Connection, OptionalExtension, params, params_from_iter};

use crate::conflict;
use crate::error::{Result, TimewrapError};
use crate::ics;
use crate::model::{
    Calendar, CalendarKind, CalendarSummary, Category, Conflict, ConflictPair, DayAgenda,
    EventDraft, EventOrigin, ImportReport, NowView, Occurrence, Resolution, Rule, RuleField,
    RuleMatch, RuleSuggestion, SaveOutcome,
};
use crate::rules;

/// Combien de passé on garde développé.
const HORIZON_PAST: Duration = Duration::days(90);
/// Combien d'avenir on développe d'avance.
const HORIZON_FUTURE: Duration = Duration::days(400);
/// En deçà de cette marge restante, on re-développe la fenêtre.
const HORIZON_MARGIN: Duration = Duration::days(60);
/// Fenêtre examinée autour d'un événement pour y chercher des chevauchements.
const CONFLICT_WINDOW: Duration = Duration::days(1);
/// Profondeur sur laquelle l'écran d'accueil compte les conflits d'un agenda.
const CONFLICT_LOOKAHEAD: Duration = Duration::days(30);
/// Un décalage en cascade doit finir par retomber sur un créneau libre.
const MAX_SHIFT_STEPS: u8 = 8;

/// Couleurs attribuées aux agendas et aux catégories dans l'ordre de création.
const PALETTE: [u32; 8] = [
    0xFF4C5FD5, // bleu-violet
    0xFF2E9E7A, // vert
    0xFFD2694B, // terre cuite
    0xFF8155C6, // violet
    0xFF3C87C8, // bleu
    0xFFC2528A, // framboise
    0xFF7A8A3C, // olive
    0xFFB08236, // ambre
];

/// Colonnes d'une occurrence telle que l'interface la reçoit.
///
/// La couleur et le titre sont résolus ici, en SQL : la catégorie posée par une
/// règle l'emporte sur la couleur de l'agenda, et le renommage sur l'intitulé
/// d'origine — que l'on continue de renvoyer à part, pour les écrans de réglage.
const OCC_COLUMNS: &str = "o.id, o.calendar_id, c.name, COALESCE(cat.color, c.color), o.uid,
     CASE WHEN o.display_title <> '' THEN o.display_title ELSE o.summary END,
     o.summary, o.location, o.description, o.start_utc, o.end_utc, o.all_day,
     o.cancelled, o.origin, o.category_id, COALESCE(cat.name, ''),
     COALESCE(cat.label, ''),
     CASE WHEN o.hidden = 1 OR o.muted = 1 THEN 1 ELSE 0 END";

const OCC_FROM: &str = "FROM occurrences o
     JOIN calendars c ON c.id = o.calendar_id
     LEFT JOIN categories cat ON cat.id = o.category_id";

/// Restriction d'une vue à certains agendas.
///
/// Vide ou absent, on retombe sur le comportement habituel : tous les agendas
/// que l'utilisateur n'a pas masqués. Renseignée, la sélection l'emporte sur la
/// visibilité — ouvrir le dossier « Perso » doit le montrer, même s'il est
/// décoché dans la vue d'ensemble.
pub type Scope = Option<Vec<String>>;

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

        // v2 : dossiers ordonnables, événements locaux, catégories et règles.
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
                     calendar_id    TEXT REFERENCES calendars(id) ON DELETE CASCADE,
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

            // Les agendas déjà présents gardent leur ordre d'arrivée.
            self.conn.execute_batch(
                "UPDATE calendars SET position = (
                     SELECT COUNT(*) FROM calendars older
                     WHERE older.created_at < calendars.created_at
                 );",
            )?;
        }

        Ok(())
    }

    /// Exécute un bloc d'écritures d'un seul tenant.
    ///
    /// Un import qui échoue à mi-course ne doit pas laisser un agenda à moitié
    /// développé : soit tout est écrit, soit rien.
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

    // ---------------------------------------------------------------- agendas

    pub fn calendars(&self) -> Result<Vec<Calendar>> {
        let mut stmt = self.conn.prepare(
            "SELECT c.id, c.name, c.kind, c.source, c.color, c.visible, c.last_sync,
                    (SELECT COUNT(DISTINCT o.uid) FROM occurrences o WHERE o.calendar_id = c.id),
                    c.position
             FROM calendars c
             ORDER BY c.position, c.created_at",
        )?;
        let rows = stmt.query_map([], read_calendar)?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    pub fn calendar(&self, calendar_id: &str) -> Result<Calendar> {
        let mut stmt = self.conn.prepare(
            "SELECT c.id, c.name, c.kind, c.source, c.color, c.visible, c.last_sync,
                    (SELECT COUNT(DISTINCT o.uid) FROM occurrences o WHERE o.calendar_id = c.id),
                    c.position
             FROM calendars c WHERE c.id = ?1",
        )?;
        stmt.query_row(params![calendar_id], read_calendar)
            .optional()?
            .ok_or_else(|| TimewrapError::CalendarNotFound(calendar_id.to_string()))
    }

    /// Crée un agenda vide — un dossier, à remplir à la main.
    pub fn create_calendar(
        &self,
        name: &str,
        kind: CalendarKind,
        source: &str,
        color: Option<u32>,
        now_utc: i64,
    ) -> Result<Calendar> {
        let name = name.trim();
        if name.is_empty() {
            return Err(TimewrapError::InvalidEvent(
                "un agenda a besoin d'un nom".into(),
            ));
        }
        let id = new_id("cal", now_utc, name);
        let count: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM calendars", [], |row| row.get(0))?;
        let color = color.unwrap_or(PALETTE[(count as usize) % PALETTE.len()]);

        self.conn.execute(
            "INSERT INTO calendars (id, name, kind, source, color, visible, created_at, raw_ics, position)
             VALUES (?1, ?2, ?3, ?4, ?5, 1, ?6, '', ?7)",
            params![id, name, kind.as_str(), source, color as i64, now_utc, count],
        )?;
        self.calendar(&id)
    }

    /// Crée un agenda à partir d'un flux `.ics` et développe ses occurrences.
    pub fn import_ics(
        &self,
        name: &str,
        kind: CalendarKind,
        source: &str,
        ics_text: &str,
        now_utc: i64,
    ) -> Result<ImportReport> {
        let calendar = self.create_calendar(name, kind, source, None, now_utc)?;
        self.conn.execute(
            "UPDATE calendars SET raw_ics = ?2 WHERE id = ?1",
            params![calendar.id, ics_text],
        )?;
        self.rebuild(&calendar.id, ics_text, now_utc)
    }

    /// Remplace le contenu d'un agenda existant par un flux `.ics` plus récent.
    pub fn reimport_ics(
        &self,
        calendar_id: &str,
        ics_text: &str,
        now_utc: i64,
    ) -> Result<ImportReport> {
        self.require_calendar(calendar_id)?;
        self.conn.execute(
            "UPDATE calendars SET raw_ics = ?2 WHERE id = ?1",
            params![calendar_id, ics_text],
        )?;
        self.rebuild(calendar_id, ics_text, now_utc)
    }

    /// Développe le flux et réécrit les occurrences importées de cet agenda.
    ///
    /// Le remplacement est intégral, donc idempotent : réimporter deux fois le
    /// même fichier laisse exactement le même état. Deux choses y survivent
    /// pourtant, parce qu'elles n'appartiennent pas au flux : les événements
    /// saisis à la main dans le même agenda, et les séances masquées à la suite
    /// d'un conflit — les identifiants d'occurrence étant stables, on les repose.
    fn rebuild(&self, calendar_id: &str, ics_text: &str, now_utc: i64) -> Result<ImportReport> {
        let (from, to) = horizon(now_utc);
        let expansion = ics::expand(ics_text, from, to, self.tz)?;
        let rules = self.rules_raw()?;

        self.transact(|| {
            let muted = self.muted_ids(calendar_id)?;

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
                    let outcome = rules::apply(
                        &rules,
                        &rules::Fields {
                            calendar_id,
                            title: &occurrence.summary,
                            location: &occurrence.location,
                            description: &occurrence.description,
                        },
                    );
                    stmt.execute(params![
                        occurrence_id(calendar_id, &occurrence.uid, occurrence.start_utc),
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
            Ok(())
        })?;

        Ok(ImportReport {
            calendar_id: calendar_id.to_string(),
            events: expansion.events,
            occurrences: expansion.occurrences.len() as u32,
            skipped: expansion.skipped,
            first_start_utc: expansion.occurrences.first().map(|o| o.start_utc),
            last_start_utc: expansion.occurrences.last().map(|o| o.start_utc),
        })
    }

    fn muted_ids(&self, calendar_id: &str) -> Result<Vec<String>> {
        let mut stmt = self
            .conn
            .prepare("SELECT id FROM occurrences WHERE calendar_id = ?1 AND muted = 1")?;
        let rows = stmt.query_map(params![calendar_id], |row| row.get(0))?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    /// Re-développe tous les agendas si l'horizon devient trop proche.
    ///
    /// Sans cela, une application ouverte un an après le dernier import
    /// n'afficherait plus rien au-delà de la fenêtre initiale.
    pub fn ensure_horizon(&self, now_utc: i64) -> Result<bool> {
        let horizon_to: Option<i64> = self.get_setting("horizon_to")?.and_then(|v| v.parse().ok());

        let needs_refresh = match horizon_to {
            Some(to) => now_utc + HORIZON_MARGIN.num_seconds() > to,
            None => false,
        };
        if !needs_refresh {
            return Ok(false);
        }

        let sources: Vec<(String, String)> = {
            let mut stmt = self
                .conn
                .prepare("SELECT id, raw_ics FROM calendars WHERE raw_ics <> ''")?;
            let rows = stmt.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?;
            rows.collect::<std::result::Result<Vec<_>, _>>()?
        };

        for (id, raw) in sources {
            self.rebuild(&id, &raw, now_utc)?;
        }
        Ok(true)
    }

    pub fn set_visible(&self, calendar_id: &str, visible: bool) -> Result<()> {
        self.require_calendar(calendar_id)?;
        self.conn.execute(
            "UPDATE calendars SET visible = ?2 WHERE id = ?1",
            params![calendar_id, visible as i64],
        )?;
        Ok(())
    }

    pub fn set_color(&self, calendar_id: &str, color: u32) -> Result<()> {
        self.require_calendar(calendar_id)?;
        self.conn.execute(
            "UPDATE calendars SET color = ?2 WHERE id = ?1",
            params![calendar_id, color as i64],
        )?;
        Ok(())
    }

    pub fn rename_calendar(&self, calendar_id: &str, name: &str) -> Result<()> {
        self.require_calendar(calendar_id)?;
        self.conn.execute(
            "UPDATE calendars SET name = ?2 WHERE id = ?1",
            params![calendar_id, name],
        )?;
        Ok(())
    }

    /// Range un agenda à une nouvelle place dans la liste d'accueil.
    pub fn move_calendar(&self, calendar_id: &str, target: i32) -> Result<()> {
        let mut ids: Vec<String> = self.calendars()?.into_iter().map(|c| c.id).collect();
        let Some(from) = ids.iter().position(|id| id == calendar_id) else {
            return Err(TimewrapError::CalendarNotFound(calendar_id.to_string()));
        };
        let to = (target.max(0) as usize).min(ids.len().saturating_sub(1));
        let moved = ids.remove(from);
        ids.insert(to, moved);

        self.transact(|| {
            for (position, id) in ids.iter().enumerate() {
                self.conn.execute(
                    "UPDATE calendars SET position = ?2 WHERE id = ?1",
                    params![id, position as i64],
                )?;
            }
            Ok(())
        })
    }

    pub fn delete_calendar(&self, calendar_id: &str) -> Result<()> {
        self.require_calendar(calendar_id)?;
        self.conn
            .execute("DELETE FROM calendars WHERE id = ?1", params![calendar_id])?;
        Ok(())
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
                .execute("DELETE FROM categories WHERE id = ?1", params![id])?;
            // Les clés étrangères remettent déjà `category_id` à NULL ; on le
            // fait explicitement pour rester correct si la base a été ouverte
            // sans `foreign_keys`.
            self.conn.execute(
                "UPDATE occurrences SET category_id = NULL WHERE category_id = ?1",
                params![id],
            )?;
            Ok(())
        })
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

    // ----------------------------------------------------------------- règles

    pub fn rules(&self) -> Result<Vec<Rule>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, calendar_id, field, match_kind, pattern, case_sensitive,
                    category_id, rename_to, hide, priority, enabled
             FROM rules ORDER BY priority, rowid",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(Rule {
                id: row.get(0)?,
                name: row.get(1)?,
                calendar_id: row.get(2)?,
                field: RuleField::from_str(&row.get::<_, String>(3)?),
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
        let mut rules = rows.collect::<std::result::Result<Vec<_>, _>>()?;

        // Le compte de correspondances n'est pas stocké : il se déduit de l'état
        // courant, et un chiffre périmé serait pire que pas de chiffre du tout.
        let samples = self.rule_inputs()?;
        for rule in &mut rules {
            rule.match_count = samples
                .iter()
                .filter(|(calendar_id, title, location, description)| {
                    rules::matches(
                        rule,
                        &rules::Fields {
                            calendar_id,
                            title,
                            location,
                            description,
                        },
                    )
                })
                .count() as u32;
        }
        Ok(rules)
    }

    /// Enregistre une règle. Un identifiant vide en crée une nouvelle.
    pub fn save_rule(&self, rule: &Rule, now_utc: i64) -> Result<Rule> {
        if rule.pattern.trim().is_empty() {
            return Err(TimewrapError::InvalidEvent(
                "une règle a besoin d'un motif à reconnaître".into(),
            ));
        }
        let id = if rule.id.trim().is_empty() {
            let id = new_id("rule", now_utc, &rule.pattern);
            let next: i64 = self.conn.query_row(
                "SELECT COALESCE(MAX(priority), -1) + 1 FROM rules",
                [],
                |row| row.get(0),
            )?;
            self.conn.execute(
                "INSERT INTO rules (id, name, calendar_id, field, match_kind, pattern,
                                    case_sensitive, category_id, rename_to, hide, priority, enabled)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
                params![
                    id,
                    rule_name(rule),
                    rule.calendar_id,
                    rule.field.as_str(),
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
                "UPDATE rules SET name = ?2, calendar_id = ?3, field = ?4, match_kind = ?5,
                                  pattern = ?6, case_sensitive = ?7, category_id = ?8,
                                  rename_to = ?9, hide = ?10, priority = ?11, enabled = ?12
                 WHERE id = ?1",
                params![
                    rule.id,
                    rule_name(rule),
                    rule.calendar_id,
                    rule.field.as_str(),
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

    /// Rejoue toutes les règles sur toutes les occurrences.
    ///
    /// Renvoie le nombre de séances dont l'apparence a changé — c'est ce que
    /// l'écran des règles affiche après une modification, pour que l'effet soit
    /// visible immédiatement même hors de la fenêtre consultée.
    pub fn reapply_rules(&self) -> Result<u32> {
        let rules = self.rules_raw()?;

        struct Row {
            id: String,
            calendar_id: String,
            summary: String,
            location: String,
            description: String,
            display_title: String,
            category_id: Option<String>,
            locked: bool,
            hidden: bool,
        }

        let rows: Vec<Row> = {
            let mut stmt = self.conn.prepare(
                "SELECT id, calendar_id, summary, location, description, display_title,
                        category_id, category_locked, hidden
                 FROM occurrences",
            )?;
            let mapped = stmt.query_map([], |row| {
                Ok(Row {
                    id: row.get(0)?,
                    calendar_id: row.get(1)?,
                    summary: row.get(2)?,
                    location: row.get(3)?,
                    description: row.get(4)?,
                    display_title: row.get(5)?,
                    category_id: row.get(6)?,
                    locked: row.get::<_, i64>(7)? != 0,
                    hidden: row.get::<_, i64>(8)? != 0,
                })
            })?;
            mapped.collect::<std::result::Result<Vec<_>, _>>()?
        };

        let mut touched = 0u32;
        self.transact(|| {
            let mut stmt = self.conn.prepare(
                "UPDATE occurrences SET display_title = ?2, category_id = ?3, hidden = ?4
                 WHERE id = ?1",
            )?;
            for row in &rows {
                let outcome = rules::apply(
                    &rules,
                    &rules::Fields {
                        calendar_id: &row.calendar_id,
                        title: &row.summary,
                        location: &row.location,
                        description: &row.description,
                    },
                );
                let display_title = outcome.display_title.unwrap_or_default();
                // Une catégorie choisie à la main sur un événement l'emporte sur
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

    /// Les règles sans leur compte de correspondances : c'est cette version
    /// qu'utilisent les écritures, qui n'ont que faire du chiffre.
    fn rules_raw(&self) -> Result<Vec<Rule>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, calendar_id, field, match_kind, pattern, case_sensitive,
                    category_id, rename_to, hide, priority, enabled
             FROM rules ORDER BY priority, rowid",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(Rule {
                id: row.get(0)?,
                name: row.get(1)?,
                calendar_id: row.get(2)?,
                field: RuleField::from_str(&row.get::<_, String>(3)?),
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

    fn rule_inputs(&self) -> Result<Vec<(String, String, String, String)>> {
        let mut stmt = self
            .conn
            .prepare("SELECT calendar_id, summary, location, description FROM occurrences")?;
        let rows = stmt.query_map([], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
        })?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    /// Ce que le cœur propose de classer, au vu de ce qui a été importé.
    pub fn rule_suggestions(&self, scope: &Scope) -> Result<Vec<RuleSuggestion>> {
        let samples: Vec<rules::Sample> = {
            let (clause, values) = match scope {
                Some(ids) if !ids.is_empty() => {
                    let placeholders = ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
                    (
                        format!("WHERE calendar_id IN ({placeholders})"),
                        ids.iter().map(|id| Value::Text(id.clone())).collect(),
                    )
                }
                _ => (String::new(), Vec::<Value>::new()),
            };
            let sql = format!("SELECT summary FROM occurrences {clause}");
            let mut stmt = self.conn.prepare(&sql)?;
            let rows = stmt.query_map(params_from_iter(values), |row| {
                Ok(rules::Sample { title: row.get(0)? })
            })?;
            rows.collect::<std::result::Result<Vec<_>, _>>()?
        };

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
                calendar_id: None,
                field: suggestion.field,
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

    // ------------------------------------------------------ événements locaux

    /// Écrit un événement, en traitant les chevauchements selon `resolution`.
    ///
    /// C'est le point de passage unique : toute création ou modification d'un
    /// créneau passe par ici, donc aucune ne peut échapper au contrôle.
    pub fn save_event(
        &self,
        draft: &EventDraft,
        resolution: Resolution,
        now_utc: i64,
    ) -> Result<SaveOutcome> {
        self.require_calendar(&draft.calendar_id)?;
        if draft.end_utc <= draft.start_utc {
            return Err(TimewrapError::InvalidEvent(
                "un événement doit finir après avoir commencé".into(),
            ));
        }
        if let Some(id) = &draft.id {
            let origin = self.occurrence_origin(id)?;
            if origin != EventOrigin::Local {
                return Err(TimewrapError::InvalidEvent(
                    "une séance importée ne se modifie pas ici : elle serait écrasée au prochain import".into(),
                ));
            }
        }

        let duration = draft.end_utc - draft.start_utc;
        let mut start = draft.start_utc;
        let mut end = draft.end_utc;
        let mut conflicts =
            self.conflicts_for(draft.id.as_deref(), &draft.calendar_id, start, end)?;
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
                        conflicts = self.conflicts_for(
                            draft.id.as_deref(),
                            &draft.calendar_id,
                            start,
                            end,
                        )?;
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

        let rules = self.rules_raw()?;
        let outcome = rules::apply(
            &rules,
            &rules::Fields {
                calendar_id: &draft.calendar_id,
                title,
                location: &draft.location,
                description: &draft.description,
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
                 calendar_id = excluded.calendar_id,
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
                draft.calendar_id,
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

        Ok(SaveOutcome {
            saved: Some(self.occurrence(&id)?),
            conflicts,
            blocked: false,
            removed,
            hidden,
            shifted_minutes,
        })
    }

    /// Fait place nette : supprime ce qui est local, masque ce qui est importé.
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
                "une séance importée ne se supprime pas : masquez-la ou retirez son agenda".into(),
            ));
        }
        self.conn
            .execute("DELETE FROM occurrences WHERE id = ?1", params![id])?;
        Ok(())
    }

    /// Masque ou réaffiche une séance précise, sans toucher à son agenda.
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
    ///
    /// La visibilité des agendas est ignorée à dessein : un agenda décoché reste
    /// un engagement pris, et poser un rendez-vous dessus sans être prévenu
    /// serait le meilleur moyen de le découvrir trop tard.
    pub fn conflicts_for(
        &self,
        draft_id: Option<&str>,
        calendar_id: &str,
        start_utc: i64,
        end_utc: i64,
    ) -> Result<Vec<Conflict>> {
        let window = CONFLICT_WINDOW.num_seconds();
        let candidates = self.raw_occurrences(start_utc - window, end_utc + window)?;
        Ok(conflict::against_draft(
            draft_id,
            calendar_id,
            start_utc,
            end_utc,
            &candidates,
        ))
    }

    /// Tous les chevauchements déjà présents sur une fenêtre.
    pub fn conflicts_between(
        &self,
        from_utc: i64,
        to_utc: i64,
        scope: &Scope,
    ) -> Result<Vec<ConflictPair>> {
        let occurrences = match scope {
            Some(ids) if !ids.is_empty() => self
                .raw_occurrences(from_utc, to_utc)?
                .into_iter()
                .filter(|o| ids.contains(&o.calendar_id))
                .collect(),
            _ => self.raw_occurrences(from_utc, to_utc)?,
        };
        Ok(conflict::pairs(&occurrences))
    }

    /// Les séances écartées à la main, pour pouvoir les faire revenir.
    ///
    /// Sans cette liste, « Remplacer » serait sans retour : la séance masquée
    /// disparaîtrait des vues comme du gestionnaire de conflits, donc de partout.
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

    /// Occurrences d'une fenêtre, visibilité des agendas ignorée : c'est la
    /// matière première du moteur de conflits, qui doit tout voir.
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

    // --------------------------------------------------------------- requêtes

    /// Occurrences chevauchant `[from_utc, to_utc)`.
    pub fn occurrences_between(
        &self,
        from_utc: i64,
        to_utc: i64,
        scope: &Scope,
    ) -> Result<Vec<Occurrence>> {
        let (clause, mut values) = scope_clause(scope);
        let sql = format!(
            "SELECT {OCC_COLUMNS} {OCC_FROM}
             WHERE {clause} AND o.hidden = 0 AND o.muted = 0
                   AND o.end_utc > ? AND o.start_utc < ?
             ORDER BY o.all_day DESC, o.start_utc, o.summary"
        );
        values.push(Value::Integer(from_utc));
        values.push(Value::Integer(to_utc));

        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(params_from_iter(values), read_occurrence)?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    /// Contenu d'une journée, `epoch_day` étant compté comme `LocalDate.toEpochDay()`.
    pub fn day(&self, epoch_day: i64, scope: &Scope) -> Result<DayAgenda> {
        let (from, to) = self.day_bounds(epoch_day)?;
        let all_day_from = epoch_day * 86_400;
        let all_day_to = all_day_from + 86_400;

        let (clause, mut values) = scope_clause(scope);
        let sql = format!(
            "SELECT {OCC_COLUMNS} {OCC_FROM}
             WHERE {clause} AND o.hidden = 0 AND o.muted = 0 AND (
                 (o.all_day = 0 AND o.end_utc > ? AND o.start_utc < ?)
                 OR (o.all_day = 1 AND o.start_utc >= ? AND o.start_utc < ?)
             )
             ORDER BY o.all_day DESC, o.start_utc, o.summary"
        );
        values.extend([
            Value::Integer(from),
            Value::Integer(to),
            Value::Integer(all_day_from),
            Value::Integer(all_day_to),
        ]);

        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(params_from_iter(values), read_occurrence)?;
        Ok(DayAgenda {
            epoch_day,
            occurrences: rows.collect::<std::result::Result<Vec<_>, _>>()?,
        })
    }

    /// `days` journées consécutives à partir de `epoch_day`.
    pub fn days(&self, epoch_day: i64, days: u32, scope: &Scope) -> Result<Vec<DayAgenda>> {
        (0..days as i64)
            .map(|offset| self.day(epoch_day + offset, scope))
            .collect()
    }

    /// Ce qui se passe maintenant, et ce qui vient après.
    pub fn now_view(&self, now_utc: i64, scope: &Scope) -> Result<NowView> {
        let current = self.query_one(
            "o.all_day = 0 AND o.cancelled = 0 AND o.start_utc <= ? AND o.end_utc > ?",
            "ORDER BY o.start_utc DESC",
            now_utc,
            scope,
        )?;

        let next = self.query_one(
            "o.all_day = 0 AND o.cancelled = 0 AND o.start_utc > ?",
            "ORDER BY o.start_utc",
            now_utc,
            scope,
        )?;

        let today = self.epoch_day_of(now_utc)?;
        let rest_of_day = self
            .day(today, scope)?
            .occurrences
            .into_iter()
            .filter(|o| o.start_utc > now_utc)
            .collect();

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
        })
    }

    fn query_one(
        &self,
        filter: &str,
        order: &str,
        now_utc: i64,
        scope: &Scope,
    ) -> Result<Option<Occurrence>> {
        let (clause, mut values) = scope_clause(scope);
        let sql = format!(
            "SELECT {OCC_COLUMNS} {OCC_FROM}
             WHERE {clause} AND o.hidden = 0 AND o.muted = 0 AND {filter}
             {order} LIMIT 1"
        );
        // Le filtre porte une ou deux fois l'instant courant selon qu'il teste
        // un intervalle ou une borne : on compte les marqueurs plutôt que de
        // maintenir deux variantes de la requête.
        for _ in 0..filter.matches('?').count() {
            values.push(Value::Integer(now_utc));
        }
        Ok(self
            .conn
            .query_row(&sql, params_from_iter(values), read_occurrence)
            .optional()?)
    }

    /// De quoi peupler l'écran d'accueil : une tuile par agenda.
    pub fn calendar_summaries(&self, now_utc: i64) -> Result<Vec<CalendarSummary>> {
        let week_end = now_utc + Duration::days(7).num_seconds();
        let lookahead = now_utc + CONFLICT_LOOKAHEAD.num_seconds();

        self.calendars()?
            .into_iter()
            .map(|calendar| {
                let scope = Some(vec![calendar.id.clone()]);
                let upcoming_week = self
                    .occurrences_between(now_utc, week_end, &scope)?
                    .into_iter()
                    .filter(|o| !o.cancelled)
                    .count() as u32;
                let next = self.query_one(
                    "o.all_day = 0 AND o.cancelled = 0 AND o.start_utc > ?",
                    "ORDER BY o.start_utc",
                    now_utc,
                    &scope,
                )?;
                let conflicts = self.conflicts_between(now_utc, lookahead, &scope)?.len() as u32;
                Ok(CalendarSummary {
                    calendar,
                    upcoming_week,
                    next,
                    conflicts,
                })
            })
            .collect()
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

    fn epoch_day_of(&self, utc: i64) -> Result<i64> {
        let dt = DateTime::from_timestamp(utc, 0)
            .ok_or_else(|| TimewrapError::Parse(format!("horodatage hors limites : {utc}")))?;
        let date = dt.with_timezone(&self.tz).date_naive();
        Ok(date
            .signed_duration_since(NaiveDate::from_ymd_opt(1970, 1, 1).unwrap())
            .num_days())
    }

    // --------------------------------------------------------------- réglages

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

    fn require_calendar(&self, calendar_id: &str) -> Result<()> {
        let exists: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM calendars WHERE id = ?1",
            params![calendar_id],
            |row| row.get(0),
        )?;
        if exists == 0 {
            return Err(TimewrapError::CalendarNotFound(calendar_id.to_string()));
        }
        Ok(())
    }
}

/// Clause de portée d'une vue, et les valeurs à lier avec elle.
fn scope_clause(scope: &Scope) -> (String, Vec<Value>) {
    match scope {
        Some(ids) if !ids.is_empty() => {
            let placeholders = ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
            (
                format!("o.calendar_id IN ({placeholders})"),
                ids.iter().map(|id| Value::Text(id.clone())).collect(),
            )
        }
        _ => ("c.visible = 1".to_string(), Vec::new()),
    }
}

fn read_calendar(row: &rusqlite::Row<'_>) -> rusqlite::Result<Calendar> {
    Ok(Calendar {
        id: row.get(0)?,
        name: row.get(1)?,
        kind: CalendarKind::from_str(&row.get::<_, String>(2)?),
        source: row.get(3)?,
        color: row.get::<_, i64>(4)? as u32,
        visible: row.get::<_, i64>(5)? != 0,
        last_sync: row.get(6)?,
        event_count: row.get::<_, i64>(7)? as u32,
        position: row.get::<_, i64>(8)? as i32,
    })
}

fn read_occurrence(row: &rusqlite::Row<'_>) -> rusqlite::Result<Occurrence> {
    Ok(Occurrence {
        id: row.get(0)?,
        calendar_id: row.get(1)?,
        calendar_name: row.get(2)?,
        color: row.get::<_, i64>(3)? as u32,
        uid: row.get(4)?,
        title: row.get(5)?,
        raw_title: row.get(6)?,
        location: row.get(7)?,
        description: row.get(8)?,
        start_utc: row.get(9)?,
        end_utc: row.get(10)?,
        all_day: row.get::<_, i64>(11)? != 0,
        cancelled: row.get::<_, i64>(12)? != 0,
        origin: EventOrigin::from_str(&row.get::<_, String>(13)?),
        category_id: row.get(14)?,
        category_name: row.get(15)?,
        category_label: row.get(16)?,
        hidden: row.get::<_, i64>(17)? != 0,
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

/// Identifiant stable : même agenda, même UID, même début, même identifiant.
/// C'est ce qui permet aux personnalisations de survivre à un réimport de l'ENT.
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

fn parse_tz(name: &str) -> Result<Tz> {
    name.parse()
        .map_err(|_| TimewrapError::UnknownTimezone(name.to_string()))
}
