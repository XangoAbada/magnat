//! Receptura — docelowy schemat (M6a §5.4) z modelem jakości i alokacją kosztu.

use magnat_core::{Energy, GoodId, LossKind, Mass, RecipeId, ResourceKind, Volume, Q};
use serde::Deserialize;

use super::good::{Substitute, SubstituteSpec};

/// Punkt wejścia domknięcia grafu produktów rozpoznawany **jawną flagą**, nie brakiem
/// wejść. Receptury `Extraction` i `Agriculture` są źródłami pierwotnymi — ich wejścia
/// materiałowe nie muszą być osiągalne, bo masa pochodzi ze złoża albo z gleby.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Deserialize)]
pub enum RecipeSource {
    /// Zwykła: wszystkie wejścia muszą być osiągalne.
    Manufacturing,
    /// Wymaga złoża pod parcelą (`K-13`, `TerrainQuery`).
    Extraction(ResourceKind),
    /// Wymaga gleby o wymaganej klasie.
    Agriculture,
}

impl RecipeSource {
    /// Czy receptura jest punktem wejścia domknięcia (KROK 1 domknięcia).
    #[must_use]
    pub const fn is_entry(self) -> bool {
        matches!(
            self,
            RecipeSource::Extraction(_) | RecipeSource::Agriculture
        )
    }
}

/// Klasa maszyny — **abstrakcyjna zdolność** („potrzebuję młyna walcowego"), nie obiekt
/// fizyczny. Receptura wymaga klasy, linia produkcyjna ją ma, archetyp budynku deklaruje,
/// ile slotów której klasy mieści hala (M6 §6.4.1).
///
/// Indeks w [`Catalog::machine_classes`](crate::Catalog::machine_classes), czyli w tablicy
/// **wyprowadzonej z receptur** przy ładowaniu, a nie w osobnym pliku danych.
///
/// `ponytail:` katalog klas maszyn nie istnieje, bo nie ma jeszcze czego w nim trzymać —
/// cena maszyny, jej pobór i decyzja inwestycyjna należą do M7. Sufit jest nazwany:
/// w chwili, w której klasa maszyny dostanie własne parametry, tablica przenosi się do
/// `data/machines/classes.ron`, a `MachineClassId` przestaje być pochodną receptur.
/// Do tego czasu drugi plik danych do utrzymania kosztowałby więcej, niż daje.
///
/// **Ścieżka wyjścia dostała adresata w R2e (`D-N16`, `R2-WP22`): M12d (modding).**
/// Powód jest ten, dla którego katalog w ogóle miałby powstać: klasa maszyny jest
/// dokładnie tym rodzajem rzeczy, którą modder chce dodać, a dziś nie może — nie
/// istnieje jako plik. Skrót bez adresata przeżył M7 i M10, choć obie miały go
/// domknąć; skrót z adresatem ma bramkę w rejestrze długu.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct MachineClassId(pub u16);

/// Rodzaj wyjścia szarży.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Deserialize)]
pub enum OutputKind {
    Main,
    ByProduct,
    /// Nie dostaje kosztu; **generuje** koszt utylizacji albo przychód, jeśli ma odbiorcę.
    Waste,
    /// Gaz opałowy spalany na miejscu: nie dostaje kosztu i **nie tworzy partii** —
    /// od razu zasila licznik energii zakładu.
    SelfConsumed,
}

/// Co widać nad kominem. Mieści się na 2 bitach (kontrakt z M11, M6 §6.4.3).
///
/// To **właściwość procesu deklarowana w danych**, a nie funkcja stosunku `pm_g` do
/// `co2_g`: czy z komina leci para czy sadza, wynika z tego, co się w środku dzieje,
/// i nie da się tego wiarygodnie odgadnąć z dwóch liczb — chłodnia kominowa i kotłownia
/// węglowa potrafią mieć zbliżone `co2_g` przy zupełnie różnym pióropuszu.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Deserialize)]
#[repr(u8)]
pub enum PlumeKind {
    /// Montownia, szwalnia, magazyn.
    #[default]
    None = 0,
    /// Biała para: chłodnia kominowa, elektrownia, mleczarnia, browar.
    Steam = 1,
    /// Ciemna sadza: huta, koksownia, cementownia, kotłownia węglowa.
    Soot = 2,
    /// Opary: rafineria, zakład chemiczny, papiernia.
    Chemical = 3,
}

/// Przezbrojenie z innej receptury: kosztuje czas, masę i energię.
#[derive(Clone, Copy, Debug, Default, Deserialize)]
pub struct Setup {
    pub minutes: u32,
    #[serde(default)]
    pub scrap_mass_g: i64,
    #[serde(default)]
    pub energy_wh: i64,
}

impl Setup {
    #[must_use]
    pub const fn scrap_mass(self) -> Mass {
        Mass(self.scrap_mass_g)
    }

    #[must_use]
    pub const fn energy(self) -> Energy {
        Energy(self.energy_wh)
    }
}

/// Emisje szarży.
#[derive(Clone, Copy, Debug, Default, Deserialize)]
pub struct Emissions {
    #[serde(default)]
    pub noise_db: u8,
    #[serde(default)]
    pub pm_g: i32,
    #[serde(default)]
    pub co2_g: i32,
    #[serde(default)]
    pub wastewater_ml: i64,
    #[serde(default)]
    pub plume: PlumeKind,
}

impl Emissions {
    #[must_use]
    pub const fn wastewater(self) -> Volume {
        Volume(self.wastewater_ml)
    }
}

/// Jakość wyjścia jako **parametry, nie domknięcie** — domknięcie nie jest ani
/// serializowalne, ani moddowalne z poziomu danych; parametry są jedno i drugie.
#[derive(Clone, Copy, Debug, Deserialize)]
pub struct QualityModel {
    pub base: u8,
    pub w_input: u8,
    pub w_skill: u8,
    pub w_tech: u8,
    pub w_machine: u8,
    /// Dobra mąka nie powstanie ze słabego zboża.
    #[serde(default)]
    pub cap_by_worst_input: bool,
    /// Kontrola jakości: `+q` kosztem osobominut.
    #[serde(default)]
    pub bonus_qc: u8,
}

impl Default for QualityModel {
    fn default() -> QualityModel {
        QualityModel {
            base: 50,
            w_input: 40,
            w_skill: 20,
            w_tech: 15,
            w_machine: 10,
            cap_by_worst_input: false,
            bonus_qc: 0,
        }
    }
}

impl QualityModel {
    /// Suma wag. Walidator wymaga `<= 100` — inaczej zakład o średniej obsadzie
    /// wypuszczałby towar lepszy od wszystkiego, co do niego weszło.
    #[must_use]
    pub const fn weight_sum(&self) -> u32 {
        self.w_input as u32 + self.w_skill as u32 + self.w_tech as u32 + self.w_machine as u32
    }

    /// Jakość wyjścia. Cała arytmetyka na `i32`, jedno dzielenie z zaokrągleniem
    /// połówek od zera na końcu — funkcja **czysta**, ten sam wsad daje tę samą jakość.
    ///
    /// `q_in` to średnia ważona masą jakości wejść (substytuty już po karze),
    /// `q_worst` to najgorsze wejście, `skill` średnia ważona umiejętności obsady zmiany,
    /// `tech` poziom technologii zakładu (do M10 stałe 50), `condition` stan maszyny.
    #[must_use]
    pub fn quality(&self, q_in: Q, q_worst: Q, skill: Q, tech: Q, condition: Q) -> Q {
        let suma = i32::from(self.w_input) * i32::from(q_in.get())
            + i32::from(self.w_skill) * i32::from(skill.get())
            + i32::from(self.w_tech) * i32::from(tech.get())
            + i32::from(self.w_machine) * i32::from(condition.get());
        let q_raw = i32::from(self.base) + div_round_half_away(suma, 100);
        let mut q_out = q_raw.clamp(0, 100);
        if self.cap_by_worst_input {
            let sufit = i32::from(q_worst.get()) + i32::from(self.bonus_qc);
            q_out = q_out.min(sufit);
        }
        Q::new(q_out.clamp(0, 100) as u8)
    }
}

/// Dzielenie całkowite z zaokrągleniem połówek **od zera** — ta sama konwencja co
/// w `Money::div_round_half_up`, żeby jakość i pieniądz nie zaokrąglały się inaczej.
fn div_round_half_away(a: i32, b: i32) -> i32 {
    debug_assert!(b > 0);
    let ujemne = a < 0;
    let a_abs = i64::from(a).unsigned_abs();
    let b_abs = i64::from(b).unsigned_abs();
    let q = (a_abs * 2 + b_abs) / (b_abs * 2);
    let q = q as i32;
    if ujemne {
        -q
    } else {
        q
    }
}

/// Jak rozdzielić koszt szarży między wyjścia.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Deserialize)]
pub enum CostAllocation {
    /// Domyślnie, gdy jest jedno wyjście `Main`.
    #[default]
    ByMass,
    /// Wg `external_base_price` wyjść — obowiązkowe dla produkcji łącznej.
    /// Rozdzielenie wg masy dałoby asfalt droższy od benzyny, czyli absurd,
    /// który wywraca ceny w całym łańcuchu w dół.
    ByMarketValue,
}

/// Wejście receptury.
#[derive(Clone, Debug)]
pub struct RecipeInput {
    pub good: GoodId,
    pub mass: Mass,
    pub min_quality: Q,
    pub substitutes: Vec<Substitute>,
    /// Brak = stop. Niekrytyczne wejście obniża tylko jakość.
    pub critical: bool,
}

/// Wyjście receptury.
#[derive(Clone, Copy, Debug)]
pub struct RecipeOutput {
    pub good: GoodId,
    pub mass: Mass,
    pub kind: OutputKind,
}

/// Receptura z kluczami rozwiązanymi na identyfikatory.
#[derive(Clone, Debug)]
pub struct Recipe {
    pub key: Box<str>,
    pub id: RecipeId,
    pub source: RecipeSource,
    pub inputs: Vec<RecipeInput>,
    pub outputs: Vec<RecipeOutput>,
    /// Nominalny wsad jednej szarży.
    pub batch_mass: Mass,
    pub duration_minutes: u32,
    /// Ubytek procesowy — domyka bilans masy. Musi mieć kategorię: masa znikająca
    /// bez kategorii jest błędem, nie zaokrągleniem (M6 §7.3 pkt 1).
    pub process_loss: Mass,
    pub loss_kind: LossKind,
    pub energy: Energy,
    pub water: Volume,
    /// **Osobominuty szarży**, nie obsada etatowa. Obsada jest w archetypie budynku.
    /// Klucze ról, nie `JobRoleId` — katalog etatów mieszka po stronie miasta.
    pub labour: Vec<(Box<str>, u32)>,
    /// Abstrakcyjna zdolność maszynowa — receptura nie zna obiektu fizycznego.
    pub machine_class: Box<str>,
    pub setup: Setup,
    pub emissions: Emissions,
    pub quality: QualityModel,
    pub cost_allocation: CostAllocation,
}

impl Recipe {
    /// Dobowa wydajność receptury dla towaru `g`, w gramach przy skali bazowej.
    /// `yield` nie jest polem — liczy się tutaj, z mas i czasu szarży, żeby nie mógł
    /// się z nimi rozjechać.
    #[must_use]
    pub fn daily_yield(&self, g: GoodId) -> i64 {
        let masa = self
            .outputs
            .iter()
            .find(|o| o.good == g)
            .map_or(0, |o| o.mass.0);
        masa.saturating_mul(1440) / i64::from(self.duration_minutes.max(1))
    }

    /// Dobowe zapotrzebowanie na wejście `g`, w gramach przy skali bazowej.
    #[must_use]
    pub fn daily_input(&self, g: GoodId) -> i64 {
        let masa = self
            .inputs
            .iter()
            .find(|i| i.good == g)
            .map_or(0, |i| i.mass.0);
        masa.saturating_mul(1440) / i64::from(self.duration_minutes.max(1))
    }

    /// Suma mas wejść szarży — **razem z wodą z licznika**.
    ///
    /// Woda jest medium licznikowym, nie towarem w partii (`D9` fazy, wpisane w M6b):
    /// piekarni nikt nie dowozi wody ciężarówką. Ma za to masę i ta masa wychodzi
    /// z pieca jako chleb, więc musi wejść do bilansu — inaczej reguła
    /// `Σ wejść == Σ wyjść + ubytek` nie domknęłaby się dla żadnej z dwudziestu
    /// jeden receptur, które wody używają. Gęstość 1 g/ml, więc mililitr **jest**
    /// gramem i przeliczenie nie ma gdzie zgubić reszty.
    #[must_use]
    pub fn input_mass(&self) -> Mass {
        Mass(self.inputs.iter().map(|i| i.mass.0).sum::<i64>() + self.water.0)
    }

    /// Suma mas wyjść szarży — razem z odpadem i tym, co zakład spala u siebie.
    #[must_use]
    pub fn output_mass(&self) -> Mass {
        Mass(self.outputs.iter().map(|o| o.mass.0).sum())
    }
}

// ── Postać z pliku danych ────────────────────────────────────────────────────────────

#[derive(Clone, Debug, Deserialize)]
pub struct RecipeInputSpec {
    pub good: String,
    pub mass_g: i64,
    #[serde(default)]
    pub min_quality: u8,
    #[serde(default)]
    pub substitutes: Vec<SubstituteSpec>,
    #[serde(default = "prawda")]
    pub critical: bool,
}

#[derive(Clone, Debug, Deserialize)]
pub struct RecipeOutputSpec {
    pub good: String,
    pub mass_g: i64,
    #[serde(default = "main")]
    pub kind: OutputKind,
}

const fn prawda() -> bool {
    true
}

const fn main() -> OutputKind {
    OutputKind::Main
}

const fn loss_process_waste() -> LossKind {
    LossKind::ProcessWaste
}

#[derive(Clone, Debug, Deserialize)]
pub struct RecipeSpec {
    pub key: String,
    pub source: RecipeSource,
    #[serde(default)]
    pub inputs: Vec<RecipeInputSpec>,
    pub outputs: Vec<RecipeOutputSpec>,
    #[serde(default)]
    pub process_loss_g: i64,
    #[serde(default = "loss_process_waste")]
    pub loss_kind: LossKind,
    #[serde(default)]
    pub batch_mass_g: i64,
    pub duration_minutes: u32,
    #[serde(default)]
    pub energy_wh: i64,
    #[serde(default)]
    pub water_ml: i64,
    #[serde(default)]
    pub labour: Vec<(String, u32)>,
    #[serde(default)]
    pub machine_class: String,
    #[serde(default)]
    pub setup: Setup,
    #[serde(default)]
    pub emissions: Emissions,
    #[serde(default)]
    pub quality: QualityModel,
    #[serde(default)]
    pub cost_allocation: CostAllocation,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Liczby z M6 §7.1, etap 5 (młyn): zboże q64, obsada skill 60, tech 50,
    /// maszyna condition 85 → **mąka q67**, bo sufit najgorszego wejścia + `bonus_qc`
    /// przycina wynik. To jest sedno modelu: dobra mąka nie powstanie ze słabego zboża.
    #[test]
    fn mlyn_daje_q67_bo_sufit_wejscia_przycina() {
        let m = QualityModel {
            base: 50,
            w_input: 40,
            w_skill: 20,
            w_tech: 15,
            w_machine: 10,
            cap_by_worst_input: true,
            bonus_qc: 3,
        };
        let q = m.quality(Q::new(64), Q::new(64), Q::new(60), Q::new(50), Q::new(85));
        assert_eq!(q, Q::new(67));
    }

    /// Bez sufitu ten sam wsad wypada ponad skalę i przycina się do 100 — dowód,
    /// że to sufit wejścia, a nie clamp, robi robotę w teście wyżej.
    #[test]
    fn bez_sufitu_wsad_wychodzi_ponad_skale() {
        let m = QualityModel {
            cap_by_worst_input: false,
            ..QualityModel::default()
        };
        assert_eq!(
            m.quality(Q::new(64), Q::new(64), Q::new(60), Q::new(50), Q::new(85)),
            Q::MAX
        );
    }

    #[test]
    fn jakosc_jest_funkcja_czysta() {
        let m = QualityModel::default();
        let a = m.quality(Q::new(41), Q::new(30), Q::new(55), Q::new(50), Q::new(70));
        let b = m.quality(Q::new(41), Q::new(30), Q::new(55), Q::new(50), Q::new(70));
        assert_eq!(a, b);
    }

    #[test]
    fn zaokraglenie_polowek_od_zera() {
        assert_eq!(div_round_half_away(50, 100), 1);
        assert_eq!(div_round_half_away(49, 100), 0);
        assert_eq!(div_round_half_away(-50, 100), -1);
    }
}
