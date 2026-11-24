//! Who writes a comment, and when: what a comment written here is signed and
//! dated with, in every application.
//
// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

/// The name a new comment is signed with: the user's full name where the
/// system keeps one, and otherwise their login.
pub fn author() -> Option<String> {
    let login = std::env::var("USER")
        .or_else(|_| std::env::var("USERNAME"))
        .ok()
        .filter(|login| !login.is_empty())?;
    let full = std::fs::read_to_string("/etc/passwd")
        .ok()
        .and_then(|passwd| {
            passwd.lines().find_map(|line| {
                let fields: Vec<&str> = line.split(':').collect();
                (fields.first() == Some(&login.as_str()))
                    .then(|| {
                        fields
                            .get(4)?
                            .split(',')
                            .next()
                            .map(str::trim)
                            .map(ToOwned::to_owned)
                    })
                    .flatten()
                    .filter(|name| !name.is_empty())
            })
        });
    Some(full.unwrap_or(login))
}

/// Now, in local time, as ISO 8601 without a zone, which is how a comment is
/// dated.
pub fn now() -> String {
    jiff::ZonedDateTime::now()
        .strftime("%Y-%m-%dT%H:%M:%S")
        .to_string()
}
