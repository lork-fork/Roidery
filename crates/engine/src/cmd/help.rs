//! Help → project links. Roidery: VectorCraft's community links (its Discord, website and app
//! page) are gone, since its brand licence keeps the ArtCraft name out of forks; what is left is
//! the VectorCraft repository Roidery is based on. The commands return the URL; the frontend
//! opens it.

use serde_json::{Value, json};

use super::*;

/// The upstream app's id on GitHub.
pub const APP_ID: &str = "vectorcraft";

/// The source repository of VectorCraft, which Roidery is based on.
pub fn github_url() -> String {
    format!("https://github.com/storytold/{APP_ID}")
}

pub fn specs() -> Vec<CommandSpec> {
    vec![
        cmd!(query "help.github", "VectorCraft on GitHub", ["Help"], None, "{} → {url} the source of VectorCraft, which Roidery is based on", always, |_, _| url(github_url())),
        cmd!(query "help.links", "Links", [], None, "{} → {github}", always, |_, _| Ok(json!({ "github": github_url() }))),
    ]
}

fn url(u: String) -> Result<Value> {
    Ok(json!({ "url": u }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn links() {
        let mut s = Session::new();
        assert!(s.execute("help.discord", &json!({})).is_err());
        assert_eq!(s.execute("help.github", &json!({})).unwrap()["url"], "https://github.com/storytold/vectorcraft");
        assert_eq!(s.execute("help.links", &json!({})).unwrap()["github"], "https://github.com/storytold/vectorcraft");
    }
}
