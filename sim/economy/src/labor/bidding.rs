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

/// Agresja licytacyjna zakładu, 0..=100 (M7c, `AV-6`).
///
/// Do M7c była stałą zerową i całe miasto licytowało tak samo. Od M7c wychodzi ze
/// stylu kierowania, a **od M7e — z osobowości firmy**, gdy menedżera nie ma.
/// Kolejność jest treścią, nie wygodą: zakładem kieruje ten, kto go prowadzi, więc
/// menedżer nadpisuje dyrektora. Zakład firmy, której nikt nie zna, licytuje jej
/// charakterem, a nie zerem.
fn aggression(firms: &Firms, site: magnat_core::SiteId) -> u8 {
    if let Some(s) = firms.style_of(site) {
        return magnat_firms::ManagerStyle::aggression(s);
    }
    firms
        .site(site)
        .and_then(|s| firms.get(s.firm))
        .map_or(0, |f| f.personality.aggression_bp())
}

/// Zapas marży, o który firma wolno przekracza kraniec widełek (M7e, `AU-4`).
///
/// **Tu kończy się hak zerowy z M7b.** Do M7e `SitePnlMonth` nie miał przychodu,
/// więc marży nie było z czego policzyć i sufitem licytacji był sam kraniec widełek
/// roli — czyli „ile ta praca jest warta w tej dzielnicy". Od M7e zakład z księgą
/// mówi też, **na ile go stać**: zarabiający dobrze wolno przepłacić, zakład na
/// granicy nie może.
///
/// Zakład bez pomiaru (produkcyjny, świeżo otwarty) zostaje przy zerze i to nadal
/// jest właściwy stan: brak księgi nie jest zerową marżą.
///
/// Zakres jest przycięty do ±2000 bp, a nie do pełnego ±5000, na które pozwala
/// `wage_ceiling`. Powód jest ten sam, dla którego cierpliwość nie rozciąga progu
/// zamknięcia zakładu: sufit płacowy jest **jedynym** hamulcem spirali (`R1`),
/// więc marża może go przesunąć, ale nie może go znieść.
fn margin_headroom_bp(firms: &Firms, site: magnat_core::SiteId) -> i32 {
    firms
        .site(site)
        .and_then(|s| s.pnl.last())
        .and_then(magnat_firms::SitePnlMonth::margin_bp)
        .map_or(0, |m| m.clamp(-2_000, 2_000))
}

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
            aggression(firms, o.site),
            margin_headroom_bp(firms, o.site),
            &m.tuning.wage,
        );
        podbicia.push((id, bid, o.days_open));
    }

    for (id, bid, dni) in podbicia {
        let kolejnosc = m
            .offers
            .get(id)
            .and_then(|o| firms.style_of(o.site))
            .map_or(
                DOMYSLNA_KOLEJNOSC,
                magnat_firms::ManagerStyle::benefit_order,
            );
        let Some(o) = m.offers.get_mut(id) else {
            continue;
        };
        if bid.delta_bp == 0 {
            // Sufit. Pieniędzy już nie ma, ale świadczenie pozapłacowe jeszcze jest —
            // i to jest realna decyzja firmy, a nie obejście: kandydat dostaje opiekę
            // medyczną zamiast podwyżki, a firma płaci za nią mniej niż za stawkę.
            dolozy_swiadczenie(o, kolejnosc);
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

/// Kolejność świadczeń zakładu bez menedżera: kolejność **kosztu**, najtańsze najpierw.
const DOMYSLNA_KOLEJNOSC: [u8; 4] = [
    magnat_firms::BenefitSet::MEALS,
    magnat_firms::BenefitSet::HEALTH,
    magnat_firms::BenefitSet::TRAINING,
    magnat_firms::BenefitSet::COMPANY_CAR,
];

/// Świadczenie zamiast podwyżki, gdy stawka stoi na suficie.
///
/// Do M7c kolejność była sztywna i była kolejnością kosztu. Od M7c jest **decyzją
/// menedżera** (`AV-6`): trener zaczyna od szkoleń, handlowiec od auta służbowego.
/// Zestaw jest ten sam u każdego — styl przestawia priorytet, a nie odbiera świadczenie.
/// Oferta, która ma już wszystko, zostaje bez zmian i wtedy wakat po prostu wisi.
fn dolozy_swiadczenie(o: &mut JobOffer, kolejnosc: [u8; 4]) {
    use magnat_firms::BenefitSet;
    for flaga in kolejnosc {
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
        let mut wakaty: Vec<(
            magnat_core::SiteId,
            magnat_firms::FirmKey,
            (Money, Money),
            bool,
        )> = Vec::new();
        for (id, site) in firms.sites() {
            if site.district != district {
                continue;
            }
            for p in &site.positions {
                if p.role != role {
                    continue;
                }
                if p.vacancies() > 0 {
                    wakaty.push((id, site.firm, p.wage_band, p.managerial));
                }
                for e in &p.filled {
                    zatrudnieni.push((e.citizen, site.firm, e.wage_month));
                }
            }
        }
        for (site, firma, band, kierownicze) in wakaty {
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
            // Kampania przeciągania (M7e WP14) celuje w **konkretnego** rywala
            // i płaci za to więcej. Bez kampanii firma bierze pierwszego z brzegu
            // i to jest zwykły headhunting z M7b.
            let kampania = firms.get(firma).and_then(|f| f.campaign).filter(|k| {
                k.kind == magnat_core::ReactionKind::Poach
                    && k.role == role
                    && k.active(Tick(now.0))
            });
            let wolny = |c: &magnat_core::CitizenId, f: &magnat_firms::FirmKey| {
                *f != firma && !m.index.is_targeted(*c) && !juz.contains(c)
            };
            // Kampania celuje w **zakład** rywala (`K-46`), a załoga jest tu spisana
            // po firmach — firmę zakładu odczytuje się z rejestru, nigdy odwrotnie.
            let cel_firmy = kampania.and_then(|k| firms.site(k.target).map(|s| s.firm));
            let Some((c, _, obecna)) = cel_firmy
                .and_then(|cel| {
                    zatrudnieni
                        .iter()
                        .copied()
                        .find(|(c, f, _)| *f == cel && wolny(c, f))
                })
                // Nie podbieramy własnym ludziom i nie wysyłamy drugiej oferty
                // do kogoś, kto dziś już jedną dostał.
                .or_else(|| zatrudnieni.iter().copied().find(|(c, f, _)| wolny(c, f)))
            else {
                continue;
            };
            if people.facts(c).is_none() {
                continue;
            }
            // **Dobrego menedżera podkupuje się drożej** (M7c §5.4): stanowisko
            // kierownicze dostaje własną, wyższą premię. Bez tego rozdziału jakość
            // zarządzania byłaby zasobem rzadkim, o który nikt nie konkuruje —
            // a wtedy „menedżer jako zasób" jest opisem, nie mechaniką.
            // `max`, a nie podstawienie: premia kierownicza ma być **nie mniejsza**
            // od zwykłej, a nie „inna". Podstawienie znaczyłoby, że przestawienie
            // `wage.headhunt_premium_bp` w balansatorze ponad wartość menedżerską
            // czyni menedżerów najtańszymi do podkupienia — czyli odwraca zdanie,
            // które ta gałąź ma wypowiadać.
            let premia = if kierownicze {
                premia.max(m.tuning.manager.headhunt_premium_bp)
            } else {
                premia
            };
            // Nadpłata kampanii dokłada się do premii, a nie ją zastępuje: firma,
            // która ogłosiła przeciąganie, płaci **ponad** to, co i tak by zapłaciła.
            let premia = premia.saturating_add(kampania.map_or(0, |k| i32::from(k.depth_bp)));
            let proponowana =
                Money(obecna.get().saturating_mul(i64::from(10_000 + premia)) / 10_000);
            // Sufit widełek obowiązuje także tutaj: przeciąganie nie jest wyjątkiem
            // od rentowności, tylko innym sposobem jej wydania.
            let sufit = magnat_firms::wage_ceiling(band, margin_headroom_bp(firms, site));
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
