//! Wbudowany katalog klipów animacji (M11b §5.4, WP3).
//!
//! Osobny plik, bo to są **dane w postaci kodu**: dwadzieścia kilka list kanałów,
//! które zmieniają się z innego powodu niż wypalanie póz obok. Kiedy animator dostanie
//! narzędzia (modding, M12), ten plik zamienia się w ładowarkę i nic poza nim nie drgnie.

use super::{AnimationClip, Channel, ClipKind, Motion, WorkStyle};
use crate::model::PartName;

/// Oś Y jest osią poprzeczną postaci i pojazdu: obrót wokół niej to wymach nogi w przód
/// i obrót koła. Model patrzy w `+X` przy `yaw == 0` — tak samo pieszy i auto.
const OS_Y: u8 = 1;
const OS_Z: u8 = 2;

fn swing(part: PartName, amp_deg: i16, phase: u8) -> Channel {
    Channel::new(
        part,
        Motion::Swing {
            axis: OS_Y,
            amp_deg,
            phase,
        },
    )
}

pub(super) fn builtin_clips() -> Vec<AnimationClip> {
    use ClipKind as K;
    let mut v = Vec::new();

    // Kolejność ma jedno znaczenie: `ClipId(0)` jest odpowiedzią na brak klipu,
    // więc zerowym klipem musi być ten, który wygląda poprawnie zawsze.
    v.push(AnimationClip::new(
        K::Idle,
        8,
        vec![swing(PartName::TORSO, 2, 0)],
    ));
    v.push(AnimationClip::new(K::Walk, 16, chod(1.0)));
    v.push(AnimationClip::new(
        K::WalkCarry,
        16,
        {
            let mut c = chod(0.9);
            // Ręce zajęte: przedramiona przed sobą, ramiona prawie bez ruchu.
            c.retain(|k| {
                !matches!(
                    k.part,
                    PartName::ARM_L | PartName::ARM_R | PartName::FOREARM_L | PartName::FOREARM_R
                )
            });
            c.push(Channel::new(PartName::ARM_L, Motion::Hold).tilt([0, -35, 0]));
            c.push(Channel::new(PartName::ARM_R, Motion::Hold).tilt([0, -35, 0]));
            c.push(Channel::new(PartName::FOREARM_L, Motion::Hold).tilt([0, -55, 0]));
            c.push(Channel::new(PartName::FOREARM_R, Motion::Hold).tilt([0, -55, 0]));
            c
        },
    ));
    v.push(AnimationClip::new(K::Run, 16, {
        let mut c = chod(1.6);
        c.push(Channel::new(PartName::TORSO, Motion::Hold).tilt([0, -12, 0]));
        c
    }));
    v.push(AnimationClip::new(K::Sit, 8, siad()));
    v.push(AnimationClip::new(
        K::Stand,
        8,
        vec![swing(PartName::TORSO, 1, 0)],
    ));
    // Wsiadanie i wysiadanie: pochylenie i zgięcie nóg. Klip jest zapętlony, bo rekord
    // nie niesie chwili startu zdarzenia — ponytail: sufit nazwany w dokumentacji modułu.
    v.push(AnimationClip::new(
        K::EnterVehicle,
        8,
        wsiadanie(),
    ));
    v.push(AnimationClip::new(K::ExitVehicle, 8, wsiadanie()));
    v.push(AnimationClip::new(K::Board, 8, wsiadanie()));
    for styl in WorkStyle::ALL {
        v.push(AnimationClip::new(K::Work(styl), 16, praca(styl)));
    }
    v.push(AnimationClip::new(K::Shop, 16, {
        let mut c = chod(0.6);
        c.retain(|k| k.part != PartName::ARM_R && k.part != PartName::FOREARM_R);
        c.push(swing(PartName::ARM_R, 30, 0));
        c.push(Channel::new(PartName::FOREARM_R, Motion::Hold).tilt([0, -40, 0]));
        c
    }));
    v.push(AnimationClip::new(
        K::Queue,
        16,
        vec![
            swing(PartName::TORSO, 3, 0),
            swing(PartName::ARM_L, 4, 128),
            swing(PartName::ARM_R, 4, 0),
        ],
    ));
    v.push(AnimationClip::new(
        K::Drive,
        16,
        vec![
            Channel::new(PartName::WHEEL_FL, Motion::Spin { axis: OS_Y }),
            Channel::new(PartName::WHEEL_FR, Motion::Spin { axis: OS_Y }),
            Channel::new(PartName::WHEEL_RL, Motion::Spin { axis: OS_Y }),
            Channel::new(PartName::WHEEL_RR, Motion::Spin { axis: OS_Y }),
        ],
    ));
    // Pojazd stojący, tankujący i rozładowywany nie rusza żadną częścią, którą mamy
    // czym narysować. Klip istnieje, bo wypełniacz musi mieć co wpisać do rekordu;
    // pusta lista kanałów znaczy „poza spoczynkowa" i nie zajmuje miejsca w atlasie.
    v.push(AnimationClip::new(K::Park, 8, Vec::new()));
    v.push(AnimationClip::new(K::Refuel, 8, Vec::new()));
    v.push(AnimationClip::new(
        K::Unload,
        16,
        vec![Channel::new(PartName::CARGO, Motion::Swing {
            axis: OS_Z,
            amp_deg: 6,
            phase: 0,
        })],
    ));
    // Maszyn (wózek, dźwig, rampa) nie ma jeszcze w katalogu modeli — klipy czekają
    // gotowe i wypieką się same, gdy model z częścią `mast` albo `cargo` się pojawi.
    v.push(AnimationClip::new(
        K::Forklift,
        16,
        vec![Channel::new(PartName::MAST, Motion::Hold).offset([0, 0, 4])],
    ));
    v.push(AnimationClip::new(
        K::Crane,
        32,
        vec![Channel::new(PartName::MAST, Motion::Swing {
            axis: OS_Z,
            amp_deg: 40,
            phase: 0,
        })],
    ));
    v.push(AnimationClip::new(
        K::RampLoad,
        16,
        vec![Channel::new(PartName::CARGO, Motion::Swing {
            axis: OS_Y,
            amp_deg: 10,
            phase: 0,
        })],
    ));
    v
}

/// Chód: nogi i ręce w przeciwfazie, przedramiona i golenie z opóźnieniem.
///
/// `skala` mnoży amplitudy — ten sam kształt służy za marsz, bieg i wolny krok
/// po sklepie. Kanały `arms`/`legs` są dla poziomu L1, w którym kończyny są jedną
/// bryłą; w L0 tych części nie ma i shader ich nie zobaczy.
fn chod(skala: f32) -> Vec<Channel> {
    let a = |stopnie: f32| (stopnie * skala) as i16;
    vec![
        swing(PartName::THIGH_L, a(28.0), 0),
        swing(PartName::THIGH_R, a(28.0), 128),
        swing(PartName::SHIN_L, a(16.0), 64),
        swing(PartName::SHIN_R, a(16.0), 192),
        swing(PartName::ARM_L, a(22.0), 128),
        swing(PartName::ARM_R, a(22.0), 0),
        swing(PartName::FOREARM_L, a(10.0), 160),
        swing(PartName::FOREARM_R, a(10.0), 32),
        swing(PartName::LEGS, a(14.0), 0),
        swing(PartName::ARMS, a(12.0), 128),
        swing(PartName::TORSO, a(2.0), 0),
    ]
}

/// Siad: uda w poziomie, golenie w dół, tors niżej o pół voxela.
///
/// Znak obrotu wokół `Y` jest tu treścią, nie szczegółem: dodatni kąt kładzie udo
/// **do tyłu**, więc siedząca postać potrzebuje ujemnego.
fn siad() -> Vec<Channel> {
    vec![
        Channel::new(PartName::THIGH_L, Motion::Hold).tilt([0, -85, 0]),
        Channel::new(PartName::THIGH_R, Motion::Hold).tilt([0, -85, 0]),
        Channel::new(PartName::SHIN_L, Motion::Hold).tilt([0, 85, 0]),
        Channel::new(PartName::SHIN_R, Motion::Hold).tilt([0, 85, 0]),
        Channel::new(PartName::LEGS, Motion::Hold).tilt([0, -85, 0]),
        Channel::new(PartName::TORSO, Motion::Swing {
            axis: OS_Y,
            amp_deg: 2,
            phase: 0,
        })
        .offset([0, 0, -2]),
        Channel::new(PartName::ARM_L, Motion::Hold).tilt([0, -20, 0]),
        Channel::new(PartName::ARM_R, Motion::Hold).tilt([0, -20, 0]),
    ]
}

fn wsiadanie() -> Vec<Channel> {
    vec![
        Channel::new(PartName::TORSO, Motion::Swing {
            axis: OS_Y,
            amp_deg: 18,
            phase: 0,
        }),
        swing(PartName::THIGH_L, 40, 0),
        swing(PartName::THIGH_R, 20, 0),
        swing(PartName::LEGS, 25, 0),
        swing(PartName::ARM_L, 15, 128),
    ]
}

fn praca(styl: WorkStyle) -> Vec<Channel> {
    match styl {
        WorkStyle::Counter => vec![
            Channel::new(PartName::ARM_L, Motion::Hold).tilt([0, -45, 0]),
            Channel::new(PartName::ARM_R, Motion::Hold).tilt([0, -45, 0]),
            swing(PartName::FOREARM_L, 20, 0),
            swing(PartName::FOREARM_R, 20, 128),
            swing(PartName::TORSO, 2, 0),
            swing(PartName::ARMS, 10, 0),
        ],
        WorkStyle::Warehouse => vec![
            swing(PartName::ARM_L, 45, 0),
            swing(PartName::ARM_R, 45, 0),
            swing(PartName::FOREARM_L, 25, 32),
            swing(PartName::FOREARM_R, 25, 32),
            swing(PartName::TORSO, 12, 0),
            swing(PartName::ARMS, 30, 0),
        ],
        WorkStyle::Line => vec![
            Channel::new(PartName::ARM_L, Motion::Hold).tilt([0, -55, 0]),
            Channel::new(PartName::ARM_R, Motion::Hold).tilt([0, -55, 0]),
            swing(PartName::FOREARM_L, 30, 0),
            swing(PartName::FOREARM_R, 30, 128),
            swing(PartName::ARMS, 14, 0),
        ],
        WorkStyle::Office => vec![
            Channel::new(PartName::ARM_R, Motion::Hold).tilt([0, -50, 0]),
            Channel::new(PartName::ARM_L, Motion::Hold).tilt([0, -50, 0]),
            swing(PartName::FOREARM_R, 8, 0),
            swing(PartName::TORSO, 2, 64),
            swing(PartName::ARMS, 6, 0),
        ],
        WorkStyle::Construction => vec![
            swing(PartName::ARM_R, 55, 0),
            swing(PartName::FOREARM_R, 30, 32),
            Channel::new(PartName::ARM_L, Motion::Hold).tilt([0, -25, 0]),
            swing(PartName::TORSO, 8, 0),
            swing(PartName::ARMS, 26, 0),
        ],
        // Za kierownicą postać siedzi — te same uda co w `Sit`, ręce przed sobą.
        WorkStyle::Driving => {
            let mut c = siad();
            c.retain(|k| k.part != PartName::ARM_L && k.part != PartName::ARM_R);
            c.push(Channel::new(PartName::ARM_L, Motion::Hold).tilt([0, -60, 0]));
            c.push(Channel::new(PartName::ARM_R, Motion::Hold).tilt([0, -60, 0]));
            c
        }
    }
}

