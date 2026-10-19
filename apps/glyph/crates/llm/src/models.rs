//! The models the server may use, from a TOML file the operator writes
//! (`GLYPH_MODELS`, `models.toml` by default). See `models.example.toml`.

use std::path::Path;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Models {
    /// The program that serves a model, for the ones the server starts
    /// itself. Looked up on `PATH` unless it's a path.
    #[serde(default = "llama_server")]
    pub llm_binary: String,
    #[serde(default)]
    pub models: Vec<Model>,
}

fn llama_server() -> String {
    "llama-server".into()
}

/// One model behind an HTTP endpoint.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Model {
    /// What language pairs call it, and what translations say wrote them.
    pub name: String,
    pub endpoint: String,
    /// OpenAI-style `/v1/chat/completions`, which llama-server offers for
    /// every model; `false` uses the raw `/completion`.
    #[serde(default = "yes")]
    pub chat_mode: bool,
    /// A local GGUF file, for starting the server with `-m`.
    #[serde(default)]
    pub model_path: String,
    /// A Hugging Face repository and file, for `--hf-repo` / `--hf-file`.
    #[serde(default)]
    pub hf_repo: String,
    #[serde(default)]
    pub hf_file: String,
    /// More arguments for the server, such as `["-c", "4096"]`.
    #[serde(default)]
    pub extra_args: Vec<String>,
}

fn yes() -> bool {
    true
}

impl Default for Model {
    fn default() -> Model {
        Model {
            name: String::new(),
            endpoint: String::new(),
            chat_mode: true,
            model_path: String::new(),
            hf_repo: String::new(),
            hf_file: String::new(),
            extra_args: Vec::new(),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ModelsError {
    #[error("couldn't read {path}: {source}")]
    Read {
        path: String,
        source: std::io::Error,
    },
    #[error("{path} isn't a valid models file: {source}")]
    Parse {
        path: String,
        source: Box<toml::de::Error>,
    },
    #[error("{0}")]
    Invalid(String),
}

impl Models {
    /// Reads the file; a missing file means no models.
    pub fn load(path: &Path) -> Result<Models, ModelsError> {
        let text = match std::fs::read_to_string(path) {
            Ok(text) => text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Models {
                    llm_binary: llama_server(),
                    models: Vec::new(),
                });
            }
            Err(source) => {
                return Err(ModelsError::Read {
                    path: path.display().to_string(),
                    source,
                });
            }
        };
        Models::parse(&text).map_err(|e| match e {
            ModelsError::Parse { source, .. } => ModelsError::Parse {
                path: path.display().to_string(),
                source,
            },
            other => other,
        })
    }

    pub fn parse(text: &str) -> Result<Models, ModelsError> {
        let models: Models = toml::from_str(text).map_err(|source| ModelsError::Parse {
            path: "the models file".into(),
            source: Box::new(source),
        })?;
        for (i, m) in models.models.iter().enumerate() {
            if m.name.trim().is_empty() || m.endpoint.trim().is_empty() {
                return Err(ModelsError::Invalid(format!(
                    "model {} needs a name and an endpoint",
                    i + 1
                )));
            }
            if models.models[..i].iter().any(|o| o.name == m.name) {
                return Err(ModelsError::Invalid(format!("two models are called {:?}", m.name)));
            }
        }
        Ok(models)
    }

    /// The model called `name`, or the first one when `name` is empty or
    /// names none.
    pub fn pick(&self, name: &str) -> Option<&Model> {
        self.find(name).or_else(|| self.models.first())
    }

    pub fn find(&self, name: &str) -> Option<&Model> {
        self.models.iter().find(|m| m.name == name)
    }
}

impl Model {
    /// Whether the server can start this model itself.
    pub fn can_start(&self) -> bool {
        self.downloads() || !self.model_path.trim().is_empty()
    }

    fn downloads(&self) -> bool {
        !self.hf_repo.trim().is_empty() && !self.hf_file.trim().is_empty()
    }

    /// The arguments that start it, or why it can't be started.
    pub fn start_args(&self) -> Result<Vec<String>, String> {
        let (repo, file) = (self.hf_repo.trim(), self.hf_file.trim());
        let mut args: Vec<String> = if !repo.is_empty() || !file.is_empty() {
            if repo.is_empty() || file.is_empty() {
                return Err("Set both hf_repo and hf_file, or neither and model_path.".into());
            }
            vec!["--hf-repo".into(), repo.into(), "--hf-file".into(), file.into()]
        } else if !self.model_path.trim().is_empty() {
            vec!["-m".into(), self.model_path.trim().into()]
        } else {
            return Err(
                "Set model_path to a GGUF file, or hf_repo and hf_file, to start it from here."
                    .into(),
            );
        };
        args.extend(["--port".into(), self.port()?.to_string()]);
        args.extend(self.extra_args.iter().cloned());
        Ok(args)
    }

    /// The port in the endpoint, or the scheme's.
    fn port(&self) -> Result<u16, String> {
        let bad = || format!("The endpoint {:?} isn't a URL.", self.endpoint);
        let (scheme, rest) = self.endpoint.trim().split_once("://").ok_or_else(bad)?;
        let host = rest.split('/').next().unwrap_or_default();
        match host.rsplit_once(':') {
            Some((_, port)) if !port.ends_with(']') => port.parse().map_err(|_| bad()),
            _ if scheme == "https" => Ok(443),
            _ => Ok(80),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FILE: &str = r#"
[[models]]
name = "viking"
endpoint = "http://localhost:8080"
model_path = "./models/viking.gguf"
extra_args = ["-c", "4096"]

[[models]]
name = "eurollm"
endpoint = "http://127.0.0.1:8081"
hf_repo = "utter-project/EuroLLM-1.7B-Instruct-GGUF"
hf_file = "model.gguf"
chat_mode = false
"#;

    #[test]
    fn a_models_file_reads_with_defaults() {
        let models = Models::parse(FILE).unwrap();
        assert_eq!(models.llm_binary, "llama-server");
        assert_eq!(models.models.len(), 2);
        assert!(models.models[0].chat_mode);
        assert!(!models.models[1].chat_mode);
        assert_eq!(models.pick("eurollm").unwrap().name, "eurollm");
        assert_eq!(models.pick("").unwrap().name, "viking");
        assert_eq!(models.pick("gone").unwrap().name, "viking");
    }

    #[test]
    fn start_args_follow_the_model() {
        let models = Models::parse(FILE).unwrap();
        assert_eq!(
            models.models[0].start_args().unwrap(),
            ["-m", "./models/viking.gguf", "--port", "8080", "-c", "4096"]
        );
        assert_eq!(
            models.models[1].start_args().unwrap(),
            [
                "--hf-repo",
                "utter-project/EuroLLM-1.7B-Instruct-GGUF",
                "--hf-file",
                "model.gguf",
                "--port",
                "8081"
            ]
        );
        let remote = Model {
            name: "remote".into(),
            endpoint: "http://gpu.local:9000".into(),
            ..Model::default()
        };
        assert!(!remote.can_start());
        assert!(remote.start_args().is_err());
    }

    #[test]
    fn bad_files_are_refused() {
        assert!(Models::parse("[[models]]\nname = \"a\"").is_err());
        assert!(Models::parse("surprise = 1").is_err());
        let twice = "[[models]]\nname = \"a\"\nendpoint = \"http://x\"\n[[models]]\nname = \"a\"\nendpoint = \"http://y\"";
        assert!(Models::parse(twice).is_err());
        assert!(Models::load(Path::new("/nonexistent/models.toml")).unwrap().models.is_empty());
    }

    #[test]
    fn the_example_file_reads() {
        let example = Models::parse(include_str!("../../../models.example.toml")).unwrap();
        assert_eq!(example.models[0].name, "viking");
        assert!(example.models[0].can_start());
    }
}
