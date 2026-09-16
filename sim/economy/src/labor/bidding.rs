//! Licytacja płac i headhunting (M7b WP5, M7 §5.5 pkt 1–2).
//!
//! **Tu powstaje pensja.** Nie ma tabeli, z której by się ją brało: oferta wisi,
//! nikt nie przychodzi, firma dokłada — i dokłada dopóty, dopóki etat jeszcze się
//! opłaca. Sufit liczy `magnat_firms::next_bid` z widełek stanowiska, a nie z rynku,
//! bo hamulec wywiedziony z rynku nie hamuje spirali, tylko w niej płynie (`R1`).

use magnat_core::{DecisionReason, Money, SimMinute, Tick, WageCause};
use magnat_firms::{next_bid, Firms};

use super::offer::{JobOffer, JobOfferId};
use super::{LaborDay, LaborMarket, Workforce};

/// Agresja licytacyjna firmy, 0..=100.
///
/// Wywodzi ją z osobowości dyrektora M7e; do tego czasu całe miasto licytuje tak samo
/// i **to jest właściwy stan przejściowy**: gdyby M7b zgadywał osobowość, M7e musiałby
/// najpierw odgadnięcie usunąć.
const AGGRESSION: u8 = 0;

/// Zapas marży, o który firma wolno przekracza kraniec widełek. Hak zerowy do M7e —
/// `SitePnlMonth` nie ma jeszcze przychodu, więc marży nie ma z czego policzyć (`AR-7`).
const MARGIN_HEADROOM_BP: i32 = 0;

/// Wygaszenie ofert przeterminowanych i podbicie tych, które wiszą bez kandydata (WP5).
pub(super) fn expire_and_escalate(
    m: &mut LaborMarket,
    firms: &mut Firms,
    now: SimMinute,
    d: &mut LaborDay,
) {
    let co_ile = m.tuning.search.escalate_after_days.max(1);
    let mut wygasle: Vec<JobOfferId> = Vec::new();
    // Plan najpierw, zapis potem: podbicie stawki zapisuje powód do dziennika firmy,
    // a ten siedzi w `Firms` — pożyczka areny i rejestru naraz nie przejdzie.
    let mut podbicia: Vec<(JobOfferId, magnat_firms::Bid, u16)> = Vec::new();
    let odnowienie = u64::from(m.tuning.search.offer_days_valid) * 1440;
    for (id, o) in m.offers.iter_mut() {
        if now.0 >= o.expires.0 {
            // Oferta **bezpośrednia** ginie: to była propozycja złożona konkretnemu
            // człowiekowi i albo ją przyjął, albo nie.
            if o.targeted.is_some() {
                wygasle.push(id);
                continue;
            }
            // Ogłoszenie na nadal otwarty wakat firma **odnawia**, a nie wystawia
            // od nowa. Różnica jest w licytacji i jest zasadnicza: oferta postawiona
            // od zera wraca na dolny kraniec widełek, więc dwa tygodnie podbijania
            // stawki przepadają i rynek pracy nigdy nie wychodzi z miejsca.
            o.expires = SimMinute(now.0 + odnowienie);
        }
        o.days_open = o.days_open.saturating_add(1);
        if o.days_open % co_ile != 0 {
            continue;
        }
        let niedobor = m.stats.shortage_index(o.role, o.district);
        let bid = next_bid(
            o.wage_month,
            o.band,
            niedobor,
            AGGRESSION,
            MARGIN_HEADROOM_BP,
            &m.tuning.wage,
        );
        podbicia.push((id, bid, o.days_open));
    }

    for (id, bid, dni) in podbicia {
        let Some(o) = m.offers.get_mut(id) else {
            continue;
        };
        if bid.delta_bp == 0 {
            // Sufit. Pieniędzy już nie ma, ale świadczenie pozapłacowe jeszcze jest —
            // i to jest realna decyzja firmy, a nie obejście: kandydat dostaje opiekę
            // medyczną zamiast podwyżki, a firma płaci za nią mniej niż za stawkę.
            dolozy_swiadczenie(o);
            if !o.frozen {
                o.frozen = true;
                firms.log(
                    o.firm,
                    Tick(now.0),
                    DecisionReason::WageRaise {
                        role: o.role,
                        delta_bp: 0,
                        days_open: dni,
                        cause: WageCause::Ceiling,
                    },
                );
                d.frozen += 1;
            }
            continue;
        }
        let rola = o.role;
        let firma = o.firm;
        o.wage_month = bid.wage;
        o.raises = o.raises.saturating_add(1);
        firms.log(
            firma,
            Tick(now.0),
            DecisionReason::WageRaise {
                role: rola,
                delta_bp: bid.delta_bp,
                days_open: dni,
                cause: bid.cause,
            },
        );
        d.raises += 1;
    }

    d.expired += wygasle.len() as u32;
    for id in wygasle {
        m.offers.remove(id);
    }
    m.index.mark_dirty();
}

/// Świadczenie zamiast podwyżki, gdy stawka stoi na suficie.
///
/// Kolejność jest stała i **jest** kolejnością kosztu: najtańsze najpierw. Oferta,
/// która ma już wszystko, zostaje bez zmian — i wtedy wakat po prostu wisi.
fn dolozy_swiadczenie(o: &mut JobOffer) {
    use magnat_firms::BenefitSet;
    for flaga in [
        BenefitSet::MEALS,
        BenefitSet::HEALTH,
        BenefitSet::TRAINING,
        BenefitSet::COMPANY_CAR,
    ] {
        if !o.benefits.has(flaga) {
            o.benefits = BenefitSet(o.benefits.0 | flaga);
            return;
        }
    }
}

/// Headhunting: oferta bezpośrednia do zatrudnionego u konkurencji (WP5, §5.5 pkt 2).
///
/// Wchodzi dopiero powyżej progu niedoboru, bo przeciąganie ludzi jest kosztowne
/// i **ma boleć** tego, kto je zaczyna: premia wychodzi z jego kieszeni. Adresat ocenia
/// ofertę zwykłą regułą zmiany pracy — ambicja obniża próg, lojalność go podnosi.
pub(super) fn headhunt(
    m: &mut LaborMarket,
    firms: &Firms,
    people: &impl Workforce,
    now: SimMinute,
    d: &mut LaborDay,
) {
    let prog = m.tuning.wage.headhunt_shortage;
    let premia = m.tuning.wage.headhunt_premium_bp;
    // Zawody i dzielnice, w których w ogóle warto kogoś przeciągać.
    let gorace: Vec<(magnat_core::JobRoleId, magnat_core::DistrictId)> = m
        .stats
        .per_role
        .iter()
        .filter(|(_, s)| s.shortage_index >= prog && s.vacancies > 0)
        .map(|(k, _)| *k)
        .collect();
    if gorace.is_empty() {
        return;
    }
    let mut wyslane: Vec<JobOffer> = Vec::new();
    // Jedna oferta bezpośrednia na firmę na dobę. Bez tego limitu firma z pięcioma
    // wakatami rozsyła pięć propozycji dziennie i przeciąganie pracownika przestaje
    // być decyzją, a staje się tłem — zmierzone: 15 tys. ofert bezpośrednich
    // na 90 dób w mieście 4 km, czyli jedna na każdego zatrudnionego.
    let mut firmy_dzis: std::collections::BTreeSet<magnat_firms::FirmKey> =
        std::collections::BTreeSet::new();
    // Adresaci wybrani w tym przebiegu — indeks jeszcze ich nie zna, a dwie oferty
    // do jednego człowieka byłyby licytacją firm o kogoś, kto i tak pójdzie do jednej.
    let mut juz: std::collections::BTreeSet<magnat_core::CitizenId> =
        std::collections::BTreeSet::new();
    for (role, district) in gorace {
        // Kto już gdzieś pracuje w tym zawodzie i w tej dzielnicy. Kolejność po
        // `(SiteId, pozycja w obsadzie)`, czyli deterministyczna z definicji.
        let mut zatrudnieni: Vec<(magnat_core::CitizenId, magnat_firms::FirmKey, Money)> =
            Vec::new();
        let mut wakaty: Vec<(magnat_core::SiteId, magnat_firms::FirmKey, (Money, Money))> =
            Vec::new();
        for (id, site) in firms.sites() {
            if site.district != district {
                continue;
            }
            for p in &site.positions {
                if p.role != role {
                    continue;
                }
                if p.vacancies() > 0 {
                    wakaty.push((id, site.firm, p.wage_band));
                }
                for e in &p.filled {
                    zatrudnieni.push((e.citizen, site.firm, e.wage_month));
                }
            }
        }
        for (site, firma, band) in wakaty {
            if firmy_dzis.contains(&firma) {
                continue;
            }
            // Przeciąganie jest **drugim** ruchem, nie pierwszym: firma sięga po nie
            // dopiero wtedy, gdy podniosła już stawkę w zwykłym ogłoszeniu i to nie
            // wystarczyło. Inaczej płaciłaby premię za kogoś, kogo i tak by dostała.
            if m.index
                .at_position(site, role)
                .and_then(|id| m.offers.get(id))
                .is_none_or(|o| o.raises == 0)
            {
                continue;
            }
            let Some((c, _, obecna)) = zatrudnieni
                .iter()
                .copied()
                // Nie podbieramy własnym ludziom i nie wysyłamy drugiej oferty
                // do kogoś, kto dziś już jedną dostał.
                .find(|(c, f, _)| *f != firma && !m.index.is_targeted(*c) && !juz.contains(c))
            else {
                continue;
            };
            if people.facts(c).is_none() {
                continue;
            }
            let proponowana =
                Money(obecna.get().saturating_mul(i64::from(10_000 + premia)) / 10_000);
            // Sufit widełek obowiązuje także tutaj: przeciąganie nie jest wyjątkiem
            // od rentowności, tylko innym sposobem jej wydania.
            let sufit = magnat_firms::wage_ceiling(band, MARGIN_HEADROOM_BP);
            if proponowana.get() > sufit.get() {
                continue;
            }
            let Some(zrodlo) = firms.site(site) else {
                continue;
            };
            let Some(p) = zrodlo.positions.iter().find(|p| p.role == role) else {
                continue;
            };
            juz.insert(c);
            firmy_dzis.insert(firma);
            wyslane.push(JobOffer {
                firm: firma,
                site,
                role,
                district,
                wage_month: proponowana,
                slots: 1,
                shift: magnat_agents::ShiftKind::Day,
                requirements: Default::default(),
                benefits: magnat_firms::BenefitSet::NONE,
                targeted: Some(c),
                posted: now,
                // Oferta bezpośrednia żyje jedną dobę: to propozycja złożona
                // konkretnemu człowiekowi, a nie ogłoszenie wiszące na rynku.
                expires: SimMinute(now.0 + 1440),
                days_open: 0,
                raises: 0,
                frozen: false,
                applicants: 0,
                band: p.wage_band,
            });
        }
    }
    d.headhunts += wyslane.len() as u32;
    for o in wyslane {
        m.post_offer(o);
    }
    m.index.rebuild(&m.offers);
}
