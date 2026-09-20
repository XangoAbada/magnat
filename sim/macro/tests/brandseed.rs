//! Decyzja `D7` fazy M10: zasiew pamięci marek po `lower()` (M10b).
//!
//! Kryterium jest jawne w treści decyzji: 2–4 sloty marek per mieszkaniec, wybrane
//! ze sklepów **jego dzielnicy**, proporcjonalnie do ich udziału rynkowego. Test
//! sprawdza wszystkie trzy warunki naraz i determinizm zasiewu.

use magnat_agents::{
    register, register_society, AgentState, BrandsRef, DemographyTable, Employment, Identity,
    KnowledgeRef, Lifecycle, NeedTable, Needs, Personality, PlanRef, Population, RelationsRef,
    Residence, SkillSlot, Skills, Vitals, Wealth,
};
use magnat_core::{DistrictId, Entity, FirmId, Money, Qty, Tick};
use magnat_ecs::World;
use magnat_macro::{seed_memory, MacroState, SEED_MAX, SEED_MIN};

const SEED: u64 = 91;

fn swiat(n: u32) -> World {
    let mut w = World::new(SEED);
    register(&mut w, NeedTable::load_default().expect("needs"));
    register_society(&mut w, DemographyTable::load_default().expect("demography"));
    for i in 0..n {
        let e = w
            .spawn()
            .with(Identity {
                birth_day: -(360 * 30),
                flags: Identity::FLAG_ALIVE,
                household: i,
                ..Identity::default()
            })
            .with(Personality([50; 8]))
            .with(Vitals {
                health: 90,
                energy: 80,
                ..Vitals::default()
            })
            .with(Needs::default())
            .with(Skills([SkillSlot::default(); 4]))
            .with(Wealth::default())
            .with(Employment::default())
            .with(Residence {
                building: i,
                unit: 0,
                district: (i % 2) as u16,
            })
            .with(PlanRef::default())
            .with(AgentState::default())
            .with(KnowledgeRef::default())
            .with(RelationsRef::default())
            .with(BrandsRef::default())
            .with(Lifecycle::default())
            .id();
        w.resource_mut::<Population>().add_citizen(e);
    }
    w
}

/// Stan makro z czterema firmami: dwie w dzielnicy 0, dwie w dzielnicy 1.
fn stan() -> MacroState {
    let mut st = MacroState::empty(magnat_macro::ClassGrain::Classes2, 4, 2);
    for i in 0..4u32 {
        st.firms.push(magnat_macro::MacroFirm {
            id: FirmId(Entity::new(100 + i, std::num::NonZeroU32::MIN)),
            district: DistrictId((i % 2) as u16),
            branch: magnat_macro::BranchId(4),
            capital: Money(10_000_000),
            debt: Money::ZERO,
            stock: magnat_macro::MacroStock::new(),
            capacity_daily: Qty(12_000),
            // Udział rynkowy: pierwsza firma dzielnicy pracuje pełną parą, druga ledwo.
            utilization_bps: if i < 2 { 9_000 } else { 500 },
            employees: 8,
            wage_bill: Money(3_200_000),
            price: magnat_macro::SparseVec::new(),
            cost: magnat_macro::SparseVec::new(),
            brand_stock: 0,
            tech: magnat_macro::TechLevel(50),
            suppliers: smallvec::SmallVec::new(),
        });
    }
    st.firms.sort_by_key(|f| f.id.0.to_bits());
    st
}

#[test]
fn zasiew_daje_kazdemu_dwie_do_czterech_marek_z_jego_dzielnicy() {
    let mut w = swiat(400);
    let st = stan();
    let ile = seed_memory(&st, &mut w, SEED, Tick(0));
    assert!(ile > 0, "zasiew nie zasiał ani jednego slotu");

    let spis = w.resource::<Population>().citizens().to_vec();
    // Marki dzielnicy 0 to firmy o parzystym numerze encji, dzielnicy 1 — nieparzystym.
    for e in spis {
        let d = w.get::<Residence>(e).expect("adres").district;
        let sloty = magnat_agents::slots_of(&w, e, 0);
        let n = sloty.len() as u32;
        assert!(
            (SEED_MIN..=SEED_MAX).contains(&n),
            "mieszkaniec dostał {n} marek, oczekiwane {SEED_MIN}–{SEED_MAX}"
        );
        for s in sloty.as_slice() {
            let firma = magnat_supply::firm_of(s.brand);
            let f = st
                .firms
                .iter()
                .find(|f| f.id == firma)
                .expect("marka spoza stanu makro");
            assert_eq!(
                f.district,
                DistrictId(d),
                "mieszkaniec dzielnicy {d} zna sklep z dzielnicy {}",
                f.district.0
            );
        }
        // Zasiew ma dawać **pamięć**, nie samą etykietę: świat „zużyty" w liczbach
        // i sterylny w zachowaniu byłby dokładnie tym, przed czym broni `D7`.
        assert!(
            sloty.as_slice().iter().all(|s| s.awareness > 0),
            "zasiany slot ma zerową znajomość"
        );
    }
}

#[test]
fn zasiew_jest_deterministyczny() {
    let st = stan();
    let mut a = swiat(200);
    let mut b = swiat(200);
    assert_eq!(
        seed_memory(&st, &mut a, SEED, Tick(0)),
        seed_memory(&st, &mut b, SEED, Tick(0))
    );
    let spis: Vec<_> = a.resource::<Population>().citizens().to_vec();
    for e in spis {
        let sa = magnat_agents::slots_of(&a, e, 0);
        let sb = magnat_agents::slots_of(&b, e, 0);
        assert_eq!(sa.as_slice(), sb.as_slice());
    }
}

#[test]
fn udzial_rynkowy_wazy_kogo_mieszkancy_znaja() {
    // Dzielnica z ośmioma sklepami i czterema slotami w pamięci: mieszkaniec **musi**
    // wybrać, a wybór idzie za udziałem rynkowym. Bez tego zasiew byłby losowaniem
    // po równo, a `D7` mówi wprost o udziale. Przy dwóch sklepach na dzielnicę
    // ten test nie mógłby nic zmierzyć — zna się wtedy oba i to jest poprawne.
    let mut w = swiat(600);
    let mut st = MacroState::empty(magnat_macro::ClassGrain::Classes2, 4, 1);
    for i in 0..8u32 {
        st.firms.push(magnat_macro::MacroFirm {
            id: FirmId(Entity::new(100 + i, std::num::NonZeroU32::MIN)),
            district: DistrictId(0),
            branch: magnat_macro::BranchId(4),
            capital: Money(10_000_000),
            debt: Money::ZERO,
            stock: magnat_macro::MacroStock::new(),
            capacity_daily: Qty(12_000),
            // Cztery zakłady pracują pełną parą, cztery ledwo zipią.
            utilization_bps: if i < 4 { 9_000 } else { 300 },
            employees: 8,
            wage_bill: Money(3_200_000),
            price: magnat_macro::SparseVec::new(),
            cost: magnat_macro::SparseVec::new(),
            brand_stock: 0,
            tech: magnat_macro::TechLevel(50),
            suppliers: smallvec::SmallVec::new(),
        });
    }
    st.firms.sort_by_key(|f| f.id.0.to_bits());
    // Wszyscy mieszkają w dzielnicy 0, żeby mierzyć wybór, a nie adres.
    let spis = w.resource::<Population>().citizens().to_vec();
    for e in &spis {
        if let Some(r) = w.get_mut::<Residence>(*e) {
            r.district = 0;
        }
    }

    let _ = seed_memory(&st, &mut w, SEED, Tick(0));

    let mut licznik = [0u32; 8];
    for e in spis {
        for s in magnat_agents::slots_of(&w, e, 0).as_slice() {
            let idx = magnat_supply::firm_of(s.brand).0.index() - 100;
            licznik[idx as usize] += 1;
        }
    }
    println!("znajomość per firma: {licznik:?}");
    let mocne: u32 = licznik[..4].iter().sum();
    let slabe: u32 = licznik[4..].iter().sum();
    assert!(
        mocne > slabe * 3,
        "udział rynkowy nie przełożył się na znajomość: mocne {mocne}, słabe {slabe}"
    );
}
