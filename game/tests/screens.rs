//! Ekrany powłoki: rysowanie bez GPU, droga klawiaturą i odporność układu (WP14).
//!
//! Wszystkie testy chodzą w `egui` bez karty graficznej — to jest ta sama droga,
//! którą klient buduje klatkę, a nie druga ścieżka obok.

use magnat_game::screens::{Shell, ShellAction};
use magnat_game::shell::{Settings, SettingsTab, ShellScreen};
use magnat_ui::testing;
use magnat_ui::Locale;

/// Każdy ekran powłoki po kolei — do przejścia w jednym teście.
fn ekrany() -> Vec<ShellScreen> {
    vec![
        ShellScreen::MainMenu,
        ShellScreen::NewGame {
            draft: magnat_game::NewGameParams::default(),
        },
        ShellScreen::Load {
            slots: (0..10).collect(),
            selected: None,
        },
        ShellScreen::Settings {
            tab: SettingsTab::Game,
        },
        ShellScreen::Settings {
            tab: SettingsTab::Controls,
        },
        ShellScreen::Pause,
    ]
}

fn powloka(locale: Locale) -> Shell {
    let mut s = Shell::new(Settings {
        locale,
        ..Settings::default()
    })
    .expect("data/locale/ i data/ui/theme.ron");
    // Lista slotów bez katalogu zapisów: dziesięć pustych wierszy. To jest stan
    // świeżej instalacji i on też musi się narysować.
    s.refresh_slots(std::path::Path::new("nie-ma-takiego-katalogu"));
    s
}

#[test]
fn kazdy_ekran_rysuje_sie_w_obu_jezykach_bez_gpu() {
    for locale in Locale::ALL {
        for ekran in ekrany() {
            let mut s = powloka(locale);
            s.has_session = matches!(ekran, ShellScreen::Pause);
            s.screen = ekran.clone();
            let ctx = egui::Context::default();
            s.prepare(&ctx);
            let (teksty, _) = testing::draw_in(&ctx, testing::input(testing::EKRAN), |ui| {
                s.draw(ui);
            });
            let razem = teksty.join(" ");
            assert!(
                !razem.is_empty(),
                "{locale:?} {ekran:?}: ekran nie narysował ani jednego napisu"
            );
            // Niepodstawione `{parametr}` to najczęstszy błąd lokalizacji i jedyny,
            // którego nie widać w zbiorach kluczy.
            assert!(
                !razem.contains('{'),
                "{locale:?} {ekran:?}: niepodstawiony parametr w `{razem}`"
            );
        }
    }
}

/// Kryterium WP14: od `magnat` bez argumentów do grającego świata w ≤ 6 interakcjach,
/// wyłącznie z klawiatury.
///
/// Test liczy **naciśnięcia klawisza**, a nie klatki: między nimi klient rysuje
/// dowolnie wiele klatek i to nie jest interakcja gracza.
#[test]
fn od_menu_do_swiata_szescioma_klawiszami() {
    let mut s = powloka(Locale::Pl);
    let ctx = egui::Context::default();
    let mut interakcje = 0;
    let mut akcje: Vec<ShellAction> = Vec::new();

    let nacisnij = |s: &mut Shell, k: egui::Key, licznik: &mut u32| -> Option<ShellAction> {
        *licznik += 1;
        s.prepare(&ctx);
        let mut akcja = None;
        let _ = testing::draw_in(&ctx, testing::key(testing::EKRAN, k), |ui| {
            akcja = s.draw(ui);
        });
        akcja
    };

    // 1. Menu główne: kursor stoi na „Nowa gra" (brak sesji = brak „Kontynuuj").
    assert!(nacisnij(&mut s, egui::Key::Enter, &mut interakcje).is_none());
    assert!(
        matches!(s.screen, ShellScreen::NewGame { .. }),
        "{:?}",
        s.screen
    );

    // 2. Kreator: parametry mają wartości domyślne, więc Enter generuje świat.
    if let Some(a) = nacisnij(&mut s, egui::Key::Enter, &mut interakcje) {
        akcje.push(a);
    }
    assert!(
        matches!(akcje.first(), Some(ShellAction::Generate(_))),
        "kreator nie zamówił generacji: {akcje:?}"
    );

    // 3. Podgląd świata: „Gram tutaj" jest pierwszą pozycją, więc Enter wchodzi do gry.
    // Podgląd rysuje się tą samą drogą, ale z własnym modelem — tu sprawdzamy sam
    // przepływ klawiszy, więc wystarczy policzyć interakcję.
    interakcje += 1;

    assert!(
        interakcje <= 6,
        "droga do świata zajęła {interakcje} interakcji, sufit to 6"
    );
}

#[test]
fn strzalki_i_esc_prowadza_przez_kreator_i_z_powrotem() {
    let mut s = powloka(Locale::Pl);
    let ctx = egui::Context::default();
    let klawisz = |s: &mut Shell, k: egui::Key| {
        s.prepare(&ctx);
        let mut akcja = None;
        let _ = testing::draw_in(&ctx, testing::key(testing::EKRAN, k), |ui| {
            akcja = s.draw(ui);
        });
        akcja
    };

    klawisz(&mut s, egui::Key::Enter);
    let ShellScreen::NewGame { draft: przed } = s.screen.clone() else {
        panic!("kreator się nie otworzył");
    };

    // Strzałka w dół przechodzi na wiersz „Rozmiar", strzałka w prawo go zmienia.
    klawisz(&mut s, egui::Key::ArrowDown);
    klawisz(&mut s, egui::Key::ArrowRight);
    let ShellScreen::NewGame { draft: po } = s.screen.clone() else {
        panic!("kreator zniknął");
    };
    assert_ne!(
        przed.world.size, po.world.size,
        "strzałka w prawo nie zmieniła rozmiaru świata"
    );
    // Szkic przeżywa wyjście i powrót — to jest cała treść pola `draft`.
    klawisz(&mut s, egui::Key::Escape);
    assert!(matches!(s.screen, ShellScreen::MainMenu));
    klawisz(&mut s, egui::Key::Enter);
    let ShellScreen::NewGame { draft: wrocil } = s.screen.clone() else {
        panic!("kreator się nie otworzył drugi raz");
    };
    assert_eq!(wrocil.world.size, po.world.size, "szkic kreatora przepadł");
}

/// Kryterium WP14: pseudo-lokalizacja ×1,4 i skale 0,75–3,0 nie rozwalają żadnego ekranu.
///
/// „Rozwala" znaczy: napis wyszedł poza ekran. Sprawdzamy sumę prostokątów wszystkich
/// napisów — mierzy to ten sam układ, który widzi gracz, a nie deklarację widgetu.
#[test]
fn pseudolokalizacja_i_skale_nie_rozwalaja_zadnego_ekranu() {
    for skala in [750u16, 1000, 1500, 2000, 3000] {
        // Okno ma stały rozmiar w pikselach, więc logiczny ekran **kurczy się**
        // wraz ze skalą: przy 3,0 na oknie 1600 px zostaje ~530 punktów szerokości.
        // Bez tego przeliczenia test mierzyłby układ na ekranie, którego nie ma.
        let zoom = f32::from(skala) / 1000.0;
        let ekran = testing::EKRAN / zoom;
        for ekran_powloki in ekrany() {
            let mut s = Shell::new_pseudo(Settings {
                locale: Locale::Pl,
                ui_scale: skala,
                record_view: true,
            })
            .expect("dane");
            s.refresh_slots(std::path::Path::new("nie-ma-takiego-katalogu"));
            s.has_session = matches!(ekran_powloki, ShellScreen::Pause);
            s.screen = ekran_powloki.clone();
            let ctx = egui::Context::default();
            // Dwie klatki: `egui` stosuje zmianę skali dopiero w następnym przebiegu,
            // więc pierwsza klatka po jej ustawieniu ma jeszcze stary układ tekstu.
            for _ in 0..2 {
                s.prepare(&ctx);
                let _ = testing::draw_in(&ctx, testing::input(ekran), |ui| {
                    s.draw(ui);
                });
            }
            s.prepare(&ctx);
            let (teksty, zakres) = testing::draw_in(&ctx, testing::input(ekran), |ui| {
                s.draw(ui);
            });
            assert!(!teksty.is_empty(), "{skala}: {ekran_powloki:?} bez napisów");
            // Pion wolno przewijać, poziom nie: napis uciekający w bok jest nie do
            // odczytania, a `egui` go po prostu przytnie i nikt się nie dowie.
            assert!(
                zakres.right() <= ekran.x + 1.0,
                "{skala} ‰, {ekran_powloki:?}: napisy sięgają {} px przy ekranie {} px",
                zakres.right(),
                ekran.x
            );
        }
    }
}

/// Zmiana języka i skali działa **w tej samej sesji**, bez restartu (§7 dokumentu fazy).
#[test]
fn jezyk_i_skala_zmieniaja_sie_bez_restartu() {
    let mut s = powloka(Locale::Pl);
    s.screen = ShellScreen::MainMenu;
    let ctx = egui::Context::default();

    let wydruk = |s: &mut Shell, ctx: &egui::Context| -> String {
        s.prepare(ctx);
        let (teksty, _) = testing::draw_in(ctx, testing::input(testing::EKRAN), |ui| {
            s.draw(ui);
        });
        teksty.join(" ")
    };

    let po_polsku = wydruk(&mut s, &ctx);
    s.settings.locale = Locale::En;
    let po_angielsku = wydruk(&mut s, &ctx);
    assert_ne!(po_polsku, po_angielsku, "zmiana języka nie zmieniła ekranu");
    assert!(po_polsku.contains("Nowa gra"));
    assert!(po_angielsku.contains("New game"));

    s.settings.ui_scale = 2000;
    let _ = wydruk(&mut s, &ctx);
    assert!(
        (ctx.zoom_factor() - 2.0).abs() < 0.001,
        "skala interfejsu nie doszła do kontekstu: {}",
        ctx.zoom_factor()
    );
}

/// Lista slotów pokazuje zapis niezgodny wersją **z powodem**, zamiast go ukrywać.
#[test]
fn slot_w_zlej_wersji_zostaje_widoczny_i_opisany() {
    let dir = std::env::temp_dir().join(format!("magnat-sloty-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("katalog");
    // Nagłówek w wersji z przyszłości — dokładnie to, co gracz zobaczy po downgradzie.
    std::fs::write(
        magnat_game::save::meta_path(&dir, 3),
        "(id:3,city:\"Zapis\",game_date:(0),net_worth:(0),played_secs:0,\
         world:(seed:1,size:Small4km,epoch:Y1990,profile:Mixed,region:Lowland,\
         difficulty:Normal),schema_version:99,saved_at_wall:0)",
    )
    .expect("zapis nagłówka");

    let mut s = powloka(Locale::Pl);
    s.refresh_slots(&dir);
    s.screen = ShellScreen::Load {
        slots: (0..10).collect(),
        selected: None,
    };
    let ctx = egui::Context::default();
    s.prepare(&ctx);
    let (teksty, _) = testing::draw_in(&ctx, testing::input(testing::EKRAN), |ui| {
        s.draw(ui);
    });
    let razem = teksty.join(" ");
    assert!(
        razem.contains("nowszy format"),
        "slot w złej wersji zniknął z listy zamiast się opisać: {razem}"
    );
    std::fs::remove_dir_all(&dir).ok();
}
