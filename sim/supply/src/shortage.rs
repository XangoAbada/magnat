//! Kaskada niedoboru jako maszyna stanów (M6b §5.7, WP6, PRD §8.4).
//!
//! Kolejność prób jest **dokładnie ta z PRD** i jest kontraktem, nie preferencją:
//! bufor → obniżenie produkcji → spot → import → substytut → postój. Substytut stoi
//! przed postojem, a nie przed importem, bo psuje jakość wyrobu — to jest przedostatnia
//! deska ratunku, nie pierwsza oszczędność.
//!
//! Kaskada jest **drabiną, nie tabelą progów**. Różnica jest widoczna i zamierzona:
//! tabela progów przeskoczyłaby z „bufor" prosto w „postój" i gracz nigdy nie zobaczyłby,
//! że zakład szukał na rynku spot i próbował importu. Drabina wchodzi o jeden szczebel
//! na przegląd, dopóki sytuacja się pogarsza — i dopiero to jest odpowiedź na pytanie
//! „co zrobiłeś, zanim stanąłeś".
//!
//! Zejście w dół działa odwrotnie: dostawa, która przyszła, zdejmuje zakład ze wszystkich
//! szczebli **naraz**. Odzyskiwanie po jednym szczeblu na godzinę trzymałoby zakład
//! w obniżonej produkcji długo po tym, jak problem minął — i to jest jedna z dwóch
//! rzeczy, które produkują efekt byczego bicza (`R3`).

use magnat_core::{
    DecisionReason, GoodId, HashState, Mass, ShortageStageKind, SimMinute, SiteId, StateHasher,
};

use crate::catalog::Catalog;
use crate::plant::PlantSite;
use crate::tuning::ShortageTuning;
use crate::Store;

/// Zapytanie ofertowe. Właścicielem typu jest **M6c** — tutaj jest, bo kaskada niesie
/// jego uchwyt, a M6b nie ma jeszcze rynku, który by go nadał. Ta sama droga, którą
/// M6a zadeklarował `LineId` i `TransportOrderId` na użytek M6b.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Default)]
pub struct RfqId(pub u32);

/// Stopień kaskady z ładunkiem. Sam stopień, bez ładunku, mieszka w `core`
/// ([`ShortageStageKind`]), bo niesie go `DecisionReason`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ShortageStage {
    Ok,
    /// Zużywaj rezerwę, alert w pulpicie.
    Buffer {
        coverage_min: u32,
    },
    /// Produkcja proporcjonalnie obniżona.
    Throttled {
        pct: u8,
    },
    /// Zapytanie ofertowe — drożej, ale szybciej niż import.
    SpotSearch {
        rfq: RfqId,
    },
    /// Import — wolniej, ale zwykle jest.
    Importing {
        eta: SimMinute,
    },
    /// Gorszy wyrób zamiast żadnego.
    Substituted {
        alt: GoodId,
        quality_loss: u8,
    },
    /// Linia `Starved`, koszty stałe lecą dalej.
    Halted {
        since: SimMinute,
    },
}

impl ShortageStage {
    #[must_use]
    pub const fn kind(self) -> ShortageStageKind {
        match self {
            ShortageStage::Ok => ShortageStageKind::Ok,
            ShortageStage::Buffer { .. } => ShortageStageKind::Buffer,
            ShortageStage::Throttled { .. } => ShortageStageKind::Throttled,
            ShortageStage::SpotSearch { .. } => ShortageStageKind::SpotSearch,
            ShortageStage::Importing { .. } => ShortageStageKind::Importing,
            ShortageStage::Substituted { .. } => ShortageStageKind::Substituted,
            ShortageStage::Halted { .. } => ShortageStageKind::Halted,
        }
    }

    #[must_use]
    pub fn rung(self) -> u8 {
        self.kind().as_index() as u8
    }
}

/// Stan kaskady dla jednej pary `(zakład, towar)`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct ShortageState {
    pub good: GoodId,
    pub stage: ShortageStage,
    pub since: SimMinute,
    /// Pokrycie zapasu w minutach z **poprzedniego** przeglądu — po nim poznaje się,
    /// czy sytuacja się pogarsza, a to jest warunek wejścia na kolejny szczebel.
    pub coverage_minutes: u32,
    /// Do ilu procent zeszła produkcja. 100 = pełna. Osobne pole, a nie ładunek
    /// stopnia, bo obniżenie **trwa** także wtedy, gdy zakład jest już szczebel wyżej
    /// i szuka na spocie (§5.7: „równolegle z Throttled").
    pub throttle_pct: u8,
}

impl HashState for ShortageState {
    fn hash_state(&self, h: &mut StateHasher) {
        h.write_u16(self.good.0);
        h.write_u8(self.stage.rung());
        match self.stage {
            ShortageStage::Buffer { coverage_min } => h.write_u32(coverage_min),
            ShortageStage::Throttled { pct } => h.write_u8(pct),
            ShortageStage::SpotSearch { rfq } => h.write_u32(rfq.0),
            ShortageStage::Importing { eta } => h.write_u64(eta.0),
            ShortageStage::Substituted { alt, quality_loss } => {
                h.write_u16(alt.0);
                h.write_u8(quality_loss);
            }
            ShortageStage::Halted { since } => h.write_u64(since.0),
            ShortageStage::Ok => {}
        }
        self.since.hash_state(h);
        h.write_u32(self.coverage_minutes);
        h.write_u8(self.throttle_pct);
    }
}

/// Co kaskada chce, żeby ktoś **kupił**. **Akcje, nie trait**: M6b nie ma rynku ani
/// węzła granicznego, a trait z jedną atrapą byłby abstrakcją bez drugiego konsumenta.
/// M6c czyta tę listę i zamienia ją na `Rfq` i `ImportQuote`; dopóki tego nie robi,
/// drabina po prostu wchodzi szczebel wyżej — czyli zachowuje się dokładnie tak,
/// jak ma się zachować spot bez wyniku.
///
/// **Substytucji tu nie ma i nie było czego tu robić** (`R2-WP14`). Wariant
/// `Substitute` istniał, `B2b::serve` miał dla niego puste ramię `match`, a wszystko,
/// co niósł — zamiennik i karę jakości — stopień `ShortageStage::Substituted` już
/// niesie. Podmiany się nie kupuje: wykonuje ją linia, sięgając po `RecipeInput.substitutes`
/// w `plant::produce`. Wariant bez wykonawcy wygląda tak samo jak wariant działający
/// i dlatego nie ma go w tym enumie.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ShortageAction {
    OpenRfq {
        site: SiteId,
        good: GoodId,
        mass: Mass,
    },
    Import {
        site: SiteId,
        good: GoodId,
        mass: Mass,
    },
}

/// Zużycie towaru przez zakład w gramach na minutę, przy pełnej produkcji.
///
/// Liczone z **linii**, nie z receptury: receptura daje proporcje, linia przepustowość
/// (§5.5). Zakład z dwoma młynami zjada dwa razy więcej zboża niż zakład z jednym,
/// choć receptura jest ta sama.
#[must_use]
pub fn consumption_per_minute(cat: &Catalog, site: &PlantSite, good: GoodId) -> i64 {
    let mut suma = 0i128;
    for l in &site.lines {
        let Some(rid) = l.recipe else { continue };
        let r = cat.recipe(rid);
        let Some(we) = r.inputs.iter().find(|i| i.good == good) else {
            continue;
        };
        if r.batch_mass.0 <= 0 {
            continue;
        }
        suma += i128::from(l.nominal_throughput.0) * i128::from(we.mass.0)
            / i128::from(r.batch_mass.0)
            / 60;
    }
    suma as i64
}

/// Pokrycie zapasu w minutach. `u32::MAX` znaczy „nikt tego nie zużywa", a nie
/// „mamy nieskończoność" — i dlatego nie jest zerem.
#[must_use]
pub fn coverage_minutes(cat: &Catalog, store: &Store, site: &PlantSite, good: GoodId) -> u32 {
    let rate = consumption_per_minute(cat, site, good);
    if rate <= 0 {
        return u32::MAX;
    }
    let stock = crate::plant::input_stock(store, site, good).0;
    (stock / rate).clamp(0, i64::from(u32::MAX)) as u32
}

/// Przegląd kaskady dla wszystkich wejść zakładu. Wołane `EveryHour`.
///
/// Zwraca listę akcji do wykonania przez rynek (M6c) i zapisuje `DecisionReason`
/// na **każdym** przejściu, także w dół — powrót do normy też jest odpowiedzią
/// na pytanie „co się stało z moją piekarnią".
pub fn review(
    cat: &Catalog,
    store: &Store,
    site: &mut PlantSite,
    now: SimMinute,
    t: &ShortageTuning,
) -> Vec<ShortageAction> {
    let mut akcje = Vec::new();
    let wejscia = wejscia_zakladu(cat, site);
    for good in wejscia {
        let cov = coverage_minutes(cat, store, site, good);
        if cov == u32::MAX {
            continue;
        }
        let poprzedni = site
            .shortage
            .iter()
            .position(|s| s.good == good)
            .map(|i| site.shortage[i]);
        let stan = poprzedni.unwrap_or(ShortageState {
            good,
            stage: ShortageStage::Ok,
            since: now,
            coverage_minutes: cov,
            throttle_pct: 100,
        });

        let pogarsza_sie = cov < stan.coverage_minutes;
        // Zejście wymaga **poprawy**, a nie samego braku pogorszenia. To nie jest
        // niuans: pokrycie stoi w miejscu także wtedy, gdy obniżona produkcja zrównała
        // się ze zużyciem — a to jest sytuacja, w której zakład dalej nie ma dostawy
        // i nie ma po co schodzić z drabiny. Bez tego rozróżnienia kaskada oscyluje
        // między importem a spotem zamiast dojść do postoju.
        let poprawia_sie = cov > stan.coverage_minutes;
        let dno = szczebel_progowy(cov, t);
        let teraz = stan.stage.rung();
        // Linia, która nie może **zacząć** szarży, zatrzymuje spadek pokrycia — zapas
        // przestaje schodzić, bo nikt go nie zużywa. Samo pokrycie wygląda wtedy na
        // ustabilizowane i drabina stanęłaby na imporcie na zawsze. Głodna linia jest
        // więc osobnym sygnałem: to ona, a nie liczba minut, mówi „zakład stoi".
        let glodna = site.lines.iter().any(
            |l| matches!(l.state, crate::plant::LineState::Starved { missing } if missing == good),
        );
        let cel = if dno < teraz && poprawia_sie {
            // Dostawa przyszła: schodzimy od razu, nie po jednym szczeblu.
            dno
        } else if pogarsza_sie || dno > teraz || glodna {
            teraz
                .saturating_add(1)
                .min(ShortageStageKind::Halted.as_index() as u8)
        } else {
            teraz
        };

        let alternatywa = substytut(cat, site, good);
        // `R2-WP14`: zakład, który jedzie na zamienniku, **nie stoi** — więc kaskada
        // nie ma po co wchodzić na `Halted`. Sufit działa tylko w górę: pierwotna
        // dostawa nadal ściąga drabinę w dół, a wyczerpanie zamiennika zdejmuje sufit
        // i postój przychodzi w następnej godzinie. Bez tego szczebel substytucji
        // byłby przejściem do postoju także wtedy, gdy podmiana faktycznie karmi linię —
        // bo pokrycie liczy się z **pierwotnego** wsadu i zostaje zerem.
        //
        // Rozstrzyga o tym **głodna linia, a nie sam stan magazynu zamiennika**.
        // „Jest cokolwiek" byłoby warunkiem fałszywym: jeden gram otrąb trzymałby
        // zakład na `Substituted` w nieskończoność, bo linia i tak stanęłaby na
        // `Starved`, a sufit ścinałby cel — czyli `Halted` stałby się nieosiągalny
        // i karta zakładu nigdy nie pokazałaby postoju stojącego zakładu.
        let podmiana_starcza = !glodna
            && alternatywa.is_some_and(|s| crate::plant::input_stock(store, site, s.good).0 > 0);
        let sufit = ShortageStageKind::Substituted.as_index() as u8;
        let cel = if podmiana_starcza {
            cel.min(sufit)
        } else {
            cel
        };
        let (nowy_stopien, akcja) = zbuduj(site.site, good, alternatywa, cel, cov, now, t);
        if let Some(a) = akcja {
            akcje.push(a);
        }
        let throttle = if nowy_stopien.rung() >= ShortageStageKind::Throttled.as_index() as u8 {
            obnizenie(cov, t)
        } else {
            100
        };

        if nowy_stopien.kind() != stan.stage.kind() {
            site.note(
                now,
                DecisionReason::Shortage {
                    good,
                    from: stan.stage.kind(),
                    to: nowy_stopien.kind(),
                    coverage_minutes: cov,
                },
            );
            if let ShortageStage::Substituted { alt, quality_loss } = nowy_stopien {
                site.note(
                    now,
                    DecisionReason::SubstituteUsed {
                        good,
                        alt,
                        quality_loss,
                    },
                );
            }
        }
        let nowy = ShortageState {
            good,
            stage: nowy_stopien,
            since: if nowy_stopien.kind() == stan.stage.kind() {
                stan.since
            } else {
                now
            },
            coverage_minutes: cov,
            throttle_pct: throttle,
        };
        match site.shortage.iter().position(|s| s.good == good) {
            Some(i) => site.shortage[i] = nowy,
            None => site.shortage.push(nowy),
        }
    }
    akcje
}

/// Wejścia wszystkich receptur ustawionych na liniach zakładu, bez powtórzeń,
/// w kolejności `GoodId` — porządek iteracji jest jawny (00 §3.2).
fn wejscia_zakladu(cat: &Catalog, site: &PlantSite) -> Vec<GoodId> {
    let mut v: Vec<GoodId> = site
        .lines
        .iter()
        .filter_map(|l| l.recipe)
        .flat_map(|r| cat.recipe(r).inputs.iter().map(|i| i.good))
        .collect();
    v.sort_unstable();
    v.dedup();
    v
}

/// Szczebel wynikający z samych progów — dno, poniżej którego drabina nie zejdzie.
fn szczebel_progowy(cov: u32, t: &ShortageTuning) -> u8 {
    if cov == 0 {
        ShortageStageKind::Halted.as_index() as u8
    } else if cov < t.importing_minutes {
        ShortageStageKind::SpotSearch.as_index() as u8
    } else if cov < t.throttled_minutes {
        ShortageStageKind::Throttled.as_index() as u8
    } else if cov < t.buffer_minutes {
        ShortageStageKind::Buffer.as_index() as u8
    } else {
        ShortageStageKind::Ok.as_index() as u8
    }
}

/// Obniżenie produkcji **proporcjonalne** do pokrycia, nie skokowe: zakład z zapasem
/// na trzy godziny pracuje wolniej niż ten z zapasem na cztery, a nie tak samo.
fn obnizenie(cov: u32, t: &ShortageTuning) -> u8 {
    if t.throttled_minutes == 0 {
        return 100;
    }
    let p = (u64::from(cov) * 100 / u64::from(t.throttled_minutes)) as u8;
    p.clamp(t.min_throughput_pct, 100)
}

/// Czym zakład może podmienić brakujące wejście.
///
/// Pytamy **recepturę**, nie towar: substytut jest właściwością procesu („ten piec
/// przyjmie otręby zamiast części mąki"), a nie towaru samego w sobie. Lista przy towarze
/// zostaje jako droga odwrotu dla wejść, których żadna receptura nie opisuje szczegółowo —
/// i dla M5, gdzie podmienia się to, co na półce, a nie to, co w recepturze.
pub(crate) fn substytut(
    cat: &Catalog,
    site: &PlantSite,
    good: GoodId,
) -> Option<crate::catalog::Substitute> {
    for l in &site.lines {
        let Some(rid) = l.recipe else { continue };
        if let Some(we) = cat.recipe(rid).inputs.iter().find(|i| i.good == good) {
            if let Some(s) = we.substitutes.first() {
                return Some(*s);
            }
        }
    }
    cat.good(good).substitutes.first().copied()
}

fn zbuduj(
    site: SiteId,
    good: GoodId,
    alternatywa: Option<crate::catalog::Substitute>,
    rung: u8,
    cov: u32,
    now: SimMinute,
    t: &ShortageTuning,
) -> (ShortageStage, Option<ShortageAction>) {
    let brakuje = Mass(i64::from(t.buffer_minutes) * 1_000);
    match ShortageStageKind::from_index(rung as usize) {
        Some(ShortageStageKind::Ok) | None => (ShortageStage::Ok, None),
        Some(ShortageStageKind::Buffer) => (ShortageStage::Buffer { coverage_min: cov }, None),
        Some(ShortageStageKind::Throttled) => (
            ShortageStage::Throttled {
                pct: obnizenie(cov, t),
            },
            None,
        ),
        Some(ShortageStageKind::SpotSearch) => (
            ShortageStage::SpotSearch {
                rfq: RfqId::default(),
            },
            Some(ShortageAction::OpenRfq {
                site,
                good,
                mass: brakuje,
            }),
        ),
        Some(ShortageStageKind::Importing) => (
            ShortageStage::Importing {
                // `ponytail:` czas dostawy importowej jest stałą, dopóki nie ma węzła
                // granicznego. Sufit nazwany: M6c/WP9 podmienia to na `lead_minutes`
                // z `TradeNode` razem z kolejką i ceną rosnącą z wolumenem.
                eta: SimMinute(now.0 + 8 * 60),
            },
            Some(ShortageAction::Import {
                site,
                good,
                mass: brakuje,
            }),
        ),
        Some(ShortageStageKind::Substituted) => match alternatywa {
            Some(s) => (
                ShortageStage::Substituted {
                    alt: s.good,
                    quality_loss: s.quality_penalty,
                },
                None,
            ),
            // Nie ma czym podmienić — szczebel po prostu nie istnieje dla tego towaru
            // i zakład staje. Udawanie substytucji byłoby gorsze niż postój.
            None => (ShortageStage::Halted { since: now }, None),
        },
        Some(ShortageStageKind::Halted) => (ShortageStage::Halted { since: now }, None),
    }
}
