//! Reading an opened mail from its source: who else it went to, its text,
//! its HTML with its pictures written in, and the files that came with it.

use std::collections::HashSet;

use base64::Engine;
use mail_parser::{MessageParser, MessagePart, PartKind};
use postrider_domain::{Attachment, MailSummary, Message};

/// How much of a mail's inline pictures goes into its HTML, all together.
const PICTURES: usize = 8 * 1024 * 1024;

/// The whole of a mail, from its source and its line in the Inbox.
pub fn read(raw: &[u8], summary: MailSummary) -> Message {
    let Some(message) = MessageParser::new().parse(raw) else {
        return Message {
            summary,
            to: Vec::new(),
            text: String::from_utf8_lossy(raw).into_owned(),
            html: None,
            attachments: Vec::new(),
            reply_to: None,
        };
    };

    let to = [message.to(), message.cc()]
        .into_iter()
        .flatten()
        .flat_map(|list| list.mailboxes().collect::<Vec<_>>())
        .filter_map(|mailbox| {
            let name = mailbox.name().map(str::trim).filter(|n| !n.is_empty());
            name.or(mailbox.address()).map(str::to_string)
        })
        .collect();

    let reply_to = message
        .reply_to()
        .and_then(|list| list.first())
        .and_then(|mailbox| {
            let name = mailbox.name().map(str::trim).filter(|n| !n.is_empty());
            name.or(mailbox.address()).map(str::to_string)
        });

    let text = message
        .text_bodies()
        .collect::<Vec<_>>()
        .join("\n\n")
        .trim_end()
        .to_string();

    // Only a mail written in HTML is shown as HTML: one in text keeps its
    // own words, in the app's type.
    let has_html = message
        .html_body()
        .any(|part| matches!(part.kind(), PartKind::Html));
    let html = has_html.then(|| message.html_bodies().collect::<Vec<_>>().join("\n"));
    let pictures = html
        .as_deref()
        .map(|html| pictures(&message, html))
        .unwrap_or_default();
    let html = html.map(|mut html| {
        for (id, part) in &pictures {
            let subtype = part
                .content_type()
                .and_then(|kind| kind.subtype())
                .unwrap_or("png");
            let url = format!(
                "data:image/{subtype};base64,{}",
                base64::engine::general_purpose::STANDARD.encode(part.decoded())
            );
            html = html.replace(&format!("cid:{id}"), &url);
        }
        html
    });

    let attachments = files(&message, &pictures)
        .map(|part| Attachment {
            name: file_name(&part),
            size: i64::try_from(part.decoded_len()).unwrap_or(i64::MAX),
        })
        .collect();

    Message {
        summary,
        to,
        text,
        html,
        attachments,
        reply_to,
    }
}

/// A mail you opened, read as [`read`] does, with how it's built in the
/// log ([`structure`]): what to look at when one shows nothing.
pub fn opened(raw: &[u8], summary: MailSummary) -> Message {
    let uid = summary.uid;
    let message = read(raw, summary);
    let html = message.html.as_deref().map_or(0, str::len);
    let text = message.text.trim().len();
    if html == 0 && text == 0 {
        tracing::warn!(uid, structure = %structure(raw), "an opened mail has nothing to show");
    } else {
        tracing::debug!(uid, html, text, structure = %structure(raw), "opened a mail");
    }
    message
}

/// How a mail is built, for the log, never its words: each part's type,
/// charset, transfer encoding, disposition, size decoded, how the body uses
/// it and what went wrong reading it, nested parts indented.
pub fn structure(raw: &[u8]) -> String {
    let Some(message) = MessageParser::new().parse(raw) else {
        return format!("not a mail, {} bytes", raw.len());
    };
    message
        .parts()
        .map(|part| {
            let depth = std::iter::successors(part.parent(), MessagePart::parent).count();
            let kind = part.content_type();
            let mut said = vec![kind.as_ref().map_or_else(
                || "no type".to_string(),
                |kind| match kind.subtype() {
                    Some(subtype) => format!("{}/{subtype}", kind.ctype()),
                    None => kind.ctype().to_string(),
                },
            )];
            said.extend(
                kind.and_then(|kind| kind.attribute("charset"))
                    .map(str::to_string),
            );
            said.extend(part.content_transfer_encoding().map(str::to_string));
            said.extend(part.content_disposition().map(|d| d.ctype().to_string()));
            if !part.is_multipart() {
                said.push(format!("{} bytes", part.decoded_len()));
            }
            said.push(format!("{:?}", part.role()));
            said.extend(part.flags().names().map(str::to_string));
            if let Some((text, problems)) = part.text_checked() {
                said.extend(problems.names().map(str::to_string));
                if text.trim().is_empty() {
                    said.push("blank".into());
                }
            }
            format!("{}{}", "  ".repeat(depth), said.join(" "))
        })
        .collect::<Vec<_>>()
        .join(" | ")
}

/// A file that came with a mail, to open.
pub struct File {
    pub name: String,
    /// Its MIME type: `application/pdf`.
    pub content_type: String,
    pub bytes: Vec<u8>,
}

/// The file at `index` among those [`read`] lists.
pub fn file(raw: &[u8], index: usize) -> Option<File> {
    let message = MessageParser::new().parse(raw)?;
    let html = message.html_bodies().collect::<Vec<_>>().join("\n");
    let pictures = pictures(&message, &html);
    let part = files(&message, &pictures).nth(index)?;
    let part = &part;
    let content_type = match part.kind() {
        PartKind::Message(_) => "message/rfc822".to_string(),
        _ => part.content_type().map_or_else(
            || "application/octet-stream".to_string(),
            |kind| match kind.subtype() {
                Some(subtype) => format!("{}/{subtype}", kind.ctype()),
                None => kind.ctype().to_string(),
            },
        ),
    };
    let bytes = match part.kind() {
        PartKind::Message(nested) => nested.raw().to_vec(),
        _ => part.decoded().into_owned(),
    };
    Some(File {
        name: file_name(part),
        content_type,
        bytes,
    })
}

/// The pictures a mail's HTML shows by `cid:`, by their id, while there's
/// room: they're written into it, and aren't among its files.
fn pictures<'m>(
    message: &'m mail_parser::Message<'_>,
    html: &str,
) -> Vec<(String, MessagePart<'m>)> {
    let mut room = PICTURES;
    let mut pictures = Vec::new();
    for part in message.html_body().chain(message.attachments()) {
        let Some(id) = part.content_id() else {
            continue;
        };
        let id = id.trim_matches(['<', '>']);
        let is_picture = part
            .content_type()
            .is_some_and(|kind| kind.ctype() == "image");
        if !is_picture || !html.contains(&format!("cid:{id}")) {
            continue;
        }
        let size = part.decoded_len();
        if size > room || pictures.iter().any(|(kept, _)| kept == id) {
            continue;
        }
        room -= size;
        pictures.push((id.to_string(), part));
    }
    pictures
}

/// The files that came with a mail, but the pictures written into it.
fn files<'m>(
    message: &'m mail_parser::Message<'_>,
    pictures: &[(String, MessagePart<'m>)],
) -> impl Iterator<Item = MessagePart<'m>> {
    let written_in: HashSet<String> = pictures.iter().map(|(id, _)| id.clone()).collect();
    message.attachments().filter(move |part| {
        part.content_id()
            .is_none_or(|id| !written_in.contains(id.trim_matches(['<', '>'])))
    })
}

/// A file's name: "Attachment" when the mail gives none; a mail in a mail
/// goes by its subject.
fn file_name(part: &MessagePart<'_>) -> String {
    let name = match part.kind() {
        PartKind::Message(nested) => nested.subject().map(str::to_string),
        _ => part.attachment_name().map(str::to_string),
    };
    name.map(|n| n.trim().to_string())
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| "Attachment".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use uuid::Uuid;

    fn summary() -> MailSummary {
        MailSummary {
            account_id: Uuid::nil(),
            folder: None,
            uid: 1,
            from_name: "Anna Weber".into(),
            from_address: "anna@example.test".into(),
            subject: "Layer for our team?".into(),
            snippet: String::new(),
            date: Utc::now(),
            seen: false,
            to_name: None,
        }
    }

    #[test]
    fn a_mail_in_text_keeps_its_words() {
        let raw = b"From: Anna Weber <anna@example.test>\r\n\
            To: Michael <michael@lazarski.test>, team@example.test\r\n\
            Cc: \"Kristina\" <kristina@lazarski.test>\r\n\
            Subject: Layer for our team?\r\n\
            Content-Type: text/plain; charset=utf-8\r\n\r\n\
            Hi Michael,\r\n\r\ndoes Layer work for a team?\r\n";
        let message = read(raw, summary());
        assert_eq!(message.to, ["Michael", "team@example.test", "Kristina"]);
        assert_eq!(
            message.text,
            "Hi Michael,\r\n\r\ndoes Layer work for a team?"
        );
        assert!(message.html.is_none());
        assert!(message.attachments.is_empty());
    }

    #[test]
    fn a_mail_in_html_brings_its_pictures_and_names_its_files() {
        let raw = b"From: Netcup <noreply@example.test>\r\n\
            Subject: Invoice\r\n\
            Content-Type: multipart/mixed; boundary=\"outer\"\r\n\r\n\
            --outer\r\n\
            Content-Type: multipart/related; boundary=\"inner\"\r\n\r\n\
            --inner\r\n\
            Content-Type: text/html; charset=utf-8\r\n\r\n\
            <p>Your invoice.</p><img src=\"cid:logo@example\">\r\n\
            --inner\r\n\
            Content-Type: image/png\r\n\
            Content-ID: <logo@example>\r\n\
            Content-Transfer-Encoding: base64\r\n\r\n\
            iVBORw0KGgo=\r\n\
            --inner--\r\n\
            --outer\r\n\
            Content-Type: application/pdf; name=\"Invoice.pdf\"\r\n\
            Content-Disposition: attachment; filename=\"Invoice.pdf\"\r\n\
            Content-Transfer-Encoding: base64\r\n\r\n\
            JVBERi0=\r\n\
            --outer--\r\n";
        let message = read(raw, summary());
        let html = message.html.expect("the mail is HTML");
        assert!(html.contains("<p>Your invoice.</p>"));
        assert!(html.contains("src=\"data:image/png;base64,iVBORw0KGgo=\""));
        assert_eq!(message.text, "Your invoice.");
        assert_eq!(message.attachments.len(), 1);
        assert_eq!(message.attachments[0].name, "Invoice.pdf");
        assert_eq!(message.attachments[0].size, 5);

        let file = file(raw, 0).expect("its first file");
        assert_eq!(file.name, "Invoice.pdf");
        assert_eq!(file.content_type, "application/pdf");
        assert_eq!(file.bytes, b"%PDF-");
        assert!(super::file(raw, 1).is_none());
    }

    #[test]
    fn the_log_says_how_a_mail_is_built_but_not_its_words() {
        let raw = b"From: Shop <noreply@example.test>\r\n\
            Subject: Meet the robot\r\n\
            Content-Type: multipart/alternative; boundary=\"b\"\r\n\r\n\
            --b\r\n\
            Content-Type: text/plain; charset=utf-8\r\n\
            Content-Transfer-Encoding: quoted-printable\r\n\r\n\
            Meet the robot\r\n\
            --b\r\n\
            Content-Type: text/html; charset=x-unknown\r\n\r\n\
            \r\n\
            --b--\r\n";
        let built = structure(raw);
        assert_eq!(
            built,
            [
                "multipart/alternative Container",
                "  text/plain utf-8 quoted-printable 14 bytes TextBody",
                "  text/html x-unknown 0 bytes HtmlBody unknown_charset blank",
            ]
            .join(" | ")
        );
        assert!(!built.contains("robot"));
    }
}
