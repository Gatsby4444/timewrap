//! Binaire hôte utilisé par Gradle pour générer les bindings Kotlin
//! à partir de la bibliothèque native compilée.
fn main() {
    uniffi::uniffi_bindgen_main()
}
