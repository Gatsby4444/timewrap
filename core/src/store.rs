//! Stockage local et requêtes d'agenda.
//!
//! Le parti pris structurant : les récurrences sont développées à l'import et
//! écrites en dur dans `occurrences`. Afficher une journée ou une semaine
//! devient alors un simple balayage d'index, et « quel est mon prochain cours »
//! une requête à une ligne. Le flux `.ics` d'origine est conservé pour pouvoir
//! re-développer plus loin dans le temps sans redemander le fichier.

use chrono::{DateTime, Duration, NaiveDate, TimeZone, Utc};
use chrono_tz::Tz;
use rusqlite::{Connection, OptionalExtension, params};

use crate::error::{Result, TimewrapError};
use crate::ics;
use crate::model::{Calendar, CalendarKind, DayAgenda, ImportReport, NowView, Occurrence};

/// Combien de passé on garde développé.
const HORIZON_PAST: Duration = Duration::days(90);
/// Combien d'avenir on développe d'avance.
const HORIZON_FUTURE: Duration = Duration::days(400);
/// En deçà de cette marge restante, on re-développe la fenêtre.
const HORIZON_MARGIN: Duration = Duration::days(60);

/// Couleurs attribuées aux agendas dans l'ordre de création.
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

        Ok(())
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
                    (SELECT COUNT(DISTINCT o.uid) FROM occurrences o WHERE o.calendar_id = c.id)
             FROM calendars c
             ORDER BY c.created_at",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(Calendar {
                id: row.get(0)?,
                name: row.get(1)?,
                kind: CalendarKind::from_str(&row.get::<_, String>(2)?),
                source: row.get(3)?,
                color: row.get::<_, i64>(4)? as u32,
                visible: row.get::<_, i64>(5)? != 0,
                last_sync: row.get(6)?,
                event_count: row.get::<_, i64>(7)? as u32,
            })
        })?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
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
        let id = new_id(now_utc, name);
        let existing: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM calendars", [], |row| row.get(0))?;
        let color = PALETTE[(existing as usize) % PALETTE.len()];

        self.conn.execute(
            "INSERT INTO calendars (id, name, kind, source, color, visible, created_at, raw_ics)
             VALUES (?1, ?2, ?3, ?4, ?5, 1, ?6, ?7)",
            params![
                id,
                name,
                kind.as_str(),
                source,
                color as i64,
                now_utc,
                ics_text
            ],
        )?;

        self.rebuild(&id, ics_text, now_utc)
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

    /// Développe le flux et réécrit les occurrences de cet agenda.
    ///
    /// Le remplacement est intégral, donc idempotent : réimporter deux fois le
    /// même fichier laisse exactement le même état.
    fn rebuild(&self, calendar_id: &str, ics_text: &str, now_utc: i64) -> Result<ImportReport> {
        let (from, to) = horizon(now_utc);
        let expansion = ics::expand(ics_text, from, to, self.tz)?;

        self.conn.execute(
            "DELETE FROM occurrences WHERE calendar_id = ?1",
            params![calendar_id],
        )?;

        {
            let mut stmt = self.conn.prepare(
                "INSERT OR REPLACE INTO occurrences
                 (id, calendar_id, uid, summary, location, description,
                  start_utc, end_utc, all_day, cancelled)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            )?;
            for occurrence in &expansion.occurrences {
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
                ])?;
            }
        }

        self.conn.execute(
            "UPDATE calendars SET last_sync = ?2 WHERE id = ?1",
            params![calendar_id, now_utc],
        )?;
        self.set_setting("horizon_from", &from.to_string())?;
        self.set_setting("horizon_to", &to.to_string())?;

        Ok(ImportReport {
            calendar_id: calendar_id.to_string(),
            events: expansion.events,
            occurrences: expansion.occurrences.len() as u32,
            skipped: expansion.skipped,
            first_start_utc: expansion.occurrences.first().map(|o| o.start_utc),
            last_start_utc: expansion.occurrences.last().map(|o| o.start_utc),
        })
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

    pub fn delete_calendar(&self, calendar_id: &str) -> Result<()> {
        self.require_calendar(calendar_id)?;
        self.conn
            .execute("DELETE FROM calendars WHERE id = ?1", params![calendar_id])?;
        Ok(())
    }

    // --------------------------------------------------------------- requêtes

    /// Occurrences chevauchant `[from_utc, to_utc)`, agendas masqués exclus.
    pub fn occurrences_between(&self, from_utc: i64, to_utc: i64) -> Result<Vec<Occurrence>> {
        let mut stmt = self.conn.prepare(
            "SELECT o.id, o.calendar_id, c.name, c.color, o.uid, o.summary, o.location,
                    o.description, o.start_utc, o.end_utc, o.all_day, o.cancelled
             FROM occurrences o
             JOIN calendars c ON c.id = o.calendar_id
             WHERE c.visible = 1 AND o.end_utc > ?1 AND o.start_utc < ?2
             ORDER BY o.all_day DESC, o.start_utc, o.summary",
        )?;
        let rows = stmt.query_map(params![from_utc, to_utc], read_occurrence)?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    /// Contenu d'une journée, `epoch_day` étant compté comme `LocalDate.toEpochDay()`.
    pub fn day(&self, epoch_day: i64) -> Result<DayAgenda> {
        let (from, to) = self.day_bounds(epoch_day)?;
        let all_day_from = epoch_day * 86_400;
        let all_day_to = all_day_from + 86_400;

        let mut stmt = self.conn.prepare(
            "SELECT o.id, o.calendar_id, c.name, c.color, o.uid, o.summary, o.location,
                    o.description, o.start_utc, o.end_utc, o.all_day, o.cancelled
             FROM occurrences o
             JOIN calendars c ON c.id = o.calendar_id
             WHERE c.visible = 1 AND (
                 (o.all_day = 0 AND o.end_utc > ?1 AND o.start_utc < ?2)
                 OR (o.all_day = 1 AND o.start_utc >= ?3 AND o.start_utc < ?4)
             )
             ORDER BY o.all_day DESC, o.start_utc, o.summary",
        )?;
        let rows = stmt.query_map(params![from, to, all_day_from, all_day_to], read_occurrence)?;
        Ok(DayAgenda {
            epoch_day,
            occurrences: rows.collect::<std::result::Result<Vec<_>, _>>()?,
        })
    }

    /// `days` journées consécutives à partir de `epoch_day`.
    pub fn days(&self, epoch_day: i64, days: u32) -> Result<Vec<DayAgenda>> {
        (0..days as i64)
            .map(|offset| self.day(epoch_day + offset))
            .collect()
    }

    /// Ce qui se passe maintenant, et ce qui vient après.
    pub fn now_view(&self, now_utc: i64) -> Result<NowView> {
        let current = self.query_one(
            "WHERE c.visible = 1 AND o.all_day = 0 AND o.cancelled = 0
                   AND o.start_utc <= ?1 AND o.end_utc > ?1
             ORDER BY o.start_utc DESC",
            now_utc,
        )?;

        let next = self.query_one(
            "WHERE c.visible = 1 AND o.all_day = 0 AND o.cancelled = 0
                   AND o.start_utc > ?1
             ORDER BY o.start_utc",
            now_utc,
        )?;

        let today = self.epoch_day_of(now_utc)?;
        let rest_of_day = self
            .day(today)?
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

    fn query_one(&self, tail: &str, now_utc: i64) -> Result<Option<Occurrence>> {
        let sql = format!(
            "SELECT o.id, o.calendar_id, c.name, c.color, o.uid, o.summary, o.location,
                    o.description, o.start_utc, o.end_utc, o.all_day, o.cancelled
             FROM occurrences o
             JOIN calendars c ON c.id = o.calendar_id
             {tail}
             LIMIT 1"
        );
        Ok(self
            .conn
            .query_row(&sql, params![now_utc], read_occurrence)
            .optional()?)
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

fn read_occurrence(row: &rusqlite::Row<'_>) -> rusqlite::Result<Occurrence> {
    Ok(Occurrence {
        id: row.get(0)?,
        calendar_id: row.get(1)?,
        calendar_name: row.get(2)?,
        color: row.get::<_, i64>(3)? as u32,
        uid: row.get(4)?,
        title: row.get(5)?,
        location: row.get(6)?,
        description: row.get(7)?,
        start_utc: row.get(8)?,
        end_utc: row.get(9)?,
        all_day: row.get::<_, i64>(10)? != 0,
        cancelled: row.get::<_, i64>(11)? != 0,
    })
}

/// Identifiant stable : même agenda, même UID, même début, même identifiant.
/// C'est ce qui permettra aux notes et personnalisations de survivre à un
/// réimport de l'ENT.
fn occurrence_id(calendar_id: &str, uid: &str, start_utc: i64) -> String {
    format!("{calendar_id}|{uid}|{start_utc}")
}

fn new_id(now_utc: i64, name: &str) -> String {
    let digest: u64 = name.bytes().fold(0xcbf2_9ce4_8422_2325u64, |acc, b| {
        (acc ^ u64::from(b)).wrapping_mul(0x0000_0100_0000_01b3)
    });
    format!("cal_{now_utc:x}_{digest:x}")
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
