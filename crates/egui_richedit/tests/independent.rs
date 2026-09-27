//! The crate depends on egui and on nothing from the workspace it is built
//! in, so that it stays a crate any egui application can use.
//
// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

#[test]
fn the_manifest_names_egui_and_nothing_else() {
    let manifest = include_str!("../Cargo.toml");
    let mut section = "";
    let mut dependencies = Vec::new();
    for line in manifest.lines().map(str::trim) {
        if line.starts_with('[') {
            section = line;
        } else if section == "[dependencies]"
            && let Some((name, _)) = line.split_once('=')
            && !line.starts_with('#')
        {
            dependencies.push(name.trim().trim_end_matches(".workspace").to_owned());
        }
    }
    assert_eq!(dependencies, ["egui"]);
}
