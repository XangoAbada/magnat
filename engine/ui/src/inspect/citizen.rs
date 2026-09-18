//! Karta inspekcji mieszkańca (M3d §5.11, PRD §14.4).
//!
//! Kolejność sekcji jest z §5.11 i nie jest dowolna: nagłówek → potrzeby → majątek →
//! oś czasu → relacje → decyzje. Karta **nie liczy niczego sama** — bierze migawkę
//! mieszkańca, odtwarza plan z uzasadnieniami i pyta `sim/agents` o status i deprywacje.
//! Dzięki temu to, co widzi gracz, jest tym, co wykonała symulacja, a nie drugim
//! rachunkiem obok niej (00 §7).

use crate::inspect::reason;
use crate::inspect::timeline::{CitizenHeader, DayTimeline};
use crate::loc::{Catalog, Locale};
use magnat_agents::{CitizenSnapshot, NeedTable, SocialClass, StatusBreakdown, StatusWeights};
use magnat_core::{Money, NeedKind, Q};

/// Wiersz paska potrzeby.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct NeedRow {
    pub need: NeedKind,
    pub label: String,
    pub level: u8,
    /// Czy poziom zszedł poniżej progu krytycznego z `data/needs/needs.ron`.
    pub critical: bool,
    /// Tempo spadku i pierwszy skutek deprywacji — treść tooltipa (§5.11 pkt 2).
    pub tooltip: String,
}

/// Wiersz rozbicia statusu — siedem składników **przed** przemnożeniem przez wagę
/// (korekta E-18). Liczba bez rozbicia nie wyjaśnia niczego (00 §7).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct StatusRow {
    pub label: String,
    pub value: u8,
    pub weight: u16,
}

/// Wszystko, co karta pokazuje — policzone raz, bez dostępu do świata w trakcie
/// rysowania. Ten sam model karmi widget graficzny i wydruk tekstowy.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct CitizenCard {
    pub header: CitizenHeader,
    pub needs: Vec<NeedRow>,
    pub cash: Money,
    pub household_budget: Money,
    pub income_monthly: Money,
    pub status: u8,
    pub social_class: String,
    pub status_rows: Vec<StatusRow>,
    pub household_kind: String,
    pub household_size: u8,
}

impl CitizenCard {
    /// Buduje kartę z migawki mieszkańca.
    ///
    /// `header` przychodzi z zewnątrz, bo nazwy własne pochodzą z `data/names/`
    /// i nie są lokalizacją UI, a adres zna tylko ten, kto widzi miasto.
    #[must_use]
    pub fn build(
        c: &Catalog,
        l: Locale,
        snap: &CitizenSnapshot,
        table: &NeedTable,
        header: CitizenHeader,
        household: Option<&magnat_agents::Household>,
        status: Option<(&StatusBreakdown, StatusWeights)>,
    ) -> CitizenCard {
        let mut needs = Vec::with_capacity(NeedKind::ALL.len());
        let mut deprywacje = Vec::new();
        magnat_agents::deprivation_of(&snap.needs, table, &mut deprywacje);
        for n in NeedKind::ALL {
            let spec = table.spec(*n);
            let poziom = snap.needs.get(*n).get();
            let skutek = deprywacje
                .iter()
                .find_map(|r| match r {
                    magnat_core::DecisionReason::Deprivation { need, .. } if need == n => {
                        Some(reason::describe(c, l, *r))
                    }
                    _ => None,
                })
                .unwrap_or_default();
            let tempo = c.fmt_key(
                l,
                "ui.card.decay",
                &[(
                    "tempo",
                    &format!("{:.1}", f64::from(spec.decay_centi_per_hour) / 100.0),
                )],
            );
            needs.push(NeedRow {
                need: *n,
                label: reason::need(c, l, *n),
                level: poziom,
                critical: poziom < spec.critical,
                tooltip: if skutek.is_empty() {
                    tempo
                } else {
                    format!("{tempo} · {skutek}")
                },
            });
        }

        let (budzet, dochod, typ, rozmiar) =
            household.map_or((Money::ZERO, Money::ZERO, String::new(), 0), |h| {
                (
                    Money(h.cash.get() + h.bank.get() + h.savings.get()),
                    h.income_monthly,
                    reason::household_kind(c, l, magnat_agents::HouseholdKind::from_u8(h.kind)),
                    h.size,
                )
            });

        let status_rows = status.map_or_else(Vec::new, |(b, w)| {
            [
                ("ui.card.wealth.income", b.income, w.income),
                ("ui.card.wealth", b.wealth, w.wealth),
                ("ui.need.Development", b.education, w.education),
                ("ui.commitment.Work", b.occupation, w.occupation),
                ("ui.card.address", b.address, w.address),
                ("ui.need.Status", b.consumption, w.consumption),
                ("ui.card.household", b.family, w.family),
            ]
            .iter()
            .map(|(k, v, w)| StatusRow {
                label: c.fmt_key(
                    l,
                    k,
                    &[("budynek", "?"), ("lokal", "?"), ("dzielnica", "?")],
                ),
                value: *v,
                weight: *w,
            })
            .collect()
        });

        CitizenCard {
            header,
            needs,
            cash: snap.wealth.cash,
            household_budget: budzet,
            income_monthly: dochod,
            status: snap.vitals.status,
            social_class: reason::social_class(c, l, SocialClass::of(Q::new(snap.vitals.status))),
            status_rows,
            household_kind: typ,
            household_size: rozmiar,
        }
    }

    /// Karta w formie tekstowej — ta sama treść co widget, do konsoli deweloperskiej
    /// i do testów akceptacyjnych bez GPU.
    #[must_use]
    pub fn render_text(&self, c: &Catalog, l: Locale, timeline: &DayTimeline<'_>) -> String {
        use std::fmt::Write;
        let mut s = String::new();
        let _ = writeln!(
            s,
            "{}",
            crate::inspect::timeline::render_day_text(c, l, &self.header, timeline)
        );

        let _ = writeln!(s, "{}:", c.fmt_key(l, "ui.card.needs", &[]));
        for n in &self.needs {
            let _ = writeln!(
                s,
                "  {:<14} {:>3}/100{}  — {}",
                n.label,
                n.level,
                if n.critical { " !" } else { "  " },
                n.tooltip
            );
        }

        let _ = writeln!(s, "{}:", c.fmt_key(l, "ui.card.wealth", &[]));
        let _ = writeln!(
            s,
            "  {}: {}",
            c.fmt_key(l, "ui.card.wealth.cash", &[]),
            zlotowki(self.cash)
        );
        let _ = writeln!(
            s,
            "  {}: {}",
            c.fmt_key(l, "ui.card.wealth.household", &[]),
            zlotowki(self.household_budget)
        );
        let _ = writeln!(
            s,
            "  {}: {}",
            c.fmt_key(l, "ui.card.wealth.income", &[]),
            zlotowki(self.income_monthly)
        );

        let _ = writeln!(
            s,
            "{}: {} ({}/100)",
            c.fmt_key(l, "ui.card.status", &[]),
            self.social_class,
            self.status
        );
        for r in &self.status_rows {
            let _ = writeln!(s, "  {:<22} {:>3}/100 × {} %", r.label, r.value, r.weight);
        }
        s
    }
}

/// Kwota w groszach jako złotówki. Formatowanie zależne od języka (separator tysięcy,
/// symbol waluty) dokłada M12 razem z pełną lokalizacją formatów — tu liczy się to,
/// żeby grosz nie zniknął w zaokrągleniu.
#[must_use]
pub fn zlotowki(m: Money) -> String {
    let g = m.get();
    let znak = if g < 0 { "-" } else { "" };
    let a = g.unsigned_abs();
    format!("{znak}{}.{:02}", a / 100, a % 100)
}

/// Panel karty mieszkańca — implementacja [`InspectorPanel`](crate::InspectorPanel).
///
/// Panel **buduje model**, a nie rysuje: klient graficzny czyta to samo, co wydruk
/// tekstowy, więc złoty test w CI broni tego, co widzi gracz. Plan odtwarza się
/// z ziarna przez `plan_day_explained` (0,84 µs — taniej niż go przechowywać),
/// realizacja pochodzi z bufora śledzenia (decyzja 9.16).
pub struct CitizenPanel {
    /// Doba, dla której odtwarzamy plan.
    pub day: u64,
    pub seed: u64,
}

/// Karta razem z tym, na czym stoi oś dnia.
///
/// Plan i dziennik uzasadnień są **odtwarzane na żądanie** (§5.4), więc muszą przeżyć
/// tak długo, jak długo ktoś patrzy na `DayTimeline` — stąd własność, a nie referencja.
/// Ten sam model karmi wydruk tekstowy i widget `egui`: jedno źródło prawdy, o które
/// prosi korekta E-8.
pub struct CitizenModel {
    pub card: CitizenCard,
    pub canvas: magnat_agents::DayCanvas,
    pub log: magnat_agents::ReasonLog,
    pub actual: Vec<crate::ActualBlock>,
    /// Plan **zapisany** w slabie — ten, w który indeksuje bufor śledzenia (`N-9`).
    /// Pusty, gdy mieszkaniec nie ma aktualnego planu na tę dobę.
    pub stored: Vec<magnat_agents::PlanSlot>,
}

impl CitizenModel {
    #[must_use]
    pub fn timeline(&self) -> DayTimeline<'_> {
        DayTimeline {
            canvas: &self.canvas,
            log: &self.log,
            actual: &self.actual,
            stored: &self.stored,
        }
    }
}

impl CitizenPanel {
    /// Zbiera wszystko, co karta pokazuje. `None` = nie ma kogo pokazać: brak zaznaczenia,
    /// encja znikła (zgon, wyprowadzka) albo świat nie ma jeszcze wpiętych źródeł.
    #[must_use]
    pub fn model(&self, ui: &crate::UiContext, world: &magnat_ecs::World) -> Option<CitizenModel> {
        let (c, l) = (&ui.catalog, ui.locale);
        let citizen = ui.selection.citizen()?;
        let snap = CitizenSnapshot::of(world, citizen.entity(), self.day)?;
        let z = world.resource::<magnat_agents::AgentSources>().get()?;
        let table = world.resource::<NeedTable>();

        let mut canvas = magnat_agents::DayCanvas::new();
        let mut log = magnat_agents::ReasonLog::new();
        let ctx = snap.ctx(
            self.seed,
            self.day,
            table,
            z.places.as_ref(),
            z.travel.as_ref(),
        );
        magnat_agents::plan_day_explained(&ctx, &mut canvas, &mut log);

        let actual = crate::actual_from_trace(
            &world
                .resource::<magnat_agents::Trace>()
                .entries(citizen.entity().index()),
        );
        // Plan zapisany, a nie odtworzony, jest tym, którego numery slotów niesie
        // bufor śledzenia (`N-9`). Uchwyt starszy niż ta doba opisuje wczorajszy
        // plan i nie ma prawa udawać dzisiejszego.
        let stored = world
            .get::<magnat_agents::PlanRef>(citizen.entity())
            .filter(|p| p.plan_day == (self.day % 65_536) as u16)
            .map(|p| {
                magnat_agents::load_plan(p, world.resource::<magnat_agents::PlanSlab>()).to_vec()
            })
            .unwrap_or_default();
        let card = CitizenCard::build(
            c,
            l,
            &snap,
            table,
            naglowek(c, l, &snap, self.day),
            world.get::<magnat_agents::Household>(snap.household),
            None,
        );
        Some(CitizenModel {
            card,
            canvas,
            log,
            actual,
            stored,
        })
    }
}

impl crate::InspectorPanel for CitizenPanel {
    fn title(&self, ui: &crate::UiContext) -> String {
        ui.text("ui.card.title")
    }

    fn build(&mut self, ui: &crate::UiContext, world: &magnat_ecs::World) -> crate::Rich {
        let Some(m) = self.model(ui, world) else {
            return crate::rich::lines(&ui.catalog.fmt_key(ui.locale, "ui.card.no_selection", &[]));
        };
        // Tekst powstaje jak dotąd, a na kawałki tnie go jedna funkcja — dzięki temu
        // złoty wydruk jest bajt w bajt ten sam, a `M9c` ma gdzie przypiąć odnośniki.
        crate::rich::lines(&m.card.render_text(&ui.catalog, ui.locale, &m.timeline()))
    }
}

/// Nagłówek karty. Imiona pochodzą z `data/names/` i **nie są** lokalizacją UI
/// (CLAUDE.md) — ta sama nazwa pada w obu wersjach językowych, bo mieszkaniec
/// nazwiskiem Schmidt nazywa się tak samo po polsku i po angielsku.
fn naglowek(c: &Catalog, l: Locale, snap: &CitizenSnapshot, day: u64) -> CitizenHeader {
    use magnat_agents::Employment;
    let zawod = if snap.employment.flags & Employment::FLAG_PUPIL != 0 {
        c.fmt_key(l, "ui.card.pupil", &[])
    } else if snap.employment.has_job() {
        c.fmt_key(l, "ui.commitment.Work", &[])
    } else if snap.employment.flags & Employment::FLAG_RETIRED != 0 {
        c.fmt_key(l, "ui.card.retired", &[])
    } else {
        c.fmt_key(l, "ui.card.unemployed", &[])
    };
    let adres = if snap.residence.building == magnat_agents::Residence::HOMELESS {
        c.fmt_key(l, "ui.card.homeless", &[])
    } else {
        c.fmt_key(
            l,
            "ui.card.address",
            &[
                ("budynek", &snap.residence.building.to_string()),
                ("lokal", &snap.residence.unit.to_string()),
                ("dzielnica", &snap.residence.district.to_string()),
            ],
        )
    };
    CitizenHeader {
        name: crate::full_name(&snap.identity),
        age_years: snap.identity.age_years(day as i32).max(0) as u32,
        occupation: zawod,
        address: adres,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grosze_nie_znikaja_w_zaokragleniu() {
        assert_eq!(zlotowki(Money(1)), "0.01");
        assert_eq!(zlotowki(Money(-1)), "-0.01");
        assert_eq!(zlotowki(Money(123_456)), "1234.56");
        assert_eq!(zlotowki(Money::ZERO), "0.00");
    }
}
