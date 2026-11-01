// `sqlx::migrate!` embeds the migrations when this crate builds; a new
// file there has to rebuild it, or the app starts without it.
fn main() {
    println!("cargo:rerun-if-changed=../../migrations");
}
