fn main() {
    env_logger::init();
    // The egui shell arrives in phase 4; until then the engine is exercised
    // through the CLI examples in tomtemacro-core.
    println!(
        "tomte {} — GUI coming in phase 4",
        env!("CARGO_PKG_VERSION")
    );
}
