//! Giełda: notowania, arkusz zleceń, akcjonariat, kalendarz wyników.
//!
//! # Kurs jest ceną jednego punktu bazowego
//!
//! Akcja nie dostała drugiej tablicy (`K-85`, `GD-1`): giełda obraca punktami
//! bazowymi z `Firm.owners`, których suma wynosi dokładnie 10 000. Kurs jest więc
//! ceną **jednej dziesięciotysięcznej firmy** i sama ta liczba jest dla gracza
//! nieczytelna — panel pokazuje obok niej wycenę całej spółki, **tekstem**, a nie
//! drugim przelicznikiem (`FF-16`). Drugi przelicznik byłby dokładnie tą drugą
//! prawdą, którą `GD-1` usunął.
//!
//! # Skąd historia kursu
//!
//! `Listing` trzyma ostatni i poprzedni fixing — i tyle ma trzymać, bo pierścień
//! notowań wchodziłby do hasha stanu i do zapisu gry za wykres. Wykres rysuje się
//! z `MetricId::StockPrice`, czyli ze strony widoku, tak samo jak wykres gotówki.

use magnat_core::{Money, SiteId, Subject};
use magnat_firms::{FirmKey, Owner};
use magnat_ui::DataSource;

use super::widgets::{action, picker, row, section, Sev};
use super::{PanelAction, PanelCtx, PanelDesc, PanelId, PanelModel, PanelView};
use crate::PlayerCommand;

pub(super) const DESC: PanelDesc = PanelDesc {
    id: PanelId::Stock,
    title: "ui.panel.stock",
    deps: &[DataSource::Clock, DataSource::World, DataSource::Locale],
    build,
    render,
    min_tier: Some(crate::CareerTier::Company),
};

/// Ile zleceń arkusza pokazać po każdej stronie.
const ZLECEN: usize = 6;

/// Ile dób trzyma zlecenie gracza, zanim wygaśnie. Fixing jest dobowy, więc tydzień
/// to siedem prób — a zlecenie bez terminu wisiałoby w arkuszu na zawsze.
const WAZNOSC_DNI: u16 = 7;

/// Ile punktów bazowych obejmuje jedno kliknięcie. Pół procent firmy: dość, żeby
/// ruszyć kurs przy realnej płynności, za mało, żeby przypadkiem przekroczyć próg
/// ujawnienia (500 bp).
const KROK_BP: u16 = 50;

/// Jedna spółka na liście.
pub struct Spolka {
    pub firm: u64,
    pub name: String,
    pub last: Money,
    pub prev: Money,
    pub float_bp: u16,
}

/// Wiersz arkusza zleceń.
pub struct Zlecenie {
    pub sell: bool,
    pub limit: Money,
    pub bp: u16,
    pub moje: bool,
}

pub struct Model {
    pub spolki: Vec<Spolka>,
    /// Arkusz wybranej spółki, kupno i sprzedaż razem.
    pub arkusz: Vec<Zlecenie>,
    /// Akcjonariat: kto i ile punktów bazowych — tylko powyżej progu ujawnienia.
    pub akcjonariat: Vec<(Option<Subject>, u16, bool)>,
    /// Udział gracza w wybranej spółce.
    pub moj_udzial: u16,
    /// Ostatnio opublikowany miesiąc i zysk dwunastu miesięcy.
    pub wyniki: Option<(u32, Money)>,
    /// Ile dób do najbliższej publikacji.
    pub do_publikacji: u32,
    /// Gotówka gospodarstwa gracza — sufit zlecenia kupna.
    pub gotowka: Money,
    /// Zakład gracza, przez który idzie komenda debiutu. `None` = gracz nie
    /// prowadzi firmy albo jego spółka jest już notowana.
    pub moj_zaklad: Option<SiteId>,
}

fn build(ctx: &PanelCtx<'_>) -> PanelModel {
    let s = ctx.session;
    let world = &s.app.world;
    let pusty = || {
        PanelModel::Stock(Model {
            spolki: Vec::new(),
            arkusz: Vec::new(),
            akcjonariat: Vec::new(),
            moj_udzial: 0,
            wyniki: None,
            do_publikacji: 0,
            gotowka: Money::ZERO,
            moj_zaklad: ctx.holdings.sites.first().copied(),
        })
    };
    let (Some(eq), Some(firms)) = (
        world.get_resource::<magnat_economy::equity::Equity>(),
        world.get_resource::<magnat_firms::Firms>(),
    ) else {
        return pusty();
    };
    let spolki: Vec<Spolka> = eq
        .listings()
        .map(|l| Spolka {
            firm: l.firm.0,
            name: ctx.fmt("ui.subject.firm", &[("nr", &l.firm.0.to_string())]),
            last: l.last_fixing,
            prev: l.prev_fixing,
            float_bp: l.float_bp,
        })
        .collect();
    if spolki.is_empty() {
        return pusty();
    }
    let wybrana = spolki[ctx.sel.min(spolki.len() - 1)].firm;
    let key = FirmKey(wybrana);

    let mut arkusz: Vec<Zlecenie> = eq
        .orders(key)
        .iter()
        .map(|o| Zlecenie {
            sell: o.side == magnat_economy::equity::book::Side::Sell,
            limit: o.limit,
            bp: o.bp,
            moje: o.holder == Owner::Player,
        })
        .collect();
    // Najpierw kupno od najwyższej ceny, potem sprzedaż od najniższej — tak czyta
    // się arkusz i tak samo go zestawia fixing.
    arkusz.sort_unstable_by(|a, b| {
        a.sell
            .cmp(&b.sell)
            .then(if a.sell {
                a.limit.get().cmp(&b.limit.get())
            } else {
                b.limit.get().cmp(&a.limit.get())
            })
            .then(b.bp.cmp(&a.bp))
    });
    arkusz.truncate(ZLECEN * 2);

    let (akcjonariat, moj_udzial) = firms.get(key).map_or((Vec::new(), 0), |f| {
        let mut v: Vec<(Option<Subject>, u16, bool)> = f
            .owners
            .iter()
            .filter(|o| u32::from(o.bp) >= magnat_economy::equity::DISCLOSURE_BP)
            .map(|o| {
                (
                    magnat_economy::equity::cap::owner_subject(o.owner),
                    o.bp,
                    u32::from(o.bp) >= magnat_economy::equity::CONTROL_BP,
                )
            })
            .collect();
        v.sort_unstable_by_key(|x| std::cmp::Reverse(x.1));
        (v, magnat_economy::equity::cap::stake_of(f, Owner::Player))
    });

    let doba = u32::try_from(s.tick().get() / magnat_core::time::MINUTES_PER_DAY).unwrap_or(0);
    let wyniki = eq.published(key).map(|p| (p.month, p.profit_12m));
    // Ile dób do najbliższej publikacji. Miesiąc `m` kończy się w dobie `(m+1)×30`,
    // a raport wychodzi `PUBLISH_LAG_DAYS` później (`K-1`: rok ma 360 dób), więc
    // publikacje wypadają w dobach przystających do `45 mod 30`. Zero znaczy
    // „dzisiaj", a nie „nigdy".
    let rytm = magnat_economy::equity::value::PUBLISH_LAG_DAYS % 30;
    let do_publikacji = (rytm + 30 - doba % 30) % 30;
    let gotowka = magnat_economy::equity::pay::player_citizen(world)
        .and_then(|c| magnat_agents::citizen_by_index(world, c.entity().index()))
        .and_then(|_| {
            let c = magnat_economy::equity::pay::player_citizen(world)?;
            let hh = magnat_economy::equity::pay::gospodarstwo(world, c)?;
            world
                .get::<magnat_agents::Household>(hh)
                .map(|h| Money(h.cash.get() + h.bank.get()))
        })
        .unwrap_or(Money::ZERO);

    PanelModel::Stock(Model {
        spolki,
        arkusz,
        akcjonariat,
        moj_udzial,
        wyniki,
        do_publikacji,
        gotowka,
        moj_zaklad: ctx.holdings.sites.first().copied(),
    })
}

/// `ponytail:` jedna funkcja na cztery sekcje (kurs, arkusz, akcjonariat, zlecenie).
/// Sufit: przekroczy 150 linii przy piątej. Ścieżka wyjścia jest mechaniczna — każda
/// sekcja jest osobnym blokiem i wychodzi do własnej funkcji bez zmiany kolejności.
fn render(
    ui: &mut egui::Ui,
    ctx: &PanelCtx<'_>,
    model: &PanelModel,
    view: &mut PanelView,
) -> PanelAction {
    let PanelModel::Stock(m) = model else {
        return PanelAction::None;
    };
    let th = ctx.theme;
    let mut akcja = PanelAction::None;
    // Debiut stoi **przed** listą spółek, bo to jedyna rzecz, którą da się zrobić
    // w mieście, w którym nikt jeszcze nie jest notowany — a takie jest każde
    // miasto w pierwszym kwartale gry.
    if let Some(site) = m.moj_zaklad {
        let cmd = PlayerCommand::GoPublic { site };
        let blokada = crate::precheck(&ctx.session.view(), &cmd)
            .err()
            .map(|e| e.text(ctx.c, ctx.l));
        if action(ui, ctx, &ctx.text("ui.stock.go_public"), blokada) {
            akcja = PanelAction::cmd(cmd);
        }
    }
    if m.spolki.is_empty() {
        ui.label(ctx.text("ui.panel.stock.no_listings"));
        return akcja;
    }
    let nazwy: Vec<String> = m.spolki.iter().map(|s| s.name.clone()).collect();
    picker(ui, th, &nazwy, &mut view.sel);
    let sp = &m.spolki[view.sel.min(m.spolki.len() - 1)];

    section(ui, th, &ctx.text("ui.stock.quote"));
    // Kurs i **wycena spółki** obok siebie: sama cena punktu bazowego jest dla
    // gracza nieczytelna (`FF-16`).
    row(
        ui,
        th,
        &ctx.text("ui.stock.last"),
        &ctx.fmt(
            "ui.stock.price_and_cap",
            &[
                ("kurs", &ctx.money(sp.last)),
                (
                    "wycena",
                    &ctx.money(Money(
                        sp.last.get() * i64::from(magnat_economy::equity::WHOLE_BP),
                    )),
                ),
            ],
        ),
    );
    let zmiana = sp.last.get() - sp.prev.get();
    super::widgets::kpi(
        ui,
        th,
        &ctx.text("ui.stock.change"),
        &ctx.money(Money(zmiana)),
        if zmiana < 0 { Sev::Bad } else { Sev::Good },
    );
    row(
        ui,
        th,
        &ctx.text("ui.stock.float"),
        &super::widgets::percent_bp(i32::from(sp.float_bp)),
    );
    match m.wyniki {
        None => {
            ui.label(ctx.text("ui.stock.no_results"));
        }
        Some((miesiac, zysk)) => {
            row(
                ui,
                th,
                &ctx.fmt("ui.stock.results", &[("miesiac", &miesiac.to_string())]),
                &ctx.money(zysk),
            );
        }
    }
    row(
        ui,
        th,
        &ctx.text("ui.stock.next_results"),
        &ctx.fmt("ui.stock.in_days", &[("dni", &m.do_publikacji.to_string())]),
    );

    section(ui, th, &ctx.text("ui.stock.book"));
    if m.arkusz.is_empty() {
        ui.label(ctx.text("ui.stock.empty_book"));
    }
    for z in &m.arkusz {
        let strona = ctx.text(if z.sell {
            "ui.stock.sell"
        } else {
            "ui.stock.buy"
        });
        let etykieta = if z.moje {
            format!("{strona} {}", ctx.text("ui.stock.mine"))
        } else {
            strona
        };
        row(
            ui,
            th,
            &etykieta,
            &ctx.fmt(
                "ui.stock.order_row",
                &[("limit", &ctx.money(z.limit)), ("bp", &z.bp.to_string())],
            ),
        );
    }

    section(ui, th, &ctx.text("ui.stock.holders"));
    row(
        ui,
        th,
        &ctx.text("ui.stock.my_stake"),
        &super::widgets::percent_bp(i32::from(m.moj_udzial)),
    );
    for (kto, bp, kontrola) in &m.akcjonariat {
        // `owner_subject` zwraca `None` dla gracza — bo gracz jest rolą, a nie
        // encją (`K-85`). To jest jedyny akcjonariusz, którego nazwę zna panel.
        let etykieta = match kto {
            Some(Subject::Firm(f)) => ctx.fmt(
                "ui.subject.firm",
                &[("nr", &f.entity().index().to_string())],
            ),
            Some(Subject::Citizen(c)) => ctx.fmt(
                "ui.subject.citizen",
                &[("nr", &c.entity().index().to_string())],
            ),
            Some(x) => ctx.text(&format!("ui.subject.kind.{}", x.kind().key())),
            None => ctx.text("ui.stock.holder_player"),
        };
        let wartosc = if *kontrola {
            format!(
                "{} {}",
                super::widgets::percent_bp(i32::from(*bp)),
                ctx.text("ui.stock.control")
            )
        } else {
            super::widgets::percent_bp(i32::from(*bp))
        };
        row(ui, th, &etykieta, &wartosc);
    }

    section(ui, th, &ctx.text("ui.stock.place"));
    // Limit: kurs ostatniego fixingu. Zlecenie po cenie rynkowej jest tym, czego
    // gracz chce w dziewięciu przypadkach na dziesięć, a arkusz i tak rozstrzyga
    // fixingiem, nie kolejnością.
    for sell in [false, true] {
        let cmd = PlayerCommand::PlaceStockOrder {
            firm: sp.firm,
            sell,
            limit: sp.last,
            bp: KROK_BP,
            days: WAZNOSC_DNI,
        };
        let blokada = crate::precheck(&ctx.session.view(), &cmd)
            .err()
            .map(|e| e.text(ctx.c, ctx.l));
        let etykieta = ctx.fmt(
            if sell {
                "ui.stock.place_sell"
            } else {
                "ui.stock.place_buy"
            },
            &[
                ("bp", &KROK_BP.to_string()),
                (
                    "kwota",
                    &ctx.money(Money(sp.last.get() * i64::from(KROK_BP))),
                ),
            ],
        );
        if action(ui, ctx, &etykieta, blokada) {
            akcja = PanelAction::cmd(cmd);
        }
    }
    row(ui, th, &ctx.text("ui.stock.cash"), &ctx.money(m.gotowka));
    akcja
}
