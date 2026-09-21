//! Powody decyzji firmy w języku gracza (`R2-WP20`).
//!
//! Pięćdziesiąt jeden powodów: cena, zapas, produkcja, kadry, finanse, marka,
//! badania, giełda i spór zbiorowy. Największy z trzech plików i to jest
//! spodziewane — firma ma najwięcej rodzajów decyzji.
//!
//! Osobny plik, bo `describe` rosła liniowo z liczbą faz i miała 1290 linii przy
//! progu błędu kontroli strukturalnej wynoszącym 250. Po podziale dopisanie powodu
//! przez fazę dotykającą firm nie powiększa pliku, który obsługuje mieszkańców.
//!
//! **Wyczerpujący `match` bez ramienia `_`** — `K-12` obowiązuje tutaj tak samo jak
//! przed podziałem, tylko na jednym aktorze zamiast na wszystkich naraz. Wariant bez
//! zdania łamie kompilację; pilnuje tego dodatkowo `tests/ui/firm_reason_bez_wildcard.rs`.

use super::*;

#[must_use]
#[allow(clippy::too_many_lines)]
pub(super) fn opisz(c: &Catalog, l: Locale, r: FirmReason, n: Names<'_>) -> String {
    match r {
        FirmReason::Repricing {
            site: _,
            good: _,
            driver,
            delta_bp,
        } => c.fmt_key(
            l,
            "ui.reason.Repricing",
            &[
                ("czlon", &price_driver(c, l, driver)),
                ("roznica", &procent_bp(i32::from(delta_bp))),
            ],
        ),
        FirmReason::Shortage {
            good: _,
            from,
            to,
            coverage_minutes,
        } => c.fmt_key(
            l,
            "ui.reason.Shortage",
            &[
                ("z", &shortage_stage(c, l, from)),
                ("na", &shortage_stage(c, l, to)),
                ("pokrycie", &godziny(coverage_minutes)),
            ],
        ),
        FirmReason::ProductionHalted {
            site: _,
            line,
            cause,
        } => c.fmt_key(
            l,
            "ui.reason.ProductionHalted",
            &[
                ("linia", &(u32::from(line) + 1).to_string()),
                ("powod", &line_stop_cause(c, l, cause)),
            ],
        ),
        FirmReason::SubstituteUsed {
            good: _,
            alt: _,
            quality_loss,
        } => c.fmt_key(
            l,
            "ui.reason.SubstituteUsed",
            &[("strata", &quality_loss.to_string())],
        ),
        FirmReason::SupplierChosen {
            good: _,
            seller: _,
            quotes,
            saving_bp,
        } => c.fmt_key(
            l,
            "ui.reason.SupplierChosen",
            &[
                ("ofert", &quotes.to_string()),
                ("przewaga", &procent_bp(i32::from(saving_bp))),
            ],
        ),
        FirmReason::ContractSigned {
            good: _,
            seller: _,
            months,
            indexed,
        } => c.fmt_key(
            l,
            "ui.reason.ContractSigned",
            &[
                ("miesiecy", &months.to_string()),
                ("cennik", &contract_pricing(c, l, indexed)),
            ],
        ),
        FirmReason::ExportChosen {
            good: _,
            premium_bp,
            mass_kg,
        } => c.fmt_key(
            l,
            "ui.reason.ExportChosen",
            &[
                ("przewaga", &procent_bp(i32::from(premium_bp))),
                ("masa", &mass_kg.to_string()),
            ],
        ),
        FirmReason::Hired {
            role: _,
            score,
            runner_up,
        } => c.fmt_key(
            l,
            "ui.reason.Hired",
            &[
                ("wynik", &score.to_string()),
                ("drugi", &drugi_w_kolejce(c, l, runner_up)),
            ],
        ),
        // Podwyżka przycięta do zera jest **innym zdaniem**, a nie tym samym z liczbą
        // zero: firma nie podniosła stawki, bo przy wyższej ten etat przestaje się
        // opłacać. Gracz pytający „dlaczego wakat stoi pusty" dostaje tu odpowiedź.
        FirmReason::WageRaise {
            role: _,
            delta_bp: 0,
            days_open,
            cause: _,
        } => c.fmt_key(
            l,
            "ui.reason.WageFrozen",
            &[(
                "dni",
                &c.plural_key(l, "ui.unit.days", u64::from(days_open)),
            )],
        ),
        FirmReason::WageRaise {
            role: _,
            delta_bp,
            days_open,
            cause,
        } => c.fmt_key(
            l,
            "ui.reason.WageRaise",
            &[
                ("przyrost", &procent_bp(i32::from(delta_bp))),
                (
                    "dni",
                    &c.plural_key(l, "ui.unit.days", u64::from(days_open)),
                ),
                ("powod", &wage_cause(c, l, cause)),
            ],
        ),
        FirmReason::JobLeft {
            role: _,
            cause,
            tenure_days,
        } => c.fmt_key(
            l,
            "ui.reason.JobLeft",
            &[
                ("powod", &leave_cause(c, l, cause)),
                (
                    "staz",
                    &c.plural_key(l, "ui.unit.days", u64::from(tenure_days)),
                ),
            ],
        ),
        // Reguła zapasowa („INACZEJ" z gramatyki M9d) jest **innym zdaniem**, a nie
        // regułą numer 255: gracz pytający „która reguła to zrobiła" ma usłyszeć,
        // że nie pasowała żadna, a nie zobaczyć numer, którego nie ma na liście.
        FirmReason::PolicyApplied {
            policy,
            rule: u8::MAX,
            action,
            lag_days,
            deviation_bp,
        } => c.fmt_key(
            l,
            "ui.reason.PolicyFallback",
            &[
                ("polityka", &policy.get().to_string()),
                ("akcja", &action_kind(c, l, action)),
                ("menedzer", &reka_menedzera(c, l, lag_days, deviation_bp)),
            ],
        ),
        FirmReason::PolicyApplied {
            policy,
            rule,
            action,
            lag_days,
            deviation_bp,
        } => c.fmt_key(
            l,
            "ui.reason.PolicyApplied",
            &[
                ("polityka", &policy.get().to_string()),
                // Reguły numeruje się dla gracza od jedynki — w edytorze M9 są
                // wierszami listy, a pierwszy wiersz nie jest wierszem zerowym.
                ("regula", &(u16::from(rule) + 1).to_string()),
                ("akcja", &action_kind(c, l, action)),
                ("menedzer", &reka_menedzera(c, l, lag_days, deviation_bp)),
            ],
        ),
        FirmReason::ManagerAssigned {
            site: _,
            skill_mgmt,
            prev,
        } => c.fmt_key(
            l,
            "ui.reason.ManagerAssigned",
            &[
                ("umiejetnosc", &skill_mgmt.get().to_string()),
                ("poprzednia", &prev.to_string()),
            ],
        ),
        FirmReason::LoanTaken {
            kind,
            rate_bp,
            term_months,
        } => c.fmt_key(
            l,
            "ui.reason.LoanTaken",
            &[
                ("produkt", &loan_kind(c, l, kind)),
                ("oprocentowanie", &procent_bp(i32::from(rate_bp))),
                ("okres", &months(c, l, u32::from(term_months))),
            ],
        ),
        FirmReason::LeaseSigned { site: _, months: m } => c.fmt_key(
            l,
            "ui.reason.LeaseSigned",
            &[("okres", &months(c, l, u32::from(m)))],
        ),
        FirmReason::ReceivablesFactored { count, discount_bp } => c.fmt_key(
            l,
            "ui.reason.ReceivablesFactored",
            &[
                ("ile", &count.to_string()),
                ("dyskonto", &procent_bp(i32::from(discount_bp))),
            ],
        ),
        FirmReason::BondIssued {
            coupon_bp,
            months: m,
        } => c.fmt_key(
            l,
            "ui.reason.BondIssued",
            &[
                ("kupon", &procent_bp(i32::from(coupon_bp))),
                ("okres", &months(c, l, u32::from(m))),
            ],
        ),
        // Niewypłacalność mierzona czasem czyta się inaczej niż ta mierzona bilansem:
        // „nie płaci od trzech miesięcy" i „ma więcej długów niż majątku" to dwa
        // różne zdania o firmie, a gracz zadaje o nie dwa różne pytania.
        FirmReason::BankruptcyOpened {
            trigger: BankruptcyTrigger::Illiquid,
            days: d,
        } => c.fmt_key(
            l,
            "ui.reason.BankruptcyIlliquid",
            &[("dni", &days(c, l, u32::from(d)))],
        ),
        FirmReason::BankruptcyOpened { trigger, days: _ } => c.fmt_key(
            l,
            "ui.reason.BankruptcyOpened",
            &[("powod", &bankruptcy_trigger(c, l, trigger))],
        ),
        FirmReason::ClaimSettled { priority, ratio_bp } => c.fmt_key(
            l,
            "ui.reason.ClaimSettled",
            &[
                ("grupa", &claim_priority(c, l, priority)),
                ("stopien", &procent_bp(i32::from(ratio_bp))),
            ],
        ),
        // ── M7e: AI firm ─────────────────────────────────────────────────────────
        // Towar znowu jest w ładunku, a nie w zdaniu — z tego samego powodu co w M6:
        // `GoodId` rozwiązuje katalog z `sim/supply`, którego `engine/ui` nie widzi.
        FirmReason::MarginTargetSet {
            good: _,
            margin_bp,
            prev_bp,
        } => c.fmt_key(
            l,
            "ui.reason.MarginTargetSet",
            &[
                ("cel", &procent_bp(margin_bp)),
                ("poprzednio", &procent_bp(prev_bp)),
            ],
        ),
        FirmReason::RestockTargetSet {
            good: _,
            days: d,
            prev,
        } => c.fmt_key(
            l,
            "ui.reason.RestockTargetSet",
            &[
                ("cel", &days(c, l, u32::from(d))),
                ("poprzednio", &days(c, l, u32::from(prev))),
            ],
        ),
        FirmReason::SiteClosed {
            months: m,
            margin_bp,
        } => c.fmt_key(
            l,
            "ui.reason.SiteClosed",
            &[
                ("okres", &months(c, l, u32::from(m))),
                ("marza", &procent_bp(margin_bp)),
            ],
        ),
        FirmReason::StrategySet { strategy, prev } => c.fmt_key(
            l,
            "ui.reason.StrategySet",
            &[
                ("kurs", &firm_strategy(c, l, strategy)),
                ("poprzednio", &firm_strategy(c, l, prev)),
            ],
        ),
        FirmReason::CompetitiveResponse {
            kind,
            target: _,
            depth_bp,
        } => c.fmt_key(
            l,
            "ui.reason.CompetitiveResponse",
            &[
                ("odpowiedz", &reaction_kind(c, l, kind)),
                ("koszt", &procent_bp(i32::from(depth_bp))),
            ],
        ),
        // **Tu nie ma kwoty i nie może jej być** (§5.10, `R15`). Decyzja stoi na
        // porównaniu wariantów w modelu makro, a ten deklaruje własny błąd 3–12 %.
        // Gracz dostaje więc to, co model faktycznie wie: z ilu wariantów rywal
        // wybierał, jak szeroki był margines i w którą stronę szedł wynik.
        FirmReason::SiteOpened {
            district: _,
            variants,
            margin_bp,
            trend: kierunek,
        } => c.fmt_key(
            l,
            "ui.reason.SiteOpened",
            &[
                ("warianty", &u32::from(variants).to_string()),
                ("margines", &procent_bp(i32::from(margin_bp))),
                ("kierunek", &trend(c, l, kierunek)),
            ],
        ),
        FirmReason::VoluntaryClosure { months: m, cash } => c.fmt_key(
            l,
            "ui.reason.VoluntaryClosure",
            &[
                ("okres", &months(c, l, u32::from(m))),
                ("kasa", &crate::zlotowki(cash)),
            ],
        ),
        FirmReason::FirmFounded { score, capital } => c.fmt_key(
            l,
            "ui.reason.FirmFounded",
            &[
                ("ocena", &u32::from(score).to_string()),
                ("kapital", &crate::zlotowki(capital)),
            ],
        ),
        FirmReason::ChainEntered { capital, sites } => c.fmt_key(
            l,
            "ui.reason.ChainEntered",
            &[
                ("kapital", &crate::zlotowki(capital)),
                ("zaklady", &u32::from(sites).to_string()),
            ],
        ),
        FirmReason::CampaignBacked {
            candidate,
            amount,
            illegal,
        } => c.fmt_key(
            l,
            if illegal {
                "ui.reason.CampaignBackedIllegal"
            } else {
                "ui.reason.CampaignBacked"
            },
            &[
                ("kandydat", &format!("{}", candidate + 1)),
                ("kwota", &crate::zlotowki(amount)),
            ],
        ),
        FirmReason::AdCampaignStarted {
            brand,
            channel,
            budget,
            claim,
        } => c.fmt_key(
            l,
            "ui.reason.AdCampaignStarted",
            &[
                ("marka", &marka(brand)),
                ("kanal", &ad_channel(c, l, channel)),
                ("budzet", &crate::zlotowki(budget)),
                ("obietnica", &format!("{}", claim.get())),
            ],
        ),
        FirmReason::StoryPublished {
            outlet,
            event,
            bias,
            reach_bp,
        } => c.fmt_key(
            l,
            "ui.reason.StoryPublished",
            &[
                ("tytul", &marka(outlet)),
                ("linia", &editorial_bias(c, l, bias)),
                ("zdarzenie", &format!("{}", event.get())),
                ("zasieg", &procent(u32::from(reach_bp))),
            ],
        ),
        // ── M10c: R&D i nowe produkty ────────────────────────────────────────────
        // Towar znowu jest w ładunku, a nie w zdaniu, i po raz pierwszy tak samo
        // jest z technologią: `TechId` rozwiązuje `data/tech/`, którego `engine/ui`
        // nie widzi — ta sama granica co przy `GoodId` od M6.
        FirmReason::ResearchStarted {
            tech,
            cost_rp,
            months_est,
        } => c.fmt_key(
            l,
            "ui.reason.ResearchStarted",
            &[
                ("technologia", &technologia(n, tech)),
                ("koszt", &format!("{cost_rp}")),
                ("miesiecy", &months(c, l, u32::from(months_est))),
            ],
        ),
        FirmReason::TechDiscovered {
            tech,
            patented,
            rp_spent,
            months,
        } => c.fmt_key(
            l,
            if patented {
                "ui.reason.TechDiscoveredPatent"
            } else {
                "ui.reason.TechDiscoveredOpen"
            },
            &[
                ("technologia", &technologia(n, tech)),
                ("punkty", &format!("{rp_spent}")),
                ("miesiecy", &self::months(c, l, u32::from(months))),
            ],
        ),
        FirmReason::LicenseSigned {
            tech,
            licensor: _,
            royalty_bp,
        } => c.fmt_key(
            l,
            "ui.reason.LicenseSigned",
            &[
                ("technologia", &technologia(n, tech)),
                ("oplata", &procent_bp(i32::from(royalty_bp))),
            ],
        ),
        FirmReason::ProductLaunched {
            good: _,
            tech,
            shops,
        } => c.fmt_key(
            l,
            "ui.reason.ProductLaunched",
            &[
                ("technologia", &technologia(n, tech)),
                ("sklepow", &format!("{shops}")),
            ],
        ),
        // ── M10d: giełda, przejęcia, ubezpieczenia ───────────────────────────────
        // Kurs jest ceną **jednego punktu bazowego** udziału, więc razy 10 000 daje
        // wycenę całej firmy. Karta pokazuje obie liczby, bo pierwsza jest ceną,
        // którą się płaci, a druga — jedyną, która cokolwiek znaczy dla gracza.
        FirmReason::StockListed { firm: _, price } => c.fmt_key(
            l,
            "ui.reason.StockListed",
            &[
                ("cena", &crate::zlotowki(price)),
                ("wycena", &crate::zlotowki(wycena_firmy(price))),
            ],
        ),
        FirmReason::StockFixing { firm: _, price } => c.fmt_key(
            l,
            "ui.reason.StockFixing",
            &[
                ("cena", &crate::zlotowki(price)),
                ("wycena", &crate::zlotowki(wycena_firmy(price))),
            ],
        ),
        // Posiadacz jest osobą albo firmą i to rozstrzyga **klucz**, a nie wstawka —
        // ta sama droga, którą `TechDiscovered` rozdziela patent od wiedzy w obiegu.
        FirmReason::StakeDisclosed { holder } => c.fmt_key(
            l,
            if holder.kind() == magnat_core::SubjectKind::Firm {
                "ui.reason.StakeDisclosedFirm"
            } else {
                "ui.reason.StakeDisclosedPerson"
            },
            &[("posiadacz", &podmiot(holder))],
        ),
        FirmReason::ControlAcquired { holder } => c.fmt_key(
            l,
            if holder.kind() == magnat_core::SubjectKind::Firm {
                "ui.reason.ControlAcquiredFirm"
            } else {
                "ui.reason.ControlAcquiredPerson"
            },
            &[("posiadacz", &podmiot(holder))],
        ),
        FirmReason::DividendPaid { firm: _, total } => c.fmt_key(
            l,
            "ui.reason.DividendPaid",
            &[("kwota", &crate::zlotowki(total))],
        ),
        FirmReason::SharesIssued { bp, price } => c.fmt_key(
            l,
            "ui.reason.SharesIssued",
            &[
                ("udzial", &procent_bp(i32::from(bp))),
                ("cena", &crate::zlotowki(price)),
            ],
        ),
        FirmReason::PerilStruck {
            peril,
            district,
            loss,
        } => c.fmt_key(
            l,
            "ui.reason.PerilStruck",
            &[
                ("ryzyko", &peril_kind(c, l, peril)),
                ("dzielnica", &district.0.to_string()),
                ("strata", &crate::zlotowki(loss)),
            ],
        ),
        FirmReason::Underwritten {
            peril,
            rate_bp,
            premium,
        } => c.fmt_key(
            l,
            "ui.reason.Underwritten",
            &[
                ("ryzyko", &peril_kind(c, l, peril)),
                ("stawka", &procent_bp(i32::from(rate_bp))),
                ("skladka", &crate::zlotowki(premium)),
            ],
        ),
        FirmReason::ClaimPaid { insurer: _, paid } => c.fmt_key(
            l,
            "ui.reason.ClaimPaid",
            &[("kwota", &crate::zlotowki(paid))],
        ),
        // ── M10e: relacje, kartele, związki ──────────────────────────────────────
        // Towar jest w ładunku, ale nie w zdaniu: `GoodId` rozwiązuje katalog
        // z `sim/supply`, którego `engine/ui` nie widzi — ta sama granica co przy
        // `SupplierChosen` od M6.
        FirmReason::TrustedSupplier {
            supplier: _,
            trust,
            discount_bp,
        } => c.fmt_key(
            l,
            "ui.reason.TrustedSupplier",
            &[
                ("zaufanie", &format!("{}", trust.get())),
                ("przewaga", &procent_bp(i32::from(discount_bp))),
            ],
        ),
        FirmReason::CartelFormed {
            good: _,
            members,
            floor,
        } => c.fmt_key(
            l,
            "ui.reason.CartelFormed",
            &[
                ("firm", &format!("{members}")),
                ("cena", &crate::zlotowki(floor)),
            ],
        ),
        FirmReason::CartelDetected {
            good: _,
            members,
            months,
        } => c.fmt_key(
            l,
            "ui.reason.CartelDetected",
            &[
                ("firm", &format!("{members}")),
                ("miesiecy", &months.to_string()),
            ],
        ),
        FirmReason::BrandScandal { brand, drop } => c.fmt_key(
            l,
            "ui.reason.BrandScandal",
            &[("marka", &marka(brand)), ("spadek", &format!("{drop}"))],
        ),
        FirmReason::UnionFormed { density, grievance } => c.fmt_key(
            l,
            "ui.reason.UnionFormed",
            &[
                ("gestosc", &format!("{}", density.get())),
                ("zal", &format!("{}", grievance.get())),
            ],
        ),
        FirmReason::WageDemandMade { raise_bp, anchor } => c.fmt_key(
            l,
            "ui.reason.WageDemandMade",
            &[
                ("podwyzka", &procent_bp(i32::from(raise_bp))),
                ("odniesienie", &crate::zlotowki(anchor)),
            ],
        ),
        FirmReason::StrikeStarted {
            participation_bp,
            round,
        } => c.fmt_key(
            l,
            "ui.reason.StrikeStarted",
            &[
                ("udzial", &procent_bp(i32::from(participation_bp))),
                ("runda", &format!("{round}")),
            ],
        ),
        // Zero podwyżki znaczy kapitulację, a nie brak pomiaru — i to są dwa różne
        // zdania dla gracza, więc rozstrzyga **klucz**, a nie wstawka.
        FirmReason::StrikeEnded { days, raise_bp } => c.fmt_key(
            l,
            if raise_bp == 0 {
                "ui.reason.StrikeEndedLost"
            } else {
                "ui.reason.StrikeEndedWon"
            },
            &[
                ("dni", &days.to_string()),
                ("podwyzka", &procent_bp(i32::from(raise_bp))),
            ],
        ),
    }
}
