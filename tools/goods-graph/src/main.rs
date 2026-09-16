//! `goods-graph` — walidator grafu produktów i generator szkieletów towarów (M6a WP1).
//!
//! Walidator jest **testem CI od M2** (dok. 00 §5) i od M6a mieszka tutaj, a nie w crate'cie
//! miasta: pyta o katalog, nie o miasto, więc nie ma powodu, żeby do jego uruchomienia
//! trzeba było zbudować generator terenu.
//!
//! Reguły 1–4 są błędami i kończą proces kodem 1. Reguła 5 to ostrzeżenia — wypisują się
//! i **nie** przewracają przebiegu, bo opisują kruchość, a nie błąd. `--deny-warnings`
//! zmienia to dla tych, którzy chcą katalogu bez ani jednego pojedynczego źródła.

use clap::{Parser, Subcommand};
use magnat_supply::{Catalog, GoodForm, StorageClass};

#[derive(Parser)]
#[command(name = "goods-graph", about = "Walidator grafu produktów")]
struct Cli {
    /// Epoka, której koszyk potrzeb wczytać.
    #[arg(long, default_value = "contemporary", global = true)]
    epoch: String,
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Waliduje katalog z `data/` i wypisuje raport.
    Check {
        /// Ostrzeżenie reguły 5 też kończy niepowodzeniem.
        #[arg(long)]
        deny_warnings: bool,
    },
    /// Wypisuje szkielet nowego towaru na wzór tych, które już są w jego kategorii,
    /// po czym waliduje katalog — żeby od razu było widać, że towar bez źródła nie przejdzie.
    New {
        /// Klucz w konwencji `<domena>_<nazwa>[_<wariant>]`.
        key: String,
        /// Klucz kategorii potrzeby z `data/needs/categories.ron`.
        #[arg(long)]
        category: String,
    },
}

fn main() -> std::process::ExitCode {
    let cli = Cli::parse();
    let cat = match magnat_supply::catalog::load_default(&cli.epoch) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("BŁĄD: {e}");
            return std::process::ExitCode::FAILURE;
        }
    };

    match cli.cmd {
        Cmd::Check { deny_warnings } => check(&cat, deny_warnings),
        Cmd::New { key, category } => nowy(&cat, &key, &category),
    }
}

fn check(cat: &Catalog, deny_warnings: bool) -> std::process::ExitCode {
    println!(
        "katalog: {} towarów, {} receptur, {} kategorii",
        cat.goods.len(),
        cat.recipes.len(),
        cat.categories.len()
    );
    // Reguły 1–4 przeszły już przy ładowaniu — `load_default` nie zwraca katalogu,
    // który ich nie spełnia. Zostaje reguła 5.
    let ostrzezenia = cat.warnings();
    for w in &ostrzezenia {
        println!("ostrzeżenie: {w}");
    }
    if ostrzezenia.is_empty() {
        println!("graf domknięty, bez ostrzeżeń");
    } else {
        println!("graf domknięty, {} ostrzeżeń", ostrzezenia.len());
    }
    if deny_warnings && !ostrzezenia.is_empty() {
        std::process::ExitCode::FAILURE
    } else {
        std::process::ExitCode::SUCCESS
    }
}

fn nowy(cat: &Catalog, key: &str, category: &str) -> std::process::ExitCode {
    let Some(c) = cat.category_id(category) else {
        eprintln!("BŁĄD: nieznana kategoria `{category}`");
        return std::process::ExitCode::FAILURE;
    };
    if cat.good_id(key).is_some() {
        eprintln!("BŁĄD: towar `{key}` już jest w katalogu");
        return std::process::ExitCode::FAILURE;
    }

    // Szablon bierze się z towarów, które w tej kategorii już stoją — pierwszy wg `GoodId`,
    // czyli alfabetycznie. Jeśli kategoria jest pusta, szablonem jest towar sypki
    // w warunkach otoczenia; to jest zgadywanie i tak ma być oznaczone.
    let wzor = cat.in_category(c).first().map(|g| cat.good(*g));
    let (form, gestosc, storage) = wzor
        .map_or((GoodForm::Bulk, 1000, StorageClass::Ambient), |g| {
            (g.form, g.density_g_per_l.max(1), g.storage)
        });

    println!("// szkielet z szablonu kategorii `{category}`");
    if form.is_bulk() {
        println!(
            "        (key: \"{key}\", category: \"{category}\", form: {form:?}, density_g_per_l: {gestosc}, storage: {storage:?}, external_base_price: Some((0))),"
        );
    } else {
        let masa = wzor.map_or(1000, |g| g.unit_mass.0.max(1));
        let obj = wzor.map_or(1000, |g| g.unit_volume.0.max(1));
        println!(
            "        (key: \"{key}\", category: \"{category}\", form: {form:?}, unit_mass_g: {masa}, unit_volume_ml: {obj}, storage: {storage:?}, external_base_price: Some((0))),"
        );
    }
    println!("// Towar wchodzi do repo RAZEM ze swoim źródłem — recepturą, złożem albo");
    println!("// wpisem w `TradeNode`. Commit z sierotą nie przechodzi walidacji.");
    check(cat, false)
}
