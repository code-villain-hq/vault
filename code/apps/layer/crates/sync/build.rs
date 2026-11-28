fn main() {
    // SQLx embeds these files but Cargo otherwise misses newly added ones.
    println!("cargo:rerun-if-changed=../../migrations");
}
