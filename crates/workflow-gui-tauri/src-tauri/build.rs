fn main() {
    let capabilities = std::path::Path::new("capabilities");
    if capabilities.exists() {
        let mut entries = std::fs::read_dir(capabilities).expect("lecture capabilities/");
        if entries.next().is_some() {
            panic!("A-Strict: capabilities/ doit rester vide (aucune capability)");
        }
    }
    println!("cargo:rerun-if-changed=capabilities");
    tauri_build::build();
}
