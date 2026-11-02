//! An account's sorting scripts, over ManageSieve (RFC 5804) on port 4190,
//! as Dovecot's Pigeonhole serves them on Uberspace: list them, read one,
//! put one, and pick the one the server runs. Over STARTTLS; only a
//! stand-in on this Mac is read without.

use base64::Engine;
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;

use crate::imap::{CONNECT_TIMEOUT, Failure, Stream, is_loopback, tls};

/// Where ManageSieve listens.
pub const PORT: u16 = 4190;

/// The most of a script Postrider reads, past which an answer is taken as
/// broken.
const LONGEST: usize = 1 << 20;

/// A word of an answer: an atom (`OK`, `ACTIVE`), a string (quoted or a
/// literal), or a response code in brackets.
#[derive(Debug, Clone, PartialEq)]
enum Word {
    Atom(String),
    Text(String),
    Code(String),
}

/// How a command ended.
struct Ended {
    ok: bool,
    code: String,
    text: String,
}

/// A signed-in connection to an account's ManageSieve.
pub struct Sieve {
    conn: BufReader<Stream>,
    host: String,
}

impl Sieve {
    /// Connects and signs in as `username`.
    pub async fn sign_in(host: &str, username: &str, password: &str) -> Result<Sieve, Failure> {
        let offline = |e: &dyn std::fmt::Display| Failure::Offline(format!("{host}: {e}"));
        let tcp = tokio::time::timeout(CONNECT_TIMEOUT, TcpStream::connect((host, PORT)))
            .await
            .map_err(|_| Failure::Offline(format!("{host} didn't answer")))?
            .map_err(|e| match e.kind() {
                std::io::ErrorKind::ConnectionRefused => Failure::Refused(format!(
                    "{host} takes no sorting rules: nothing answers ManageSieve on port {PORT}"
                )),
                _ => offline(&e),
            })?;
        let mut plain = BufReader::new(tcp);
        let (said, _) = answer(&mut plain, host).await?;
        let stream = if is_loopback(host) {
            Stream::Plain(plain.into_inner())
        } else {
            if !said
                .iter()
                .any(|line| line.first() == Some(&Word::Text("STARTTLS".into())))
            {
                return Err(Failure::Refused(format!(
                    "{host} offers sorting rules only unencrypted"
                )));
            }
            command(&mut plain, host, "STARTTLS").await?;
            let name = rustls::pki_types::ServerName::try_from(host.to_string())
                .map_err(|e| offline(&e))?;
            let tls =
                tokio::time::timeout(CONNECT_TIMEOUT, tls()?.connect(name, plain.into_inner()))
                    .await
                    .map_err(|_| Failure::Offline(format!("{host} didn't answer")))?
                    .map_err(|e| offline(&e))?;
            Stream::Tls(Box::new(tls))
        };
        let mut sieve = Sieve {
            conn: BufReader::new(stream),
            host: host.to_string(),
        };
        if !is_loopback(host) {
            // It says what it offers again, now encrypted.
            answer(&mut sieve.conn, host).await?;
        }
        let plain =
            base64::engine::general_purpose::STANDARD.encode(format!("\0{username}\0{password}"));
        sieve
            .run(&format!("AUTHENTICATE \"PLAIN\" {}", quoted(&plain)))
            .await
            .map_err(|failure| match failure {
                Failure::Refused(_) => Failure::Refused(format!(
                    "{host} didn't take the password for {username}'s sorting rules"
                )),
                offline => offline,
            })?;
        Ok(sieve)
    }

    /// The account's scripts, and whether each is the one the server runs.
    pub async fn scripts(&mut self) -> Result<Vec<(String, bool)>, Failure> {
        let listed = self.run("LISTSCRIPTS").await?;
        Ok(listed
            .into_iter()
            .filter_map(|line| {
                let mut words = line.into_iter();
                let Some(Word::Text(name)) = words.next() else {
                    return None;
                };
                let active = words
                    .any(|word| matches!(&word, Word::Atom(a) if a.eq_ignore_ascii_case("ACTIVE")));
                Some((name, active))
            })
            .collect())
    }

    /// A script's text, or `None` when there's none by that name.
    pub async fn get(&mut self, name: &str) -> Result<Option<String>, Failure> {
        let line = format!("GETSCRIPT {}", quoted(name));
        write(&mut self.conn, &self.host, &line).await?;
        let (said, ended) = answer(&mut self.conn, &self.host).await?;
        if !ended.ok {
            if ended.code.eq_ignore_ascii_case("NONEXISTENT") {
                return Ok(None);
            }
            return Err(no(&self.host, ended));
        }
        Ok(said.into_iter().flatten().find_map(|word| match word {
            Word::Text(text) => Some(text),
            _ => None,
        }))
    }

    /// Puts a script, in place of one by that name. The server checks it
    /// first, and says what's wrong with one it won't run.
    pub async fn put(&mut self, name: &str, script: &str) -> Result<(), Failure> {
        let line = format!(
            "PUTSCRIPT {} {{{}+}}\r\n{script}",
            quoted(name),
            script.len()
        );
        self.run(&line).await.map(|_| ())
    }

    /// Makes `name` the script the server runs.
    pub async fn activate(&mut self, name: &str) -> Result<(), Failure> {
        self.run(&format!("SETACTIVE {}", quoted(name)))
            .await
            .map(|_| ())
    }

    /// Takes a script off the server. The one it runs can't be.
    pub async fn delete(&mut self, name: &str) -> Result<(), Failure> {
        self.run(&format!("DELETESCRIPT {}", quoted(name)))
            .await
            .map(|_| ())
    }

    /// Says goodbye. What it answers doesn't matter.
    pub async fn log_out(mut self) {
        let _ = self.run("LOGOUT").await;
    }

    async fn run(&mut self, line: &str) -> Result<Vec<Vec<Word>>, Failure> {
        command(&mut self.conn, &self.host, line).await
    }
}

/// Sends a command and reads its answer: what it said before OK, or why not.
async fn command<S: AsyncRead + AsyncWrite + Unpin>(
    conn: &mut BufReader<S>,
    host: &str,
    line: &str,
) -> Result<Vec<Vec<Word>>, Failure> {
    write(conn, host, line).await?;
    let (said, ended) = answer(conn, host).await?;
    if ended.ok {
        Ok(said)
    } else {
        Err(no(host, ended))
    }
}

async fn write<S: AsyncRead + AsyncWrite + Unpin>(
    conn: &mut BufReader<S>,
    host: &str,
    line: &str,
) -> Result<(), Failure> {
    let stream = conn.get_mut();
    stream
        .write_all(format!("{line}\r\n").as_bytes())
        .await
        .map_err(|e| Failure::Offline(format!("{host}: {e}")))?;
    stream
        .flush()
        .await
        .map_err(|e| Failure::Offline(format!("{host}: {e}")))
}

/// A NO in words. One to try again later is offline; the rest refused.
fn no(host: &str, ended: Ended) -> Failure {
    let text = if ended.text.is_empty() {
        format!("{host} said no to a sorting rule")
    } else {
        ended.text.trim().to_string()
    };
    if ended.code.eq_ignore_ascii_case("TRYLATER") {
        Failure::Offline(text)
    } else {
        Failure::Refused(text)
    }
}

/// The lines of an answer up to its OK, NO or BYE, and how it ended.
async fn answer<S: AsyncRead + Unpin>(
    conn: &mut BufReader<S>,
    host: &str,
) -> Result<(Vec<Vec<Word>>, Ended), Failure> {
    let mut said = Vec::new();
    loop {
        let line = read_line(conn, host).await?;
        let status = match line.first() {
            Some(Word::Atom(atom)) => atom.to_ascii_uppercase(),
            _ => String::new(),
        };
        if matches!(status.as_str(), "OK" | "NO" | "BYE") {
            let code = line.iter().find_map(|word| match word {
                Word::Code(code) => Some(code.split_whitespace().next().unwrap_or("").to_string()),
                _ => None,
            });
            let text = line.iter().skip(1).find_map(|word| match word {
                Word::Text(text) => Some(text.clone()),
                _ => None,
            });
            if status == "BYE" {
                return Err(Failure::Offline(format!(
                    "{host} closed the connection{}",
                    text.map(|t| format!(": {t}")).unwrap_or_default()
                )));
            }
            let ended = Ended {
                ok: status == "OK",
                code: code.unwrap_or_default(),
                text: text.unwrap_or_default(),
            };
            return Ok((said, ended));
        }
        said.push(line);
    }
}

/// One line of an answer, with the literals in it read whole.
async fn read_line<S: AsyncRead + Unpin>(
    conn: &mut BufReader<S>,
    host: &str,
) -> Result<Vec<Word>, Failure> {
    let broken = |e: &dyn std::fmt::Display| Failure::Offline(format!("{host}: {e}"));
    let mut words = Vec::new();
    loop {
        let mut raw = Vec::new();
        let read = conn
            .read_until(b'\n', &mut raw)
            .await
            .map_err(|e| broken(&e))?;
        if read == 0 {
            return Err(Failure::Offline(format!("{host} closed the connection")));
        }
        let text = String::from_utf8_lossy(&raw);
        let text = text.trim_end_matches(['\r', '\n']);
        let (head, literal) = literal_at_end(text);
        words.extend(words_of(head));
        let Some(length) = literal else {
            return Ok(words);
        };
        if length > LONGEST {
            return Err(Failure::Offline(format!(
                "{host} sent a script too long to read"
            )));
        }
        let mut bytes = vec![0; length];
        conn.read_exact(&mut bytes).await.map_err(|e| broken(&e))?;
        words.push(Word::Text(String::from_utf8_lossy(&bytes).into_owned()));
    }
}

/// A line's text before the literal it ends with (`{12}` or `{12+}`), and
/// the literal's length.
fn literal_at_end(line: &str) -> (&str, Option<usize>) {
    let Some(open) = line.strip_suffix('}').and_then(|rest| rest.rfind('{')) else {
        return (line, None);
    };
    let digits = line[open + 1..line.len() - 1].trim_end_matches('+');
    match digits.parse() {
        Ok(length) => (&line[..open], Some(length)),
        Err(_) => (line, None),
    }
}

/// The words of a line: atoms, quoted strings and response codes.
fn words_of(line: &str) -> Vec<Word> {
    let mut words = Vec::new();
    let mut chars = line.chars().peekable();
    while let Some(&c) = chars.peek() {
        match c {
            ' ' => {
                chars.next();
            }
            '"' => {
                chars.next();
                let mut text = String::new();
                while let Some(c) = chars.next() {
                    match c {
                        '\\' => text.extend(chars.next()),
                        '"' => break,
                        c => text.push(c),
                    }
                }
                words.push(Word::Text(text));
            }
            '(' => {
                chars.next();
                let mut code = String::new();
                let mut quoting = false;
                for c in chars.by_ref() {
                    match c {
                        '"' => quoting = !quoting,
                        ')' if !quoting => break,
                        c => code.push(c),
                    }
                }
                words.push(Word::Code(code));
            }
            _ => {
                let mut atom = String::new();
                while let Some(&c) = chars.peek() {
                    if c == ' ' {
                        break;
                    }
                    atom.push(c);
                    chars.next();
                }
                words.push(Word::Atom(atom));
            }
        }
    }
    words
}

/// A string as ManageSieve quotes it.
fn quoted(text: &str) -> String {
    format!("\"{}\"", text.replace('\\', "\\\\").replace('"', "\\\""))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_reads_the_words_of_an_answer() {
        assert_eq!(
            words_of(r#"NO (NONEXISTENT) "There is no \"script\" by that name.""#),
            vec![
                Word::Atom("NO".into()),
                Word::Code("NONEXISTENT".into()),
                Word::Text(r#"There is no "script" by that name."#.into()),
            ]
        );
        assert_eq!(
            words_of(r#""postrider" ACTIVE"#),
            vec![Word::Text("postrider".into()), Word::Atom("ACTIVE".into())]
        );
        assert_eq!(literal_at_end("{42}"), ("", Some(42)));
        assert_eq!(literal_at_end(r#"NO {7+}"#), ("NO ", Some(7)));
        assert_eq!(literal_at_end(r#""a {b}""#), (r#""a {b}""#, None));
    }

    #[tokio::test]
    async fn it_reads_a_script_sent_as_a_literal() {
        let sent = b"{19}\r\nrequire \"fileinto\";\r\nOK \"Getscript completed.\"\r\n";
        let mut conn = BufReader::new(&sent[..]);
        let (said, ended) = answer(&mut conn, "styx").await.unwrap();
        assert!(ended.ok);
        assert_eq!(said, vec![vec![Word::Text("require \"fileinto\";".into())]]);
    }

    #[test]
    fn it_quotes_what_it_sends() {
        assert_eq!(quoted(r#"a "b" \c"#), r#""a \"b\" \\c""#);
    }
}
