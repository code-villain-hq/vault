//! Reading an opened mail from its source: who else it went to, its text,
//! its HTML with its pictures written in, and the files that came with it.

use std::collections::HashSet;

use base64::Engine;
use mail_parser::{MessageParser, PartKind};
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
    let mut written_in = HashSet::new();
    let html = has_html.then(|| {
        let mut html = message.html_bodies().collect::<Vec<_>>().join("\n");
        let mut room = PICTURES;
        for part in message.html_body().chain(message.attachments()) {
            let Some(id) = part.content_id() else {
                continue;
            };
            let id = id.trim_matches(['<', '>']);
            let reference = format!("cid:{id}");
            let Some(kind) = part.content_type() else {
                continue;
            };
            if kind.ctype() != "image" || !html.contains(&reference) {
                continue;
            }
            let picture = part.decoded();
            if picture.len() > room {
                continue;
            }
            room -= picture.len();
            let url = format!(
                "data:image/{};base64,{}",
                kind.subtype().unwrap_or("png"),
                base64::engine::general_purpose::STANDARD.encode(&picture)
            );
            html = html.replace(&reference, &url);
            written_in.insert(id.to_string());
        }
        html
    });

    let attachments = message
        .attachments()
        .filter(|part| {
            part.content_id()
                .is_none_or(|id| !written_in.contains(id.trim_matches(['<', '>'])))
        })
        .map(|part| {
            let name = match part.kind() {
                PartKind::Message(nested) => nested.subject().map(str::to_string),
                _ => part.attachment_name().map(str::to_string),
            };
            Attachment {
                name: name
                    .map(|n| n.trim().to_string())
                    .filter(|n| !n.is_empty())
                    .unwrap_or_else(|| "Attachment".into()),
                size: i64::try_from(part.decoded_len()).unwrap_or(i64::MAX),
            }
        })
        .collect();

    Message {
        summary,
        to,
        text,
        html,
        attachments,
    }
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
    }
}
