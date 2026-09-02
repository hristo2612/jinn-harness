// The archive is compiled in from `$JINN_UI_BUNDLE_DIR`; a kit that
// points the variable elsewhere (a marked or a corrupted variant) must
// rebuild the crate, not reuse the last build's bytes.
fn main() {
    println!("cargo:rerun-if-env-changed=JINN_UI_BUNDLE_DIR");
}
