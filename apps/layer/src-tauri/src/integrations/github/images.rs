//! Images in a pull request's description (PR-13).
//!
//! GitHub has no API for the attachments its own editor uploads: those go
//! through the signed-in web page. So the images go into the repository
//! itself, as blobs in a commit that only `refs/villain-layer/images` points
//! at. That ref is not a branch: no branch list shows it, a clone's default
//! refspec does not fetch it, and nothing can be checked out or merged from
//! it by accident. The description links each image by its commit, so a
//! later upload moving the ref on leaves the earlier links working.

use base64::Engine;
use serde_json::{json, Value};

use super::siblings;
use super::GitHub;
use crate::error::{Error, Result};

/// Under `refs/`, as the git-data endpoints spell it.
const REF: &str = "villain-layer/images";
const START: &str = "<!-- villain-layer:images -->";
const END: &str = "<!-- /villain-layer:images -->";

/// How many images one call may send, and how large each may be.
pub const MAX_IMAGES: usize = 10;
pub const MAX_BYTES: usize = 10 * 1024 * 1024;

/// One image to store: its file's bytes, checked by [`kind_of`].
pub struct Image {
    pub caption: String,
    pub ext: &'static str,
    pub bytes: Vec<u8>,
}

/// One stored image, as the description shows it.
#[derive(Debug, Clone)]
pub struct Shown {
    pub caption: String,
    pub url: String,
}

/// The image format of `bytes`, from its first bytes rather than a file
/// name, as the extension it is stored under. None for anything else.
///
/// The tool reads a path an agent hands it and publishes the file to
/// everyone who can read the repository: a key file renamed `shot.png` must
/// not get through.
pub fn kind_of(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some("png")
    } else if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        Some("jpg")
    } else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        Some("gif")
    } else if bytes.len() >= 12 && bytes.starts_with(b"RIFF") && &bytes[8..12] == b"WEBP" {
        Some("webp")
    } else {
        None
    }
}

impl GitHub {
    /// Store `images` for pull request `number` and show them in its
    /// description, replacing the ones shown before. Returns what is shown.
    pub async fn show_images(
        &self,
        owner: &str,
        repo: &str,
        number: u64,
        pr_url: &str,
        images: &[Image],
    ) -> Result<Vec<Shown>> {
        let files: Vec<String> = images
            .iter()
            .enumerate()
            .map(|(i, img)| format!("pr-{number}/{}-{}.{}", i + 1, slug(&img.caption), img.ext))
            .collect();

        let mut tree = Vec::new();
        for (img, path) in images.iter().zip(&files) {
            let blob = self
                .json(self.req(reqwest::Method::POST, &format!("/repos/{owner}/{repo}/git/blobs")).json(&json!({
                    "content": base64::engine::general_purpose::STANDARD.encode(&img.bytes),
                    "encoding": "base64",
                })))
                .await?;
            tree.push(json!({ "path": path, "mode": "100644", "type": "blob", "sha": sha_of(&blob)? }));
        }
        let tree = self
            .json(self.req(reqwest::Method::POST, &format!("/repos/{owner}/{repo}/git/trees")).json(&json!({ "tree": tree })))
            .await?;
        let commit = self.commit_onto_ref(owner, repo, number, sha_of(&tree)?).await?;

        let shown: Vec<Shown> = images
            .iter()
            .zip(&files)
            .map(|(img, path)| Shown { caption: img.caption.clone(), url: image_url(pr_url, &commit, path) })
            .collect();

        let path = format!("/repos/{owner}/{repo}/pulls/{number}");
        let v = self.json(self.req(reqwest::Method::GET, &path)).await?;
        let body = v.get("body").and_then(Value::as_str).unwrap_or_default();
        let next = with_images(body, &shown);
        if next != body {
            self.json(self.req(reqwest::Method::PATCH, &path).json(&json!({ "body": next }))).await?;
        }
        Ok(shown)
    }

    /// A commit of `tree` on top of the images ref, and the ref moved to it.
    async fn commit_onto_ref(&self, owner: &str, repo: &str, number: u64, tree: &str) -> Result<String> {
        // Two uploads at once (two repos of a task, two agents) race for the
        // ref. Moving it is a fast-forward only, so the loser builds on the
        // winner's commit and tries again rather than dropping its images.
        for _ in 0..3 {
            let parent = self.images_ref(owner, repo).await?;
            let commit = self
                .json(self.req(reqwest::Method::POST, &format!("/repos/{owner}/{repo}/git/commits")).json(&json!({
                    "message": format!("Images for #{number}"),
                    "tree": tree,
                    "parents": parent.iter().collect::<Vec<_>>(),
                })))
                .await?;
            let commit = sha_of(&commit)?.to_string();
            let moved = match parent {
                None => {
                    self.json(self.req(reqwest::Method::POST, &format!("/repos/{owner}/{repo}/git/refs")).json(&json!({
                        "ref": format!("refs/{REF}"),
                        "sha": commit,
                    })))
                    .await
                }
                Some(_) => {
                    self.json(self.req(reqwest::Method::PATCH, &format!("/repos/{owner}/{repo}/git/refs/{REF}")).json(&json!({
                        "sha": commit,
                        "force": false,
                    })))
                    .await
                }
            };
            match moved {
                Ok(_) => return Ok(commit),
                // Moved under us, or created by the other upload first.
                Err(Error::Other(e)) if e.starts_with("GitHub 422") => continue,
                Err(e) => return Err(e),
            }
        }
        Err(Error::Other(format!("{REF} kept moving while the images were stored; try again")))
    }

    /// Where the images ref points, or None before the first upload.
    async fn images_ref(&self, owner: &str, repo: &str) -> Result<Option<String>> {
        // The prefix listing answers an empty list for no ref, where asking for
        // the ref itself answers 404, which reads like a missing repository.
        let v = self
            .json(self.req(reqwest::Method::GET, &format!("/repos/{owner}/{repo}/git/matching-refs/{REF}")))
            .await?;
        let want = format!("refs/{REF}");
        Ok(v.as_array()
            .into_iter()
            .flatten()
            .find(|r| r.get("ref").and_then(Value::as_str) == Some(&want))
            .and_then(|r| r.pointer("/object/sha").and_then(Value::as_str))
            .map(str::to_string))
    }
}

fn sha_of(v: &Value) -> Result<&str> {
    v.get("sha")
        .and_then(Value::as_str)
        .ok_or_else(|| Error::Other("GitHub answered without a sha".into()))
}

/// A caption as a file name: lowercase letters, digits and dashes.
fn slug(caption: &str) -> String {
    let s: String = caption
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_lowercase() } else { '-' })
        .collect();
    let s = s.split('-').filter(|p| !p.is_empty()).take(6).collect::<Vec<_>>().join("-");
    if s.is_empty() {
        "image".into()
    } else {
        s.chars().take(40).collect()
    }
}

/// The image's address, beside the pull request's own: on GitHub Enterprise
/// the web host is not the API's.
pub fn image_url(pr_url: &str, commit: &str, path: &str) -> String {
    let repo = pr_url.rsplitn(3, '/').nth(2).unwrap_or(pr_url);
    format!("{repo}/raw/{commit}/{path}")
}

/// `body` with its image section replaced by one showing `shown`: where it
/// was, or above PR-2's list of the other PRs, or at the end.
pub fn with_images(body: &str, shown: &[Shown]) -> String {
    let items: Vec<String> = shown
        .iter()
        .map(|s| {
            let caption = plain(&s.caption);
            format!("**{caption}**\n\n![{}]({})", caption.replace(['[', ']'], ""), s.url)
        })
        .collect();
    let section = format!("{START}\n### Screenshots\n\n{}\n{END}", items.join("\n\n"));

    if let Some(at) = body.find(START) {
        let after = body[at..].find(END).map(|e| at + e + END.len()).unwrap_or(body.len());
        return format!("{}{section}{}", &body[..at], &body[after..]);
    }
    if let Some(at) = body.find(siblings::START) {
        let before = body[..at].trim_end();
        let sep = if before.is_empty() { "" } else { "\n\n" };
        return format!("{before}{sep}{section}\n\n{}", &body[at..]);
    }
    let own = body.trim_end();
    if own.is_empty() {
        section
    } else {
        format!("{own}\n\n{section}")
    }
}

/// A caption on one line, with nothing in it that could end the section.
fn plain(caption: &str) -> String {
    let bare = caption.replace("<!--", "").replace("-->", "").replace('*', "");
    let one = bare.split_whitespace().collect::<Vec<_>>().join(" ");
    if one.is_empty() {
        "Screenshot".into()
    } else {
        one
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::GithubConfig;
    use std::sync::Arc;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    fn shot(caption: &str) -> Shown {
        Shown { caption: caption.into(), url: format!("https://github.com/acme/web/raw/abc/pr-12/{caption}.png") }
    }

    #[test]
    fn images_go_below_what_the_author_wrote() {
        let body = with_images("Darker sidebar.\n", &[shot("Night"), shot("Day")]);
        assert_eq!(
            body,
            "Darker sidebar.\n\n<!-- villain-layer:images -->\n### Screenshots\n\n\
             **Night**\n\n![Night](https://github.com/acme/web/raw/abc/pr-12/Night.png)\n\n\
             **Day**\n\n![Day](https://github.com/acme/web/raw/abc/pr-12/Day.png)\n\
             <!-- /villain-layer:images -->"
        );
    }

    #[test]
    fn a_later_call_replaces_the_images_and_keeps_everything_around_them() {
        let first = with_images("Darker sidebar.", &[shot("Night")]);
        let edited = format!("{}\n\nThanks!", first.replace("Darker sidebar.", "Darker sidebar, really."));

        let second = with_images(&edited, &[shot("Day")]);

        assert!(second.starts_with("Darker sidebar, really.\n\n<!-- villain-layer:images -->"));
        assert!(second.ends_with("<!-- /villain-layer:images -->\n\nThanks!"));
        assert!(!second.contains("Night"));
        assert_eq!(with_images(&second, &[shot("Day")]), second);
    }

    #[test]
    fn images_go_above_the_list_of_other_prs() {
        let web = siblings::Sibling {
            repo: "web".into(),
            owner: "acme".into(),
            name: "web".into(),
            number: 3,
            url: "https://github.com/acme/web/pull/3".into(),
        };
        let listed = siblings::with_siblings("Spec.", &[&web], None);

        let body = with_images(&listed, &[shot("Night")]);

        let (images, others) = (body.find(START).unwrap(), body.find(siblings::START).unwrap());
        assert!(body.starts_with("Spec.\n\n") && images < others, "{body}");
        // And PR-2 rewriting its list later leaves the images where they are.
        let again = siblings::with_siblings(&body, &[&web], None);
        assert!(again.find(START).unwrap() < again.find(siblings::START).unwrap(), "{again}");
    }

    #[test]
    fn a_caption_cannot_close_the_section_or_break_the_markdown() {
        let caption = "Night <!-- /villain-layer:images --> **bold** [x]\nnext";
        let body = with_images("", &[Shown { caption: caption.into(), url: "https://github.com/acme/web/raw/abc/1.png".into() }]);
        assert_eq!(body.matches("villain-layer:images -->").count(), 2);
        assert!(body.contains("**Night /villain-layer:images bold [x] next**"), "{body}");
        assert!(body.contains("![Night /villain-layer:images bold x next]("), "{body}");
    }

    #[test]
    fn only_image_files_are_images_whatever_they_are_called() {
        assert_eq!(kind_of(b"\x89PNG\r\n\x1a\n...."), Some("png"));
        assert_eq!(kind_of(&[0xFF, 0xD8, 0xFF, 0xE0]), Some("jpg"));
        assert_eq!(kind_of(b"GIF89a..."), Some("gif"));
        assert_eq!(kind_of(b"RIFF\0\0\0\0WEBPVP8 "), Some("webp"));
        assert_eq!(kind_of(b"-----BEGIN OPENSSH PRIVATE KEY-----"), None);
        assert_eq!(kind_of(b""), None);
    }

    #[test]
    fn an_image_is_linked_beside_its_pull_request_on_any_host() {
        assert_eq!(
            image_url("https://github.com/acme/web/pull/12", "abc", "pr-12/1-night.png"),
            "https://github.com/acme/web/raw/abc/pr-12/1-night.png"
        );
        assert_eq!(
            image_url("https://ghe.acme.test/acme/web/pull/12", "abc", "pr-12/1-day.png"),
            "https://ghe.acme.test/acme/web/raw/abc/pr-12/1-day.png"
        );
        assert_eq!(slug("Night: the sidebar!"), "night-the-sidebar");
        assert_eq!(slug("…"), "image");
    }

    /// A GitHub that answers the git-data and pull endpoints, keeping every
    /// request it was sent. `ref_at` is where the images ref already is;
    /// `races` is how many ref moves to refuse as not a fast-forward.
    async fn github(ref_at: Option<&'static str>, races: usize) -> (GitHub, Arc<parking_lot::Mutex<Vec<String>>>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let seen = Arc::new(parking_lot::Mutex::new(Vec::<String>::new()));
        let log = seen.clone();
        let races = Arc::new(parking_lot::Mutex::new(races));
        tokio::spawn(async move {
            while let Ok((mut sock, _)) = listener.accept().await {
                let mut raw = Vec::new();
                let mut buf = [0u8; 65536];
                // Read the head, then as much body as it says.
                loop {
                    let n = sock.read(&mut buf).await.unwrap_or(0);
                    if n == 0 {
                        break;
                    }
                    raw.extend_from_slice(&buf[..n]);
                    let text = String::from_utf8_lossy(&raw).to_string();
                    if let Some(head_end) = text.find("\r\n\r\n") {
                        let len = text[..head_end]
                            .lines()
                            .find_map(|l| l.to_ascii_lowercase().strip_prefix("content-length:").map(|v| v.trim().parse::<usize>().unwrap()))
                            .unwrap_or(0);
                        if raw.len() >= head_end + 4 + len {
                            break;
                        }
                    }
                }
                let text = String::from_utf8_lossy(&raw).to_string();
                let line = text.lines().next().unwrap_or_default().to_string();
                let body = text.split_once("\r\n\r\n").map(|(_, b)| b.to_string()).unwrap_or_default();
                log.lock().push(format!("{line} {body}"));
                let (status, reply) = if line.starts_with("GET /repos/acme/web/git/matching-refs/") {
                    let refs = match ref_at {
                        Some(sha) => json!([{ "ref": "refs/villain-layer/images", "object": { "sha": sha } }]),
                        None => json!([]),
                    };
                    ("200 OK", refs)
                } else if line.starts_with("POST /repos/acme/web/git/blobs") {
                    ("201 Created", json!({ "sha": "blob1" }))
                } else if line.starts_with("POST /repos/acme/web/git/trees") {
                    ("201 Created", json!({ "sha": "tree1" }))
                } else if line.starts_with("POST /repos/acme/web/git/commits") {
                    ("201 Created", json!({ "sha": "commit1" }))
                } else if line.starts_with("PATCH /repos/acme/web/git/refs/") || line.starts_with("POST /repos/acme/web/git/refs") {
                    let mut left = races.lock();
                    if *left > 0 {
                        *left -= 1;
                        ("422 Unprocessable Entity", json!({ "message": "Update is not a fast forward" }))
                    } else {
                        ("200 OK", json!({ "ref": "refs/villain-layer/images" }))
                    }
                } else if line.starts_with("GET /repos/acme/web/pulls/12") {
                    ("200 OK", json!({ "number": 12, "body": "Darker sidebar." }))
                } else if line.starts_with("PATCH /repos/acme/web/pulls/12") {
                    ("200 OK", json!({ "number": 12 }))
                } else {
                    ("404 Not Found", json!({ "message": "Not Found" }))
                };
                let reply = reply.to_string();
                let out = format!(
                    "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{reply}",
                    reply.len()
                );
                let _ = sock.write_all(out.as_bytes()).await;
            }
        });
        let cfg = GithubConfig { api_url: format!("http://{addr}"), ..Default::default() };
        (GitHub::new(&cfg, "token").unwrap(), seen)
    }

    fn png(caption: &str) -> Image {
        Image { caption: caption.into(), ext: "png", bytes: b"\x89PNG\r\n\x1a\nnight".to_vec() }
    }

    #[tokio::test]
    async fn the_first_upload_starts_the_ref_and_the_description_links_its_commit() {
        let (gh, seen) = github(None, 0).await;

        let shown = gh.show_images("acme", "web", 12, "https://github.com/acme/web/pull/12", &[png("Night")]).await.unwrap();

        assert_eq!(shown[0].url, "https://github.com/acme/web/raw/commit1/pr-12/1-night.png");
        let seen = seen.lock();
        let commit = seen.iter().find(|r| r.starts_with("POST /repos/acme/web/git/commits")).unwrap();
        assert!(commit.contains(r#""parents":[]"#), "{commit}");
        let create = seen.iter().find(|r| r.starts_with("POST /repos/acme/web/git/refs ")).unwrap();
        assert!(create.contains(r#""ref":"refs/villain-layer/images""#), "{create}");
        let tree = seen.iter().find(|r| r.starts_with("POST /repos/acme/web/git/trees")).unwrap();
        assert!(tree.contains(r#""path":"pr-12/1-night.png""#) && !tree.contains("base_tree"), "{tree}");
        let edit = seen.iter().find(|r| r.starts_with("PATCH /repos/acme/web/pulls/12")).unwrap();
        assert!(edit.contains("Darker sidebar.") && edit.contains("raw/commit1/pr-12/1-night.png"), "{edit}");
    }

    #[tokio::test]
    async fn a_later_upload_builds_on_the_ref_and_never_forces_it() {
        let (gh, seen) = github(Some("older"), 0).await;

        gh.show_images("acme", "web", 12, "https://github.com/acme/web/pull/12", &[png("Day")]).await.unwrap();

        let seen = seen.lock();
        let commit = seen.iter().find(|r| r.starts_with("POST /repos/acme/web/git/commits")).unwrap();
        assert!(commit.contains(r#""parents":["older"]"#), "{commit}");
        let moved = seen.iter().find(|r| r.starts_with("PATCH /repos/acme/web/git/refs/villain-layer/images")).unwrap();
        assert!(moved.contains(r#""force":false"#), "{moved}");
    }

    #[tokio::test]
    async fn an_upload_that_loses_the_race_for_the_ref_builds_on_the_winner() {
        let (gh, seen) = github(Some("older"), 1).await;

        gh.show_images("acme", "web", 12, "https://github.com/acme/web/pull/12", &[png("Day")]).await.unwrap();

        let seen = seen.lock();
        assert_eq!(seen.iter().filter(|r| r.starts_with("PATCH /repos/acme/web/git/refs/")).count(), 2);
        assert_eq!(seen.iter().filter(|r| r.starts_with("GET /repos/acme/web/git/matching-refs/")).count(), 2);
    }
}
