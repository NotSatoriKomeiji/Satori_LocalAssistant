//! Content is never collected. These rules only gate basic application metadata.
pub fn sensitive_app(name: &str) -> bool {
    let name = name.to_lowercase();
    [
        "credentialuibroker.exe",
        "logonui.exe",
        "consent.exe",
        "keepass.exe",
        "keepassxc.exe",
        "1password.exe",
        "bitwarden.exe",
        "dashlane.exe",
    ]
    .iter()
    .any(|entry| name == *entry)
}
