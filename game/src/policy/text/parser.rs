//! Odczyt postaci tekstowej (M9d WP8).
//!
//! Zejście rekurencyjne po tokenach, bez tablic i bez generatora — gramatyka ma
//! dwadzieścia parę produkcji i nie jest dwuznaczna, więc parser generowany byłby
//! zależnością i plikiem budowania na rzecz czegoś, co czyta się wprost.
//!
//! **Przyjmuje oba języki naraz.** Słownik ([`super::Slownik`]) zbiera słowa z każdego
//! `Locale`, więc polityka przysłana po angielsku wczyta się u gracza grającego
//! po polsku. To nie jest wygoda: postać tekstowa istnieje **po to**, żeby politykami
//! dawało się dzielić, a warunek „w moim języku" unieważniałby połowę tego pomysłu.

use magnat_core::{ActionKind, Entity, GoodId, JobRoleId, Money, NeedCategoryId, PriceBasis, RecipeId};
use magnat_policy::{
    Bp, Cadence, CmpOp, GoodRef, Metric, Policy, PolicyScope, Severity, TagId, Value,
};
use magnat_ui::Catalog;
use std::num::NonZeroU32;

use super::super::slot::{ActionDraft, Base, Clause, Join, RuleDraft, Slot};
use super::{norm, GoodKeys, Slownik, TextError};

/// Czyta politykę z tekstu. Zwraca ją razem z zakresem, bo zakres jest częścią
/// zapisu (`DLA …`), a nie polem `Policy`.
///
/// # Errors
/// [`TextError`] — nieznane słowo, brakujący element gramatyki albo zmieszanie
/// podstaw ceny (`K-7`). Import jest jedyną drogą, którą taka polityka może powstać:
/// formularz jej nie złoży.
pub fn parse(
    txt: &str,
    goods: &GoodKeys,
    c: &Catalog,
) -> Result<(Policy, PolicyScope), TextError> {
    let sl = Slownik::new(c);
    let tok = lex(txt);
    let mut p = P {
        t: &tok,
        i: 0,
        sl: &sl,
        goods,
    };
    p.polityka()
}

// ── lekser ───────────────────────────────────────────────────────────────────────

#[derive(Clone, PartialEq, Eq, Debug)]
enum Tok {
    Word(String),
    Str(String),
    Punct(&'static str),
}

const PUNCT: &[&str] = &["(", ")", ",", "*", "..", "<=", ">=", "!=", "=", "<", ">"];

fn lex(txt: &str) -> Vec<Tok> {
    let mut out = Vec::new();
    let znaki: Vec<char> = txt.chars().collect();
    let mut i = 0usize;
    while i < znaki.len() {
        let ch = znaki[i];
        if ch.is_whitespace() {
            i += 1;
            continue;
        }
        if ch == '"' {
            let mut s = String::new();
            i += 1;
            while i < znaki.len() && znaki[i] != '"' {
                s.push(znaki[i]);
                i += 1;
            }
            i += 1;
            out.push(Tok::Str(s));
            continue;
        }
        // Dwuznaki przed jednoznakami, inaczej „<=" rozpadłoby się na „<" i „=".
        if let Some(p) = PUNCT.iter().find(|p| p.len() > 1 && pasuje(&znaki, i, p)) {
            out.push(Tok::Punct(p));
            i += p.len();
            continue;
        }
        if let Some(p) = PUNCT.iter().find(|p| p.len() == 1 && pasuje(&znaki, i, p)) {
            out.push(Tok::Punct(p));
            i += 1;
            continue;
        }
        let mut s = String::new();
        while i < znaki.len() {
            let ch = znaki[i];
            if ch.is_whitespace()
                || ch == '"'
                || PUNCT.iter().any(|p| pasuje(&znaki, i, p))
            {
                break;
            }
            s.push(ch);
            i += 1;
        }
        out.push(Tok::Word(s));
    }
    out
}

fn pasuje(znaki: &[char], i: usize, wzor: &str) -> bool {
    wzor.chars().enumerate().all(|(k, c)| znaki.get(i + k) == Some(&c))
}

// ── parser ───────────────────────────────────────────────────────────────────────

struct P<'a> {
    t: &'a [Tok],
    i: usize,
    sl: &'a Slownik,
    goods: &'a GoodKeys,
}

impl P<'_> {
    fn blad(&self, what: &'static str) -> TextError {
        TextError::Expected { what, at: self.i }
    }

    fn peek(&self) -> Option<&Tok> {
        self.t.get(self.i)
    }

    fn slowo(&self) -> Option<String> {
        match self.peek() {
            Some(Tok::Word(w)) => Some(norm(w)),
            _ => None,
        }
    }

    /// Zjada słowo kluczowe o tej roli. `false`, gdy stoi co innego.
    fn kw(&mut self, rola: &str) -> bool {
        let Some(w) = self.slowo() else { return false };
        if self.sl.kw.get(&w) == Some(&rola) {
            self.i += 1;
            return true;
        }
        false
    }

    fn wymagaj_kw(&mut self, rola: &'static str) -> Result<(), TextError> {
        if self.kw(rola) {
            Ok(())
        } else {
            Err(self.blad(rola))
        }
    }

    /// Zjada znak przestankowy. Porównanie idzie po **treści**, a nie po wyszukaniu
    /// w tablicy: wersja z `unwrap_or("")` zamieniała literówkę w wywołaniu w ciche
    /// „nigdy nie pasuje", czyli w błąd, który wygląda jak zła gramatyka wejścia.
    fn punkt(&mut self, p: &str) -> bool {
        if matches!(self.peek(), Some(Tok::Punct(x)) if *x == p) {
            self.i += 1;
            return true;
        }
        false
    }

    fn wymagaj_punkt(&mut self, p: &'static str) -> Result<(), TextError> {
        if self.punkt(p) {
            Ok(())
        } else {
            Err(self.blad(p))
        }
    }

    fn napis(&mut self) -> Result<String, TextError> {
        match self.peek() {
            Some(Tok::Str(s)) => {
                let s = s.clone();
                self.i += 1;
                Ok(s)
            }
            _ => Err(self.blad("napis w cudzysłowie")),
        }
    }

    fn surowe(&mut self) -> Result<String, TextError> {
        match self.peek() {
            Some(Tok::Word(w)) => {
                let w = w.clone();
                self.i += 1;
                Ok(w)
            }
            _ => Err(self.blad("słowo")),
        }
    }

    fn liczba(&mut self) -> Result<i64, TextError> {
        let at = self.i;
        let w = self.surowe()?;
        w.parse::<i64>().map_err(|_| TextError::BadNumber { at })
    }

    /// `role#4`, `recipe#7`, `msg#1`, `enum#2` — identyfikator, nie słowo.
    fn odnosnik(&mut self, prefiks: &'static str) -> Result<u32, TextError> {
        let at = self.i;
        let w = self.surowe()?;
        w.strip_prefix(prefiks)
            .and_then(|n| n.strip_prefix('#'))
            .and_then(|n| n.parse::<u32>().ok())
            .ok_or(TextError::BadNumber { at })
    }

}

// ── produkcje gramatyki ──────────────────────────────────────────────────────────
//
// Osobny blok od prymitywów wyżej, bo to jest **inny temat**: tam jest czytanie
// tokenów, tutaj kształt zdania. Podział jest też odpowiedzią na próg strukturalny
// (CLAUDE.md) — jeden blok na wszystko przekraczał pięćset linii i nie dlatego,
// że był długi, tylko dlatego, że były w nim dwie rzeczy.
//
// ponytail: sufit nazwany — ten blok ma ~470 linii i zostaje jednym, bo jest jedną
// gramatyką. Dalszy podział („warunki" vs. „akcje") rozciąłby produkcje, które się
// nawzajem wołają, i zostawiłby trzeci plik z dyspozytorem. Rośnie o ~15 linii na
// nową metrykę i o ~20 na nową akcję; przy drugiej dziedzinie z wykonawcą (kadry,
// produkcja) wraca pytanie, bo wtedy naprawdę będą dwa tematy.

impl P<'_> {
    fn polityka(&mut self) -> Result<(Policy, PolicyScope), TextError> {
        self.wymagaj_kw("policy")?;
        let name = self.napis()?;
        self.wymagaj_kw("domain")?;
        let domain = {
            let at = self.i;
            let w = self.surowe()?;
            *self
                .sl
                .domain
                .get(&norm(&w))
                .ok_or(TextError::Unknown { word: w, at })?
        };
        self.wymagaj_kw("for")?;
        let scope = self.zakres()?;
        self.wymagaj_kw("every")?;
        let cadence = if self.kw("day") {
            Cadence::Daily
        } else if self.kw("hour") {
            Cadence::Hourly
        } else {
            return Err(self.blad("dzień albo godzinę"));
        };
        let cooldown_h = if self.kw("cooldown") {
            u8::try_from(self.liczba()?).unwrap_or(u8::MAX)
        } else {
            0
        };

        let mut rules = Vec::new();
        let mut fallback = None;
        loop {
            if self.kw("otherwise") {
                fallback = Some(self.akcja()?.to_action());
                break;
            }
            let enabled = !self.kw("disabled");
            if !self.kw("when") {
                break;
            }
            let (clauses, join) = self.warunek()?;
            let mut actions = Vec::new();
            if self.kw("then") {
                actions.push(self.akcja()?);
                while self.kw("also") {
                    actions.push(self.akcja()?);
                }
            }
            let note = if self.kw("note") {
                self.napis()?
            } else {
                String::new()
            };
            rules.push(RuleDraft {
                clauses,
                join,
                actions,
                enabled,
                note,
            });
        }

        // Nic nie zostaje za ostatnią regułą. Bez tego śmieci na końcu wchodziły
        // bez słowa, a polityka wyglądała na wczytaną w całości — czyli dokładnie
        // ten rodzaj błędu, którego parser ma nie przepuszczać.
        if self.i < self.t.len() {
            return Err(self.blad("koniec polityki"));
        }
        let p = Policy {
            id: magnat_core::PolicyId(0),
            name,
            domain,
            rules: rules.iter().map(RuleDraft::to_rule).collect(),
            fallback,
            cadence,
            cooldown_h,
        };
        Ok((p, scope))
    }

    fn zakres(&mut self) -> Result<PolicyScope, TextError> {
        let at = self.i;
        let w = self.surowe()?;
        let rola = *self
            .sl
            .scope
            .get(&norm(&w))
            .ok_or(TextError::Unknown { word: w, at })?;
        match rola {
            "firm" => Ok(PolicyScope::Firm(magnat_core::FirmId(self.encja()?))),
            "site" => Ok(PolicyScope::Site(magnat_core::SiteId(self.encja()?))),
            "group" => Ok(PolicyScope::Group(TagId(
                u16::try_from(self.liczba()?).unwrap_or(0),
            ))),
            "product" => {
                let good = self.towar_klucz()?;
                self.wymagaj_kw("in")?;
                Ok(PolicyScope::Product {
                    inner: Box::new(self.zakres()?),
                    good,
                })
            }
            _ => {
                let cat = NeedCategoryId(u16::try_from(self.liczba()?).unwrap_or(0));
                self.wymagaj_kw("in")?;
                Ok(PolicyScope::Category {
                    inner: Box::new(self.zakres()?),
                    cat,
                })
            }
        }
    }

    fn encja(&mut self) -> Result<Entity, TextError> {
        let at = self.i;
        let w = self.surowe()?;
        let (i, g) = w.split_once(':').ok_or(TextError::BadNumber { at })?;
        let index = i.parse::<u32>().map_err(|_| TextError::BadNumber { at })?;
        let gen = g
            .parse::<u32>()
            .ok()
            .and_then(NonZeroU32::new)
            .ok_or(TextError::BadNumber { at })?;
        Ok(Entity::new(index, gen))
    }

    fn towar_klucz(&mut self) -> Result<GoodId, TextError> {
        let k = self.napis()?;
        if let Some(n) = k.strip_prefix("good#") {
            if let Ok(v) = n.parse::<u16>() {
                return Ok(GoodId(v));
            }
        }
        self.goods.id(&k).ok_or(TextError::UnknownGood(k))
    }

    fn towar(&mut self) -> Result<GoodRef, TextError> {
        if self.kw("this_good") {
            return Ok(GoodRef::This);
        }
        Ok(GoodRef::Id(self.towar_klucz()?))
    }

    fn podstawa(&mut self) -> Result<PriceBasis, TextError> {
        if self.kw("gross") {
            Ok(PriceBasis::GrossRetail)
        } else if self.kw("net") {
            Ok(PriceBasis::NetB2B)
        } else {
            Err(self.blad("brutto albo netto"))
        }
    }

    fn warunek(&mut self) -> Result<(Vec<Clause>, Join), TextError> {
        if self.kw("always") {
            return Ok((Vec::new(), Join::And));
        }
        let mut clauses = vec![self.klauzula()?];
        let mut join = Join::And;
        loop {
            if self.kw("and") {
                join = Join::And;
            } else if self.kw("or") {
                join = Join::Or;
            } else {
                break;
            }
            clauses.push(self.klauzula()?);
        }
        Ok((clauses, join))
    }

    fn klauzula(&mut self) -> Result<Clause, TextError> {
        let lhs = self.slot()?;
        let op = self.operator()?;
        // Nazwa sezonu i dnia tygodnia rozpoznaje się **po słowie**, a nie po
        // metryce po lewej: słowniki obu są rozłączne, więc kontekst nic nie
        // dokładał i był kanałem danych bez czytelnika.
        let rhs = self.slot()?;
        // `K-7` w miejscu, w którym jedynie może zadziałać: formularz takiej klauzuli
        // nie złoży, więc tekst jest jedyną drogą, którą ona wchodzi.
        if let (Some(a), Some(b)) = (lhs.basis(), rhs.basis()) {
            if a != b {
                return Err(TextError::PriceBasisMismatch { lhs: a, rhs: b });
            }
        }
        Ok(Clause { lhs, op, rhs })
    }

    fn operator(&mut self) -> Result<CmpOp, TextError> {
        for (p, o) in [
            ("<=", CmpOp::Le),
            (">=", CmpOp::Ge),
            ("!=", CmpOp::Ne),
            ("<", CmpOp::Lt),
            (">", CmpOp::Gt),
            ("=", CmpOp::Eq),
        ] {
            if self.punkt(p) {
                return Ok(o);
            }
        }
        Err(self.blad("operator porównania"))
    }

    fn slot(&mut self) -> Result<Slot, TextError> {
        if self.kw("gross") {
            self.wymagaj_punkt("(")?;
            let s = self.slot_bez_konwersji()?;
            self.wymagaj_punkt(")")?;
            return Ok(s.converted(PriceBasis::GrossRetail));
        }
        if self.kw("net") {
            self.wymagaj_punkt("(")?;
            let s = self.slot_bez_konwersji()?;
            self.wymagaj_punkt(")")?;
            return Ok(s.converted(PriceBasis::NetB2B));
        }
        self.slot_bez_konwersji()
    }

    fn slot_bez_konwersji(&mut self) -> Result<Slot, TextError> {
        let base = self.baza()?;
        let mut s = Slot {
            base,
            scale: None,
            convert: None,
        };
        if self.punkt("*") {
            s.scale = Some(self.procent()?);
        }
        Ok(s)
    }

    fn procent(&mut self) -> Result<Bp, TextError> {
        let at = self.i;
        let w = self.surowe()?;
        let body = w.strip_suffix('%').ok_or(TextError::BadNumber { at })?;
        let (znak, body) = match body.strip_prefix('-') {
            Some(r) => (-1i64, r),
            None => (1i64, body),
        };
        let v = match body.split_once('.') {
            None => body.parse::<i64>().map_err(|_| TextError::BadNumber { at })? * 100,
            Some((a, b)) => {
                let a = a.parse::<i64>().map_err(|_| TextError::BadNumber { at })?;
                let b = format!("{b:0<2}")[..2]
                    .parse::<i64>()
                    .map_err(|_| TextError::BadNumber { at })?;
                a * 100 + b
            }
        };
        Ok(Bp(i32::try_from(znak * v).map_err(|_| TextError::BadNumber { at })?))
    }

    fn baza(&mut self) -> Result<Base, TextError> {
        if let Some(w) = self.slowo() {
            if let Some(nazwa) = self.sl.metric.get(&w).copied() {
                self.i += 1;
                return Ok(Base::Metric(self.metryka(nazwa)?));
            }
            if let Some(s) = self.sl.season.get(&w).copied() {
                self.i += 1;
                return Ok(Base::Lit(Value::Enum(s)));
            }
            if let Some(d) = self.sl.dow.get(&w).copied() {
                self.i += 1;
                return Ok(Base::Lit(Value::Enum(d)));
            }
        }
        self.literal()
    }

    fn literal(&mut self) -> Result<Base, TextError> {
        let at = self.i;
        let w = self.surowe()?;
        if w.ends_with('%') {
            self.i -= 1;
            return Ok(Base::Lit(Value::Bp(self.procent()?)));
        }
        if let Some(n) = w.strip_prefix("enum#") {
            return Ok(Base::Lit(Value::Enum(
                n.parse::<u8>().map_err(|_| TextError::BadNumber { at })?,
            )));
        }
        let v = w.parse::<i64>().map_err(|_| TextError::BadNumber { at })?;
        // Jednostka stoi **za** liczbą i jest opcjonalna: liczba bez jednostki jest
        // liczbą sztuk czegoś nieokreślonego (`Count`), a nie błędem.
        let jednostka = self
            .slowo()
            .and_then(|w| self.sl.unit.get(&w).copied())
            .filter(|u| *u != "metres");
        if jednostka.is_some() {
            self.i += 1;
        }
        Ok(Base::Lit(match jednostka {
            Some("money") => Value::Money(Money(v)),
            Some("qty") => Value::Qty(v),
            Some("days") => Value::Days(i32::try_from(v).unwrap_or(i32::MAX)),
            _ => Value::Count(i32::try_from(v).unwrap_or(i32::MAX)),
        }))
    }

    fn promien(&mut self) -> Result<u32, TextError> {
        let v = self.liczba()?;
        if self.slowo().and_then(|w| self.sl.unit.get(&w).copied()) == Some("metres") {
            self.i += 1;
        }
        Ok(u32::try_from(v).unwrap_or(0))
    }

    fn metryka(&mut self, nazwa: &'static str) -> Result<Metric, TextError> {
        let towarowa = |p: &mut P<'_>| -> Result<GoodRef, TextError> {
            p.wymagaj_punkt("(")?;
            let g = p.towar()?;
            p.wymagaj_punkt(")")?;
            Ok(g)
        };
        Ok(match nazwa {
            "Price" => {
                self.wymagaj_punkt("(")?;
                let good = self.towar()?;
                self.wymagaj_punkt(",")?;
                let basis = self.podstawa()?;
                self.wymagaj_punkt(")")?;
                Metric::Price { good, basis }
            }
            "CheapestCompetitorPrice" | "AvgCompetitorPrice" => {
                self.wymagaj_punkt("(")?;
                let good = self.towar()?;
                self.wymagaj_punkt(",")?;
                let radius_m = self.promien()?;
                self.wymagaj_punkt(",")?;
                let basis = self.podstawa()?;
                self.wymagaj_punkt(")")?;
                if nazwa == "CheapestCompetitorPrice" {
                    Metric::CheapestCompetitorPrice {
                        good,
                        radius_m,
                        basis,
                    }
                } else {
                    Metric::AvgCompetitorPrice {
                        good,
                        radius_m,
                        basis,
                    }
                }
            }
            "CompetitorCount" => {
                self.wymagaj_punkt("(")?;
                let radius_m = self.promien()?;
                self.wymagaj_punkt(")")?;
                Metric::CompetitorCount { radius_m }
            }
            "UnitCost" => Metric::UnitCost(towarowa(self)?),
            "Margin" => Metric::Margin(towarowa(self)?),
            "Stock" => Metric::Stock(towarowa(self)?),
            "StockDays" => Metric::StockDays(towarowa(self)?),
            "Turnover7d" => Metric::Turnover7d(towarowa(self)?),
            "Sales7d" => Metric::Sales7d(towarowa(self)?),
            "DaysToExpiry" => Metric::DaysToExpiry(towarowa(self)?),
            "ShelfGap" => Metric::ShelfGap(towarowa(self)?),
            "DaysSinceLastChange" => Metric::DaysSinceLastChange(towarowa(self)?),
            "OpenPositions" | "MedianMarketWage" => {
                self.wymagaj_punkt("(")?;
                let r = JobRoleId(u16::try_from(self.odnosnik("role")?).unwrap_or(0));
                self.wymagaj_punkt(")")?;
                if nazwa == "OpenPositions" {
                    Metric::OpenPositions(r)
                } else {
                    Metric::MedianMarketWage(r)
                }
            }
            "MachineUtilization" => Metric::MachineUtilization,
            "StaffTurnover12m" => Metric::StaffTurnover12m,
            "StaffMood" => Metric::StaffMood,
            "ManagerSkill" => Metric::ManagerSkill,
            "CashBalance" => Metric::CashBalance,
            "Receivables" => Metric::Receivables,
            "Season" => Metric::Season,
            "DayOfWeek" => Metric::DayOfWeek,
            "DayOfMonth" => Metric::DayOfMonth,
            _ => Metric::HourOfDay,
        })
    }

    fn akcja(&mut self) -> Result<ActionDraft, TextError> {
        let at = self.i;
        let w = self.surowe()?;
        let kind = *self
            .sl
            .act
            .get(&norm(&w))
            .ok_or(TextError::Unknown { word: w, at })?;
        let mut d = ActionDraft {
            kind,
            ..ActionDraft::default()
        };
        match kind {
            ActionKind::SetPrice | ActionKind::AdjustPrice => {
                d.good = self.towar()?;
                self.wymagaj_punkt("=")?;
                d.a = self.slot()?;
            }
            ActionKind::SetMargin | ActionKind::Markdown => {
                d.good = self.towar()?;
                self.wymagaj_punkt("=")?;
                d.bp = self.procent()?;
            }
            ActionKind::ClampPrice => {
                d.good = self.towar()?;
                self.wymagaj_kw("to")?;
                d.a = self.slot()?;
                self.wymagaj_punkt("..")?;
                d.b = self.slot()?;
                d.has_b = true;
            }
            ActionKind::OrderUpTo => {
                d.good = self.towar()?;
                d.a = self.slot()?;
            }
            ActionKind::OrderQty => {
                d.good = self.towar()?;
                d.a = self.slot()?;
                let at = self.i;
                let w = self.surowe()?;
                d.source = *self
                    .sl
                    .src
                    .get(&norm(&w))
                    .ok_or(TextError::Unknown { word: w, at })?;
            }
            ActionKind::RemoveFromShelf => d.good = self.towar()?,
            ActionKind::Hire => {
                d.count = u16::try_from(self.liczba()?).unwrap_or(1);
                d.role = JobRoleId(u16::try_from(self.odnosnik("role")?).unwrap_or(0));
                self.wymagaj_kw("at")?;
                d.a = self.slot()?;
            }
            ActionKind::RaiseWage => {
                d.role = JobRoleId(u16::try_from(self.odnosnik("role")?).unwrap_or(0));
                self.wymagaj_punkt("=")?;
                d.bp = self.procent()?;
                if self.kw("to") {
                    d.b = self.slot()?;
                    d.has_b = true;
                }
            }
            ActionKind::PlanProduction => {
                d.recipe = RecipeId(u16::try_from(self.odnosnik("recipe")?).unwrap_or(0));
                d.a = self.slot()?;
            }
            ActionKind::Alert => {
                d.msg = u16::try_from(self.odnosnik("msg")?).unwrap_or(0);
                let at = self.i;
                let w = self.surowe()?;
                d.severity = *self
                    .sl
                    .sev
                    .get(&norm(&w))
                    .ok_or(TextError::Unknown { word: w, at })?;
            }
            ActionKind::AskPlayer => {
                d.msg = u16::try_from(self.odnosnik("msg")?).unwrap_or(0);
                d.severity = Severity::Warning;
            }
        }
        Ok(d)
    }
}
