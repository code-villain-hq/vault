// `sqlx::migrate!` embeds the migrations when this crate compiles; rebuild
// whenever one is added or changed.
fn main() {
    println!("cargo:rerun-if-changed=../../migrations");
}
