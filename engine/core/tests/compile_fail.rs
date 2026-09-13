//! Testy „to NIE MA się skompilować" (M0 WP-02b, §7.2).
//!
//! Kontrakt egzekwowany przez kompilator jest wart więcej niż kontrakt egzekwowany
//! przez przegląd kodu — te testy pilnują, że nadal jest egzekwowany.

#[test]
fn typy_pilnuja_kontraktow() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/ui/*.rs");
}
