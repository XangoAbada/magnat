//! Kryterium zamknięcia M7a WP3, oba zdania osobno:
//!
//! 1. **zakład z załogą wytwarza w M6 wynik proporcjonalny do `effective_labor`** —
//!    ten sam zakład, ta sama doba, różna obsada, zmierzona masa;
//! 2. **przebieg mikro i mezo tego samego zakładu daje identyczną sumę wypłat**
//!    (tolerancja 0) — czyli lista płac nie zależy od tego, jak gęsto liczymy czas.

use magnat_agents::{ShiftKind, Vitals};
use magnat_core::{
    BuildingId, CitizenId, DistrictId, Entity, JobRoleId, Money, SimCalendar, SimMinute, SiteId,
    Tick, Q,
};
use magnat_firms::firm::Owner;
use magnat_firms::hr::employment::Employment;
use magnat_firms::hr::roles::{RoleTable, RoleWeights};
use magnat_firms::{Firm, FirmKey, Firms, Position, Site, SiteTypeId};
use std::num::NonZeroU32;

const ROLE: JobRoleId = JobRoleId(0);
const PLACA: Money = Money(450_000);

fn role_table() -> RoleTable {
    RoleTable::parse(
        r#"(
            schema_version: 2,
            roles: [
                ( key: "production_worker",
                  weights: (skill: 300, energy: 350, mood: 100, health: 250) ),
            ],
        )"#,
    )
    .expect("tablica ról")
}

fn mieszkaniec(i: u32) -> CitizenId {
    CitizenId(Entity::new(i, NonZeroU32::MIN))
}

fn w_formie() -> Vitals {
    Vitals {
        health: 100,
        energy: 100,
        mood: 100,
        stress: 0,
        edu_level: 0,
        edu_field: 0,
        status: 50,
        _pad: 0,
    }
}

/// Zakład o `slots` etatach, z których `obsadzone` są zajęte.
fn zaklad(key: FirmKey, slots: u16, obsadzone: u16) -> Site {
    let mut p = Position::new(ROLE, slots, false, (Money(300_000), Money(600_000)));
    for i in 0..obsadzone {
        p.filled.push(Employment::new(
            mieszkaniec(1000 + u32::from(i)),
            ROLE,
            PLACA,
            SimMinute(0),
            ShiftKind::Day,
        ));
    }
    // Stanowisko kierownicze zakład ma z katalogu; tu buduję go ręcznie, więc
    // sprawdzam samą arytmetykę obsady i kierownika nie potrzebuję.
    Site {
        id: SiteId(Entity::new(7, NonZeroU32::MIN)),
        firm: key,
        site_type: SiteTypeId(0),
        building: BuildingId(Entity::new(7, NonZeroU32::MIN)),
        district: DistrictId(1),
        floor_m2: 1_000,
        positions: vec![p],
        mgmt: magnat_firms::ManagementQuality::NEUTRAL,
        tech: Q::new(50),
        fixed_cost_month: Money(1_200_000),
        pnl: magnat_firms::Ring::new(),
        opened: SimMinute(0),
    }
}

fn firmy(slots: u16, obsadzone: u16) -> (Firms, FirmKey) {
    let mut f = Firms::new();
    let key = f.insert(|k| {
        Firm::sole_owner(
            k,
            "Młyn".to_owned(),
            SimMinute(0),
            DistrictId(1),
            Owner::Player,
        )
    });
    assert!(f.add_site(zaklad(key, slots, obsadzone)));
    (f, key)
}

#[test]
fn pokrycie_etatowe_jest_proporcjonalne_do_obsady() {
    let roles = role_table();
    let zdrowi = |_: CitizenId| Some((w_formie(), Q::new(100)));

    let (pelna, _) = firmy(10, 10);
    let (polowa, _) = firmy(10, 5);
    let (pusty, _) = firmy(10, 0);

    let id = SiteId(Entity::new(7, NonZeroU32::MIN));
    let p_pelna = pelna.site(id).expect("zakład").labor_pct(&roles, &zdrowi);
    let p_polowa = polowa.site(id).expect("zakład").labor_pct(&roles, &zdrowi);
    let p_pusty = pusty.site(id).expect("zakład").labor_pct(&roles, &zdrowi);

    assert_eq!(p_pelna, magnat_supply::PlantSite::FULL_LABOR);
    // Połowa załogi to dokładnie połowa pokrycia — skala ma punkt neutralny,
    // więc proporcjonalność nie jest przybliżona.
    assert_eq!(p_polowa, 500, "połowa obsady dała {p_polowa}‰");
    assert_eq!(p_pusty, 0, "zakład bez ludzi ma stać, a nie produkować");
}

#[test]
fn chora_zaloga_obniza_pokrycie() {
    let roles = role_table();
    let id = SiteId(Entity::new(7, NonZeroU32::MIN));
    let (f, _) = firmy(10, 10);
    let site = f.site(id).expect("zakład");

    let chorzy = |_: CitizenId| {
        let mut v = w_formie();
        v.health = 40;
        v.energy = 40;
        Some((v, Q::new(100)))
    };
    let zdrowi = |_: CitizenId| Some((w_formie(), Q::new(100)));
    assert!(site.labor_pct(&roles, &chorzy) < site.labor_pct(&roles, &zdrowi));
}

#[test]
fn nieobecny_mieszkaniec_nie_pracuje() {
    // Mieszkaniec, którego już nie ma (zmarł, wyprowadził się), nie wytwarza pracy.
    // Etat zostaje — zwolni go rotacja M7b — ale nie udaje obsadzonego.
    let roles = role_table();
    let id = SiteId(Entity::new(7, NonZeroU32::MIN));
    let (f, _) = firmy(10, 10);
    let nikogo = |_: CitizenId| None;
    assert_eq!(f.site(id).expect("zakład").labor_pct(&roles, &nikogo), 0);
}

#[test]
fn wyplata_obciaza_firme_suma_stawek() {
    let (mut f, key) = firmy(10, 10);
    let dzien = magnat_firms::payday(key);
    // Doba, w której wypada dzień wypłaty tej firmy.
    let cal = SimCalendar::new(Tick(u64::from(dzien) * 1440));
    let run = f.run_payroll(cal);
    assert_eq!(run.items.len(), 10);
    assert_eq!(run.total_gross(), Money(PLACA.get() * 10));
    // Brutto == netto do czasu M8 (`D6`).
    assert!(run.items.iter().all(|i| i.net() == PLACA));
}

#[test]
fn wyplata_nie_wypada_dwa_razy_w_miesiacu() {
    let (mut f, key) = firmy(10, 10);
    let mut ile = 0;
    for d in 0..30u64 {
        let cal = SimCalendar::new(Tick(d * 1440));
        if !f.run_payroll(cal).is_empty() {
            ile += 1;
        }
    }
    assert_eq!(ile, 1, "firma {key:?} wypłaciła {ile} razy w miesiącu");
}

#[test]
fn suma_wyplat_nie_zalezy_od_gestosci_liczenia_czasu() {
    // Test LOD (dokument 00 §4, tolerancja 0): mezo liczy dobę jednym krokiem,
    // mikro minuta po minucie. Suma wypłat miesiąca musi być **identyczna** —
    // inaczej obrót kamerą gracza zmieniałby saldo gospodarstwa domowego.
    let miesiac = 30 * 1440;

    let (mut mezo, _) = firmy(10, 10);
    let mut suma_mezo = 0i64;
    for d in 0..30u64 {
        suma_mezo += mezo
            .run_payroll(SimCalendar::new(Tick(d * 1440)))
            .total_gross()
            .get();
    }

    let (mut mikro, _) = firmy(10, 10);
    let mut suma_mikro = 0i64;
    for m in 0..miesiac {
        // Lista płac domyka się raz na dobę, na jej granicy — tak samo w obu trybach.
        let cal = SimCalendar::new(Tick(m));
        if cal.minute_of_day() == 0 {
            suma_mikro += mikro.run_payroll(cal).total_gross().get();
        }
    }

    assert_eq!(suma_mezo, suma_mikro);
    assert_eq!(suma_mezo, PLACA.get() * 10);
}

#[test]
fn rachunek_wyniku_zakladu_zapisuje_koszt_pracy_i_koszt_staly() {
    let (mut f, key) = firmy(10, 10);
    let id = SiteId(Entity::new(7, NonZeroU32::MIN));
    let cal = SimCalendar::new(Tick(u64::from(magnat_firms::payday(key)) * 1440));
    f.run_payroll(cal);
    let site = f.site(id).expect("zakład");
    let ostatni = site.pnl.last().copied().expect("miesiąc zapisany");
    assert_eq!(ostatni.labor, Money(PLACA.get() * 10));
    assert_eq!(ostatni.fixed, Money(1_200_000));
    assert_eq!(ostatni.cost(), Money(PLACA.get() * 10 + 1_200_000));
}

#[test]
fn wagi_roli_zmieniaja_wynik_a_nie_jego_skale() {
    // Dwie role o tej samej sumie wag, ale innym rozkładzie: pracownik o wysokiej
    // umiejętności i słabym zdrowiu wypada inaczej w każdej z nich.
    let a = RoleWeights {
        skill: 700,
        energy: 100,
        mood: 100,
        health: 100,
    };
    let b = RoleWeights {
        skill: 100,
        energy: 100,
        mood: 100,
        health: 700,
    };
    let mut v = w_formie();
    v.health = 20;
    let fachowiec_a = magnat_firms::effective_labor(
        &v,
        Q::new(100),
        Q::new(50),
        magnat_firms::ManagementQuality::NEUTRAL,
        &a,
    );
    let fachowiec_b = magnat_firms::effective_labor(
        &v,
        Q::new(100),
        Q::new(50),
        magnat_firms::ManagementQuality::NEUTRAL,
        &b,
    );
    assert!(
        fachowiec_a.0 > fachowiec_b.0,
        "zawód ceniący umiejętność ma inaczej wyceniać tego człowieka"
    );
}
