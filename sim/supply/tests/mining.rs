//! Wydobycie — kryterium ukończenia WP10 (M6d).
//!
//! Trzy zdania kryterium, trzy testy: `prop_deposit_monotone`, złoże wyczerpane
//! w przebiegu pięćdziesięcioletnim razem z zamknięciem szybu, oraz kaskada niedoboru
//! odpalająca się **w dół łańcucha** — u rafinerii, która na tym szybie stała.
//!
//! Katalog i strojenie są prawdziwe (`data/`), a szyb jest tym z §7.2 dokumentu fazy:
//! koncentracja 82 %, głębokość 1 800 m, receptura `oil_well`.

use magnat_core::{
    DepositId, Energy, Entity, FirmId, GoodId, Mass, Money, OpenHours, Q, SimMinute, SiteId,
    UtilityService, Volume,
};
use magnat_supply::catalog::load_default;
use magnat_supply::plant::{advance_production, Dock, PlantSite, ProductionCtx};
use magnat_supply::store::WarehouseRole;
use magnat_supply::{
    Catalog, Deposits, HazardClass, LineState, MiningSite, Plant, ProductionLine, SlotId,
    StorageClass, Store, Tuning,
};
use std::cell::Cell;
use std::num::NonZeroU32;

fn encja(i: u32) -> Entity {
    Entity::new(i, NonZeroU32::new(1).expect("generacja"))
}

/// Bilans jednego złoża. Odpowiednik `magnat_world::DepositLedger` bez `sim/world` —
/// crate testowy nie ma prawa ciągnąć za sobą generatora terenu, żeby sprawdzić
/// arytmetykę wyczerpania.
struct Zloze {
    reserves: Mass,
    extracted: Cell<i64>,
}

impl Zloze {
    fn nowe(reserves: Mass) -> Zloze {
        Zloze {
            reserves,
            extracted: Cell::new(0),
        }
    }
}

impl Deposits for Zloze {
    fn remaining(&self, _id: DepositId) -> Mass {
        Mass(self.reserves.0 - self.extracted.get())
    }
    fn initial(&self, _id: DepositId) -> Mass {
        self.reserves
    }
    fn extract(&self, id: DepositId, want: Mass) -> Mass {
        let got = want.0.clamp(0, self.remaining(id).0);
        self.extracted.set(self.extracted.get() + got);
        Mass(got)
    }
}

/// Szyb naftowy z §7.2: jedna linia `well`, ruch ciągły, zbiornik na urobek.
struct Szyb {
    cat: Catalog,
    tuning: Tuning,
    store: Store,
    plant: Plant,
    zloze: Zloze,
    site: SiteId,
    zbiornik: SlotId,
    ropa: GoodId,
}

/// Ile gramów ropy daje szyb na dobę przy skali bazowej — z receptury, nie z komentarza.
const DOBA: u64 = 1_440;

impl Szyb {
    fn nowy(zasoby_t: i64) -> Szyb {
        let cat = load_default("contemporary").expect("katalog z data/");
        let tuning = Tuning::load_default().expect("data/tuning/supply.ron");
        let mut store = Store::new(cat.goods.len());
        let site = SiteId(encja(21));
        let owner = FirmId(encja(2));

        let ropa = cat.good_id("raw_crude_oil").expect("raw_crude_oil");
        let wydobycie = cat.recipe_id("oil_well").expect("oil_well");
        let czesc = cat.good_id("part_pump_seal").expect("part_pump_seal");

        // Zbiornik z jawną zgodą na towar łatwopalny — bez maski `put` odmówi przyjęcia
        // i szyb wpadłby w `Blocked`, co wyglądałoby na błąd wydobycia.
        let zbiornik = store.add_slot(
            site,
            WarehouseRole::Output,
            StorageClass::Tank,
            Mass(i64::MAX / 4),
            Volume(i64::MAX / 4),
            HazardClass::Flammable.bit(),
        );

        let mut zaklad = PlantSite::new(site, owner, Dock::new(1, 20, 1, OpenHours::ALWAYS));
        zaklad.outputs.push(zbiornik);
        zaklad.mining = Some(MiningSite::new(DepositId(0), Q::new(82), 1_800));
        zaklad.meters.push(magnat_supply::UtilityMeter::new(
            UtilityService::Electricity,
            FirmId(encja(3)),
            Money(tuning.utility.power_gr_per_kwh),
        ));
        // Ruch ciągły: szyb pracuje na trzy zmiany, inaczej dobowa szarża nigdy
        // nie zdąży się zacząć w oknie jednej zmiany.
        for (i, od) in [0u16, 480, 960].into_iter().enumerate() {
            zaklad.schedule.shifts[i] = Some(magnat_supply::Shift {
                from: od,
                to: od + 480,
                headcount: 6,
                wage_multiplier_pct: 100,
                skill: Q::new(60),
            });
        }

        let mut linia = ProductionLine::new(
            cat.machine_class_id("well").expect("klasa maszyny"),
            // Przepustowość godzinowa równa dobowej wydajności receptury podzielonej
            // przez 24 — szarża trwa dobę, więc wsad szarży wychodzi na całą dobę.
            Mass(cat.recipe(wydobycie).batch_mass.0 / 24),
            Energy(1_200_000 / 24),
            czesc,
        );
        // Pompa praktycznie niezawodna: ten test mierzy wyczerpanie złoża, a nie rozkład
        // awarii. Przegląd planowy zostaje domyślny (co 720 h), bo bez niego `condition`
        // spada do zera po kilkunastu latach pracy i szyb staje z powodu, który z tym
        // testem nie ma nic wspólnego.
        linia.mtbf_hours = 1_000_000;
        // Pierwszy termin przeglądu trzeba **nadać**: `ProductionLine::new` zostawia
        // `next_maintenance` na `u64::MAX`, czyli „nigdy". Bez tego `condition` schodzi
        // do zera po 166 dobach ciągłej pracy i szyb staje na awarii, której nie ma
        // czym naprawić. To jest pułapka dla M6e i jest wpisana do jego tabeli korekt.
        linia.next_maintenance = SimMinute(720 * 60);
        linia.recipe = Some(wydobycie);
        zaklad.lines.push(linia);

        let mut plant = Plant::new();
        plant.insert(zaklad);
        Szyb {
            cat,
            tuning,
            store,
            plant,
            zloze: Zloze::nowe(Mass(zasoby_t * 1_000_000)),
            site,
            zbiornik,
            ropa,
        }
    }

    fn doba(&mut self, dzien: u64) {
        let ctx = ProductionCtx {
            cat: &self.cat,
            tuning: &self.tuning,
            world_seed: 5,
            deposits: &self.zloze,
        };
        advance_production(
            &ctx,
            &mut self.store,
            &mut self.plant,
            self.site,
            SimMinute(dzien * DOBA),
            DOBA as u32,
        );
    }

    fn koszt_tony(&self) -> i64 {
        let m = self.plant.get(self.site).expect("szyb").mining.expect("złoże");
        m.cost_per_tonne(
            Money(self.tuning.mining.base_gr_per_tonne),
            self.zloze.remaining(DepositId(0)),
            self.zloze.initial(DepositId(0)),
        )
        .0
    }

    fn stan_linii(&self) -> LineState {
        self.plant.get(self.site).expect("szyb").lines[0].state
    }
}

#[test]
fn koszt_startowy_szybu_zgadza_sie_z_lancuchem_referencyjnym() {
    let s = Szyb::nowy(2_400_000);
    // §7.2: 1 620 zł/t na złożu nietkniętym. Tolerancja ±2 % jak dla wszystkich
    // kosztów łańcuchów referencyjnych (§7.1 zdanie wstępne).
    let zl = s.koszt_tony();
    assert!(
        (158_760..=165_240).contains(&zl),
        "koszt startowy {zl} gr/t poza ±2 % wobec 162 000 gr/t z §7.2"
    );
}

/// `prop_deposit_monotone` (§7.3 pkt 7): `remaining` nigdy nie rośnie, koszt wydobycia
/// nigdy nie maleje wraz z wyczerpaniem, koncentracja efektywna monotonicznie nierosnąca.
///
/// Sprawdzane na **przebiegu**, nie na czystej funkcji: monotoniczność samego wzoru
/// ma test jednostkowy w `mining.rs`, a ten pyta o coś innego — czy produkcja nie umie
/// tej monotoniczności złamać, na przykład oddając masę do złoża przy nieudanej szarży.
#[test]
fn prop_deposit_monotone() {
    let mut s = Szyb::nowy(30_000);
    let mut poprzednie = s.zloze.remaining(DepositId(0)).0;
    let mut poprzedni_koszt = 0i64;
    let mut poprzednia_konc = i64::MAX;
    for d in 0..200 {
        s.doba(d);
        let zostalo = s.zloze.remaining(DepositId(0)).0;
        assert!(zostalo <= poprzednie, "złoże urosło w dobie {d}");
        assert!(zostalo >= 0, "złoże zeszło poniżej zera w dobie {d}");
        let koszt = s.koszt_tony();
        assert!(koszt >= poprzedni_koszt, "koszt spadł w dobie {d}");
        let m = s.plant.get(s.site).expect("szyb").mining.expect("złoże");
        let konc = m.concentration_eff(Mass(zostalo), s.zloze.initial(DepositId(0)));
        assert!(konc <= poprzednia_konc, "koncentracja wzrosła w dobie {d}");
        poprzednie = zostalo;
        poprzedni_koszt = koszt;
        poprzednia_konc = konc;
    }
    assert!(poprzednie < s.zloze.initial(DepositId(0)).0, "nic nie wydobyto");
}

/// Wydobyta masa i masa w zbiorniku to **ta sama** liczba: szyb nie tworzy ropy
/// z niczego ani jej nie gubi po drodze.
#[test]
fn co_zeszlo_ze_zloza_stoi_w_zbiorniku() {
    let mut s = Szyb::nowy(5_000);
    for d in 0..40 {
        s.doba(d);
    }
    let ze_zloza = s.zloze.initial(DepositId(0)).0 - s.zloze.remaining(DepositId(0)).0;
    let w_zbiorniku = s.store.stock_of(s.zbiornik, s.ropa).0;
    assert!(ze_zloza > 0, "nic nie wydobyto");
    assert_eq!(
        ze_zloza, w_zbiorniku,
        "różnica {} g między złożem a zbiornikiem",
        ze_zloza - w_zbiorniku
    );
    s.store.check_mass(s.ropa).expect("bilans masy ropy");
}

/// Złoże wyczerpane w przebiegu pięćdziesięcioletnim: szyb staje, a jego koszt
/// wydobycia po drodze przekracza cenę importową — czyli import staje się opłacalny
/// **sam z siebie**, bez reguły, która by to wymuszała (§7.2 etap 1).
#[test]
fn zloze_wyczerpuje_sie_w_piecdziesiat_lat_i_szyb_staje() {
    // 300 t/dobę × 360 dni × 50 lat = 5,4 Mt. Bierzemy dwie trzecie, żeby wyczerpanie
    // wypadło **wewnątrz** okna, a nie dokładnie na jego krawędzi.
    let mut s = Szyb::nowy(3_600_000);
    let importowa = s
        .cat
        .good(s.ropa)
        .external_base_price
        .expect("ropa jest importowalna")
        .0
        * 10; // cena katalogowa jest za 100 kg, koszt wydobycia za tonę
    let mut dzien_drozej_niz_import = None;
    let mut dzien_postoju = None;

    for d in 0..(360 * 50) {
        s.doba(d);
        if dzien_drozej_niz_import.is_none() && s.koszt_tony() > importowa {
            dzien_drozej_niz_import = Some(d);
        }
        if s.zloze.remaining(DepositId(0)).0 == 0 {
            // Jeszcze jedna doba, żeby linia zdążyła zobaczyć puste złoże.
            s.doba(d + 1);
            dzien_postoju = Some(d);
            break;
        }
    }

    let d = dzien_postoju.expect("złoże nie wyczerpało się w 50 lat");
    assert!(
        d < 360 * 50,
        "wyczerpanie w dobie {d}, czyli poza oknem pięćdziesięciu lat"
    );
    assert!(
        matches!(s.stan_linii(), LineState::Starved { .. }),
        "szyb nie stanął po wyczerpaniu: {:?}",
        s.stan_linii()
    );
    let kiedy = dzien_drozej_niz_import.expect("koszt wydobycia nigdy nie przebił importu");
    assert!(
        kiedy < d,
        "import stał się tańszy dopiero po zamknięciu szybu ({kiedy} vs {d})"
    );
}

/// Kaskada niedoboru odpala się **w dół łańcucha**: kiedy szyb przestaje dawać ropę,
/// to rafineria — a nie szyb — wchodzi na drabinę z PRD §8.4 i w końcu staje.
///
/// Rafineria bierze wsad z tego samego zbiornika, do którego szyb tłoczy urobek.
/// `ponytail:` zamiast zlecenia transportowego, bo pytanie tego testu brzmi „czy
/// wyczerpanie złoża widać u odbiorcy", a nie „czy partia jedzie legalnie" — to drugie
/// ma własny test własnościowy (`prop_no_teleport` w `teleport.rs`). Sufit nazwany:
/// gdyby kiedyś trzeba było sprawdzić opóźnienie rurociągu, wchodzi tu `Transport`.
#[test]
fn wyczerpane_zloze_kaskaduje_do_rafinerii() {
    let mut s = Szyb::nowy(1_200); // ~4 doby wydobycia
    let rafineria = SiteId(encja(22));
    let przerob = s
        .cat
        .recipe_id("refinery_crude_fractionation")
        .expect("refinery_crude_fractionation");

    {
        let wyjscie = s.store.add_slot(
            rafineria,
            WarehouseRole::Output,
            StorageClass::Tank,
            Mass(i64::MAX / 4),
            Volume(i64::MAX / 4),
            HazardClass::Flammable.bit(),
        );
        let mut z = PlantSite::new(
            rafineria,
            FirmId(encja(4)),
            Dock::new(2, 20, 1, OpenHours::ALWAYS),
        );
        z.inputs.push(s.zbiornik);
        z.outputs.push(wyjscie);
        z.meters.push(magnat_supply::UtilityMeter::new(
            UtilityService::Electricity,
            FirmId(encja(3)),
            Money(s.tuning.utility.power_gr_per_kwh),
        ));
        for (i, od) in [0u16, 480, 960].into_iter().enumerate() {
            z.schedule.shifts[i] = Some(magnat_supply::Shift {
                from: od,
                to: od + 480,
                headcount: 20,
                wage_multiplier_pct: 100,
                skill: Q::new(60),
            });
        }
        let mut linia = ProductionLine::new(
            s.cat.machine_class_id("refinery_cdu").expect("klasa maszyny"),
            Mass(12_000_000), // 12 t/h — instalacja ciągła z §7.2
            Energy(4_200_000),
            s.cat.good_id("part_pump_seal").expect("part_pump_seal"),
        );
        linia.mtbf_hours = 1_000_000;
        linia.next_maintenance = SimMinute(720 * 60);
        linia.recipe = Some(przerob);
        z.lines.push(linia);
        s.plant.insert(z);
    }

    let mut szczeble = Vec::new();
    for d in 0..30u64 {
        for g in 0..24u64 {
            let minuta = d * DOBA + g * 60;
            {
                let ctx = ProductionCtx {
                    cat: &s.cat,
                    tuning: &s.tuning,
                    world_seed: 5,
                    deposits: &s.zloze,
                };
                advance_production(
                    &ctx,
                    &mut s.store,
                    &mut s.plant,
                    s.site,
                    SimMinute(minuta),
                    60,
                );
                advance_production(
                    &ctx,
                    &mut s.store,
                    &mut s.plant,
                    rafineria,
                    SimMinute(minuta),
                    60,
                );
            }
            let cat = &s.cat;
            let store = &s.store;
            let t = s.tuning.shortage;
            let z = s.plant.get_mut(rafineria).expect("rafineria");
            magnat_supply::shortage::review(cat, store, z, SimMinute(minuta + 60), &t);
            let stan = z.stage(s.ropa).kind();
            if szczeble.last() != Some(&stan) {
                szczeble.push(stan);
            }
        }
    }

    assert_eq!(
        s.zloze.remaining(DepositId(0)).0,
        0,
        "złoże miało się wyczerpać w oknie testu"
    );
    assert!(
        matches!(s.stan_linii(), LineState::Starved { .. }),
        "szyb miał stanąć: {:?}",
        s.stan_linii()
    );
    assert_eq!(
        szczeble.last(),
        Some(&magnat_core::ShortageStageKind::Halted),
        "rafineria nie doszła do postoju; przebyte szczeble: {szczeble:?}"
    );
    // Dopóki szyb daje ropę, drabina wchodzi i **schodzi** — każda dobowa szarża
    // przywraca pokrycie i to jest poprawne zachowanie, nie usterka. Pytaniem testu
    // jest ostatnie zejście: po wyczerpaniu złoża dostawy nie ma i drabina ma przejść
    // szczebel po szczeblu do postoju, bez przeskoku na dno.
    let ostatnie_ok = szczeble
        .iter()
        .rposition(|k| *k == magnat_core::ShortageStageKind::Ok)
        .map_or(0, |i| i + 1);
    let koncowka = &szczeble[ostatnie_ok..];
    for para in koncowka.windows(2) {
        assert!(
            para[1].as_index() > para[0].as_index(),
            "kaskada cofnęła się po wyczerpaniu złoża: {koncowka:?}"
        );
    }
    assert!(
        koncowka.len() >= 4,
        "kaskada przeskoczyła szczeble: {koncowka:?}"
    );
}
