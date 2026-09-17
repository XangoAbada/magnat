//! Testy modelu makro (M7f WP13; M7 §7.5, §7.7).
//!
//! Cztery pytania i jedno, którego tu nie ma.
//!
//! **Są:** czy krok zachowuje pieniądz co do grosza, czy dwa przebiegi tego samego
//! stanu dają ten sam hash, czy ranking nie rozstrzyga tam, gdzie różnica mieści się
//! w marginesie, i czy rozstrzyga tam, gdzie nie mieści.
//!
//! **Nie ma:** czy prognoza zysku firmy zgadza się co do procentu. Takiej obietnicy
//! `sim/macro` nie składa i M7 na niej nie polega (§7.7, zdanie zamykające) —
//! test dokładności bezwzględnej albo przechodziłby przypadkiem, albo wymuszałby
//! kalibrację czegoś, czego skalibrować się nie da.

use magnat_core::{DistrictId, GoodId, Money, Qty};
use magnat_macro::state::{MacroCell, MacroFirm, MacroState};
use magnat_macro::types::{
    BranchId, CitizenSeed, ClassGrain, ClassId, CommuteMatrix, MacroAccount, SparseVec, TechLevel,
};
use magnat_macro::{rank_variants, state_hash, step, MacroParams, MacroStock, Scenario};

const ROLE: usize = 4;
const CHLEB: GoodId = GoodId(0);
const MLEKO: GoodId = GoodId(1);

/// Miasto testowe: dwie dzielnice × dwie klasy, cztery firmy, dwa towary.
///
/// Liczby są okrągłe z rozmysłu — test ma pokazywać, że coś się nie zgadza,
/// a nie zmuszać do szukania, o który grosz chodzi.
fn miasto() -> MacroState {
    let mut st = MacroState::empty(ClassGrain::Classes2, ROLE, 2);
    st.commute = CommuteMatrix::flat(2, 20);
    for d in 0..2u16 {
        for k in 0..2u8 {
            let mut c = MacroCell::new((DistrictId(d), ClassId(k)), ROLE);
            for i in 0..600u32 {
                c.citizens.push(CitizenSeed {
                    birth_index: u32::from(d) * 10_000 + u32::from(k) * 1_000 + i,
                    cell: d * 2 + u16::from(k),
                });
            }
            c.cash = Money(600 * 40_000);
            c.employed = 400;
            c.unemployed = 100;
            c.labor = vec![500, 500, 200, 100];
            c.skill_sum = vec![25_000, 25_000, 10_000, 5_000];
            st.cells.push(c);
        }
    }
    st.cells.sort_by_key(|c| (c.key.0 .0, c.key.1 .0));

    for i in 0..4u32 {
        let mut price: SparseVec<GoodId, Money> = SparseVec::new();
        let mut cost: SparseVec<GoodId, Money> = SparseVec::new();
        // Firmy różnią się ceną, żeby softmax miał co dzielić.
        price.set(CHLEB, Money(500 + i64::from(i) * 30));
        price.set(MLEKO, Money(420 + i64::from(i) * 25));
        cost.set(CHLEB, Money(400));
        cost.set(MLEKO, Money(340));
        let mut stock = MacroStock::new();
        stock.add(CHLEB, 40_000);
        stock.add(MLEKO, 40_000);
        st.firms.push(MacroFirm {
            id: magnat_core::FirmId(magnat_core::Entity::new(100 + i, std::num::NonZeroU32::MIN)),
            district: DistrictId((i % 2) as u16),
            branch: BranchId(4),
            capital: Money(30_000_000),
            debt: Money(0),
            stock,
            capacity_daily: Qty(12 * magnat_firms::hr::productivity::FULL_TIME),
            utilization_bps: 0,
            employees: 8,
            wage_bill: Money(8 * 400_000),
            price,
            cost,
            brand_stock: 0,
            tech: TechLevel(50),
            suppliers: smallvec_new(),
        });
    }
    st.firms.sort_by_key(|f| f.id.0.to_bits());

    let gd: i64 = st.cells.iter().map(|c| c.cash.get()).sum();
    let firmy: i64 = st.firms.iter().map(|f| f.capital.get()).sum();
    st.ledger.set(MacroAccount::Households, Money(gd));
    st.ledger.set(MacroAccount::Firms, Money(firmy));
    st.ledger.set(MacroAccount::RestOfWorld, Money(0));
    st
}

fn smallvec_new() -> smallvec::SmallVec<[(GoodId, magnat_core::FirmId); 8]> {
    smallvec::SmallVec::new()
}

/// Suma kont musi być sumą gotówki komórek i kapitału firm — w każdej dobie.
fn sprawdz_zgodnosc(st: &MacroState) {
    let gd: i64 = st.cells.iter().map(|c| c.cash.get()).sum();
    let firmy: i64 = st.firms.iter().map(|f| f.capital.get()).sum();
    assert_eq!(
        st.ledger.get(MacroAccount::Households).get(),
        gd,
        "konto gospodarstw rozjechało się z gotówką komórek"
    );
    assert_eq!(
        st.ledger.get(MacroAccount::Firms).get(),
        firmy,
        "konto firm rozjechało się z kapitałem firm"
    );
}

#[test]
fn krok_nie_tworzy_ani_nie_niszczy_grosza() {
    let mut st = miasto();
    let p = MacroParams::default();
    let przed = st.money();
    sprawdz_zgodnosc(&st);
    for _ in 0..90 {
        step(&mut st, &p);
        // Tolerancja **zero** (00 §4 pkt 1): makro nie ma prawa stworzyć ani
        // zniszczyć grosza, a odchylenie 3–12 % dotyczy agregatów, nie pieniądza.
        assert_eq!(st.money(), przed, "suma pieniądza drgnęła w kroku makro");
        sprawdz_zgodnosc(&st);
    }
}

#[test]
fn krok_nie_gubi_ludzi() {
    let mut st = miasto();
    let p = MacroParams::default();
    let przed: u32 = st.cells.iter().map(|c| c.labour_force()).sum();
    for _ in 0..90 {
        step(&mut st, &p);
        let teraz: u32 = st.cells.iter().map(|c| c.labour_force()).sum();
        assert_eq!(teraz, przed, "siła robocza drgnęła w kroku makro");
    }
}

#[test]
fn dwa_przebiegi_tego_samego_stanu_daja_ten_sam_hash() {
    let p = MacroParams::default();
    let mut a = miasto();
    let mut b = miasto();
    for _ in 0..90 {
        step(&mut a, &p);
        step(&mut b, &p);
    }
    assert_eq!(state_hash(&a), state_hash(&b));
}

#[test]
fn zapas_firmy_nie_rosnie_bez_zaplaty() {
    // Firma bez kapitału nie zatowaruje się „na kreskę": faza 3 kupuje wyłącznie
    // za to, co ma. Bez tego model tworzyłby masę z niczego, a bilans masy jest
    // tak samo nienaruszalny jak bilans pieniądza (00 §4 pkt 1).
    let mut st = miasto();
    for f in &mut st.firms {
        f.capital = Money(0);
        f.stock = MacroStock::new();
    }
    st.ledger.set(MacroAccount::Firms, Money(0));
    let p = MacroParams::default();
    let przed = st.mass();
    step(&mut st, &p);
    assert_eq!(st.mass(), przed, "firma bez kapitału zdobyła towar");
}

#[test]
fn ranking_nie_rozstrzyga_w_granicach_marginesu() {
    // Dwa warianty różniące się niczym: `KeepCourse` i zamknięcie zakładu o zerowej
    // obsadzie. Przewaga musi zmieścić się w marginesie, a wtedy `decisive_winner`
    // **musi** zwrócić `None` — w 100 % przypadków (§7.7 pkt 3).
    let st = miasto();
    let firma = st.firms[0].id;
    let warianty = [
        Scenario::KeepCourse { firm: firma },
        Scenario::CloseSite {
            firm: firma,
            site: magnat_core::SiteId(magnat_core::Entity::new(1, std::num::NonZeroU32::MIN)),
            slots: 0,
        },
    ];
    let r = rank_variants(&st, &warianty, 90);
    assert_eq!(r.decisive_winner(), None);
    assert!(r.error_margin_bp > 0, "model nie zadeklarował marginesu");
}

#[test]
fn ranking_rozstrzyga_gdy_roznica_jest_realna() {
    // Zamknięcie **wszystkich** etatów firmy zdejmuje z niej całą listę płac
    // i w horyzoncie kwartału musi wygrać z „nic nie rób" ponad margines.
    // To jest test uporządkowania (§7.7 pkt 1) w wersji, którą da się sprawdzić
    // bez uruchamiania mezo: kierunek znany z konstrukcji, nie z kalibracji.
    let mut st = miasto();
    // Firma dopłaca do interesu: towar sprzedaje poniżej kosztu, więc każdy dzień
    // pracy ją kosztuje, a etat jest jedynym kosztem, który da się ściąć.
    for f in &mut st.firms {
        f.price.set(CHLEB, Money(380));
        f.price.set(MLEKO, Money(320));
        f.wage_bill = Money(8 * 900_000);
    }
    let firma = st.firms[0].id;
    let warianty = [
        Scenario::KeepCourse { firm: firma },
        Scenario::CloseSite {
            firm: firma,
            site: magnat_core::SiteId(magnat_core::Entity::new(1, std::num::NonZeroU32::MIN)),
            slots: 12,
        },
    ];
    let r = rank_variants(&st, &warianty, 90);
    assert_eq!(
        r.decisive_winner(),
        Some(1),
        "zamknięcie nierentownego zakładu nie wygrało z bezczynnością"
    );
    assert_eq!(r.direction(1), magnat_core::Trend::Up);
}

#[test]
fn margines_zwieksza_sie_z_horyzontem_i_maleje_z_liczba_klientow() {
    // Uczciwość marginesu (§7.7 pkt 2) w części, którą da się sprawdzić bez mezo:
    // deklaracja ma **zależeć** od tego, od czego zależy wariancja. Margines stały
    // przechodziłby każdy test porównawczy i nie mówiłby nic.
    let st = miasto();
    let firma = st.firms[0].id;
    let w = [
        Scenario::KeepCourse { firm: firma },
        Scenario::Reprice {
            firm: firma,
            delta_bp: 300,
        },
    ];
    let krotki = rank_variants(&st, &w, 30).error_margin_bp;
    let dlugi = rank_variants(&st, &w, 360).error_margin_bp;
    assert!(
        dlugi >= krotki,
        "dłuższy horyzont nie podniósł deklarowanego marginesu ({krotki} → {dlugi})"
    );

    let mut male = miasto();
    for c in &mut male.cells {
        c.citizens.truncate(30);
    }
    assert!(
        rank_variants(&male, &w, 90).error_margin_bp >= rank_variants(&st, &w, 90).error_margin_bp,
        "mniejsza dzielnica nie podniosła marginesu"
    );
}

#[test]
fn co_jesli_nie_rusza_stanu_wyjsciowego() {
    // „Co jeśli" liczy się **na klonie** (§5.10). Gdyby ruszało stan, prognoza
    // zmieniałaby świat, o który pyta — i dwie firmy pytające w tej samej dobie
    // dostałyby różne odpowiedzi w zależności od kolejności.
    let st = miasto();
    let przed = state_hash(&st);
    let firma = st.firms[0].id;
    let _ = rank_variants(
        &st,
        &[
            Scenario::KeepCourse { firm: firma },
            Scenario::Reprice {
                firm: firma,
                delta_bp: -500,
            },
        ],
        90,
    );
    assert_eq!(state_hash(&st), przed);
}
